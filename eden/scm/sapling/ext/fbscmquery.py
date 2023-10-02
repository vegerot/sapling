# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# scmquery.py
# An extension to augement hg with information obtained from SCMQuery

import re
from typing import Any, List, Pattern

from sapling import (
    extensions,
    namespaces,
    node,
    registrar,
    revset,
    smartset,
    templater,
    ui as uimod,
)
from sapling.i18n import _, _x
from sapling.namespaces import namespace
from sapling.node import bin

from .extlib.phabricator import arcconfig, graphql


cmdtable = {}
command = registrar.command(cmdtable)

configtable = {}
configitem = registrar.configitem(configtable)
configitem("fbscmquery", "auto-username", "true")

namespacepredicate = registrar.namespacepredicate()

DEFAULT_TIMEOUT = 60
MAX_CONNECT_RETRIES = 3


githashre: Pattern[str] = re.compile(r"g([0-9a-f]{40})")
svnrevre: Pattern[str] = re.compile(r"^r[A-Z]+(\d+)$")
phabhashre: Pattern[str] = re.compile(r"^r([A-Z]+)([0-9a-f]{12,40})$")


def uisetup(ui) -> None:
    def _globalrevswrapper(loaded):
        if loaded:
            globalrevsmod = extensions.find("globalrevs")
            extensions.wrapfunction(
                globalrevsmod, "_lookupglobalrev", _scmquerylookupglobalrev
            )

    if ui.configbool("globalrevs", "scmquerylookup") and not ui.configbool(
        "globalrevs", "edenapilookup"
    ):
        extensions.afterloaded("globalrevs", _globalrevswrapper)

    revset.symbols["gitnode"] = gitnode
    gitnode._weight = 10

    if ui.configbool("fbscmquery", "auto-username"):

        def _auto_username(orig, ui):
            try:
                client = graphql.Client(ui=ui)
                return client.get_username()
            except Exception:
                return None

        extensions.wrapfunction(uimod, "_auto_username", _auto_username)


@templater.templatefunc("mirrornode")
def mirrornode(ctx, mapping, args):
    """template: find this commit in other repositories"""

    reponame = mapping["repo"].ui.config("fbscmquery", "reponame")
    if not reponame:
        # We don't know who we are, so we can't ask for a translation
        return ""

    if mapping["ctx"].mutable():
        # Local commits don't have translations
        return ""

    node = mapping["ctx"].hex()
    args = [f(ctx, mapping, a) for f, a in args]
    if len(args) == 1:
        torepo, totype = reponame, args[0]
    else:
        torepo, totype = args

    try:
        client = graphql.Client(repo=mapping["repo"])
        return client.getmirroredrev(reponame, "hg", torepo, totype, node)
    except arcconfig.ArcConfigError:
        mapping["repo"].ui.warn(_("couldn't read .arcconfig or .arcrc\n"))
        return ""
    except graphql.ClientError as e:
        mapping["repo"].ui.warn(_x(str(e) + "\n"))
        return ""


templatekeyword = registrar.templatekeyword()


@templatekeyword("gitnode")
def showgitnode(repo, ctx, templ, **args):
    """Return the git revision corresponding to a given hg rev"""
    # Try reading from commit extra first.
    extra = ctx.extra()
    if "hg-git-rename-source" in extra:
        hexnode = extra.get("convert_revision")
        if hexnode:
            return hexnode
    reponame = repo.ui.config("fbscmquery", "reponame")
    if not reponame:
        # We don't know who we are, so we can't ask for a translation
        return ""
    backingrepos = repo.ui.configlist("fbscmquery", "backingrepos", default=[reponame])

    if ctx.mutable():
        # Local commits don't have translations
        return ""

    matches = []
    for backingrepo in backingrepos:
        try:
            client = graphql.Client(repo=repo)
            githash = client.getmirroredrev(
                reponame, "hg", backingrepo, "git", ctx.hex()
            )
            if githash != "":
                matches.append((backingrepo, githash))
        except (graphql.ClientError, arcconfig.ArcConfigError):
            pass

    if len(matches) == 0:
        return ""
    elif len(backingrepos) == 1:
        return matches[0][1]
    else:
        # in case it's not clear, the sort() is to ensure the output is in a
        # deterministic order.
        matches.sort()
        return "; ".join(["{0}: {1}".format(*match) for match in matches])


def gitnode(repo, subset, x):
    """``gitnode(id)``
    Return the hg revision corresponding to a given git rev."""
    l = revset.getargs(x, 1, 1, _("id requires one argument"))
    n = revset.getstring(l[0], _("id requires a string"))

    reponame = repo.ui.config("fbscmquery", "reponame")
    if not reponame:
        # We don't know who we are, so we can't ask for a translation
        return smartset.baseset([], repo=repo)
    backingrepos = repo.ui.configlist("fbscmquery", "backingrepos", default=[reponame])

    lasterror = None
    hghash = None
    for backingrepo in backingrepos:
        try:
            client = graphql.Client(repo=repo)
            hghash = client.getmirroredrev(backingrepo, "git", reponame, "hg", n)
            if hghash != "":
                break
        except Exception as ex:
            lasterror = ex

    if not hghash:
        if lasterror:
            repo.ui.warn(
                ("Could not translate revision {0}: {1}\n".format(n, lasterror))
            )
        else:
            repo.ui.warn(_x("Could not translate revision {0}\n".format(n)))
        # If we don't have a valid hg hash, return an empty set
        return smartset.baseset([], repo=repo)

    rn = repo[node.bin(hghash)].rev()
    return subset & smartset.baseset([rn], repo=repo)


@namespacepredicate("conduit", priority=70)
def _getnamespace(_repo) -> namespace:
    return namespaces.namespace(
        listnames=lambda repo: [], namemap=_phablookup, nodemap=lambda repo, node: []
    )


def _phablookup(repo: "Any", phabrev: str) -> "List[bytes]":
    # Is the given revset a phabricator hg hash (ie: rHGEXTaaacb34aacb34aa)

    def gittohg(githash):
        return list(repo.nodes("gitnode(%s)" % githash))

    phabmatch = phabhashre.match(phabrev)
    if phabmatch:
        phabrepo = phabmatch.group(1)
        phabhash = phabmatch.group(2)

        # The hash may be a git hash
        if phabrepo in repo.ui.configlist("fbscmquery", "gitcallsigns", []):
            return gittohg(phabhash)

        return [repo[phabhash].node()]

    # TODO: 's/svnrev/globalrev' after turning off Subversion servers. We will
    # know about this when we remove the `svnrev` revset.
    svnrevmatch = svnrevre.match(phabrev)
    if svnrevmatch is not None:
        svnrev = svnrevmatch.group(1)
        return list(repo.nodes("svnrev(%s)" % svnrev))

    m = githashre.match(phabrev)
    if m is not None:
        githash = m.group(1)
        if len(githash) == 40:
            return gittohg(githash)

    return []


def _scmquerylookupglobalrev(orig, repo, rev):
    reponame = repo.ui.config("fbscmquery", "reponame")
    if reponame:
        try:
            client = graphql.Client(repo=repo)
            hghash = str(
                client.getmirroredrev(reponame, "GLOBAL_REV", reponame, "hg", str(rev))
            )
            matchedrevs = []
            if hghash:
                matchedrevs.append(bin(hghash))
            return matchedrevs
        except Exception as exc:
            repo.ui.warn(
                _("failed to lookup globalrev %s from scmquery: %s\n") % (rev, exc)
            )

    return orig(repo, rev)


@command(
    "debuginternusername",
    [("u", "unixname", "", _("unixname to lookup"))],
    norepo=True,
)
def debuginternusername(ui, **opts):
    client = graphql.Client(ui=ui)
    unixname = opts.get("unixname") or None
    name = client.get_username(unixname=unixname)
    ui.write("%s\n" % name)
