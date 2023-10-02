# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

import socket

import ssl
import sys
from typing import List, Optional, Sized

from sapling import mutation, pycompat, registrar, smartset, util as hgutil
from sapling.i18n import _

from .extlib.phabricator import arcconfig, diffprops, graphql


COMMITTEDSTATUS = "Committed"


def memoize(f):
    """
    NOTE: This is a hack
    if f args are like (a, b1, b2, b3) and returns [o1, o2, o3] where
    o1, o2, o3 are output of f respectively for (a, b1), (a, b2) and
    (a, b3) then we memoize f(a, b1, b2, b3)'s result but also
    f(a, b1) => o1 , f(a, b2) => o2 and f(a, b3) => o3.
    Example:

    >>> partialsum = lambda a, *b: [a + bn for bn in b]
    >>> partialsum = memoize(partialsum)

    Create a class that wraps the integer '3', otherwise we cannot add
    _phabstatuscache to it for the test
    >>> class IntWrapperClass(int):
    ...     def __new__(cls, *args, **kwargs):
    ...         return  super(IntWrapperClass, cls).__new__(cls, 3)

    >>> three = IntWrapperClass()
    >>> partialsum(three, 1, 2, 3)
    [4, 5, 6]

    As expected, we have 4 entries in the cache for a call like f(a, b, c, d)
    >>> pp(three._phabstatuscache)
    {(3, 1): [4], (3, 1, 2, 3): [4, 5, 6], (3, 2): [5], (3, 3): [6]}
    """

    def helper(*args):
        repo = args[0]
        if not hasattr(repo, "_phabstatuscache"):
            repo._phabstatuscache = {}
        if args not in repo._phabstatuscache:
            u = f(*args)
            repo._phabstatuscache[args] = u
            if isinstance(u, list):
                revs = args[1:]
                for x, r in enumerate(revs):
                    repo._phabstatuscache[(repo, r)] = [u[x]]
        return repo._phabstatuscache[args]

    return helper


def _fail(repo, diffids: Sized, *msgs) -> List[str]:
    for msg in msgs:
        repo.ui.warn(msg)
    return ["Error"] * len(diffids)


@memoize
def getdiffstatus(repo, *diffid):
    """Perform a Conduit API call to get the diff status

    Returns status of the diff"""

    if not diffid:
        return []
    timeout = repo.ui.configint("ssl", "timeout", 10)
    signalstatus = repo.ui.configbool("ssl", "signal_status", True)

    try:
        client = graphql.Client(repodir=pycompat.getcwd(), repo=repo)
        statuses = client.getrevisioninfo(timeout, signalstatus, diffid)
    except arcconfig.ArcConfigError as ex:
        msg = _(
            "arcconfig configuration problem. No diff information can be provided.\n"
        )
        hint = _("Error info: %s\n") % str(ex)
        ret = _fail(repo, diffid, msg, hint)
        return ret
    except (graphql.ClientError, ssl.SSLError, socket.timeout) as ex:
        msg = _("Error talking to phabricator. No diff information can be provided.\n")
        hint = _("Error info: %s\n") % str(ex)
        ret = _fail(repo, diffid, msg, hint)
        return ret
    except ValueError as ex:
        msg = _(
            "Error decoding GraphQL response. No diff information can be provided.\n"
        )
        hint = _("Error info: %s\n") % str(ex)
        ret = _fail(repo, diffid, msg, hint)
        return ret

    # This makes the code more robust in case we don't learn about any
    # particular revision
    result = []
    for diff in diffid:
        matchingresponse = statuses.get(str(diff))
        if not matchingresponse:
            result.append("Error")
        else:
            result.append(matchingresponse)
    return result


def populateresponseforphab(repo, diffnum) -> None:
    """:populateresponse: Runs the memoization function
    for use of phabstatus and sync status
    """
    if not hasattr(repo, "_phabstatusrevs"):
        return

    if hasattr(repo, "_phabstatuscache") and (repo, diffnum) in repo._phabstatuscache:
        # We already have cached data for this diff
        return

    next_revs = repo._phabstatusrevs.peekahead()
    if repo._phabstatusrevs.done:
        # repo._phabstatusrevs doesn't have anything else to process.
        # Remove it so we will bail out earlier next time.
        del repo._phabstatusrevs

    alldiffnumbers = [getdiffnum(repo, repo[rev]) for rev in next_revs]
    okdiffnumbers = set(d for d in alldiffnumbers if d is not None)
    # Make sure we always include the requested diff number
    okdiffnumbers.add(diffnum)
    # To populate the cache, the result will be used by the templater
    getdiffstatus(repo, *okdiffnumbers)


templatekeyword = registrar.templatekeyword()


@templatekeyword("phabstatus")
def showphabstatus(repo, ctx, templ, **args):
    """String. Return the diff approval status for a given @prog@ rev"""
    diffnum = getdiffnum(repo, ctx)
    if diffnum is None:
        return None
    populateresponseforphab(repo, diffnum)

    result = getdiffstatus(repo, diffnum)[0]
    if isinstance(result, dict) and "status" in result:
        landstatus = result.get("land_job_status")
        finalreviewstatus = result.get("needs_final_review_status")
        if landstatus == "LAND_JOB_RUNNING":
            return "Landing"
        elif landstatus == "LAND_RECENTLY_SUCCEEDED":
            return "Committing"
        elif landstatus == "LAND_RECENTLY_FAILED":
            return "Recently Failed to Land"
        elif finalreviewstatus == "NEEDED":
            return "Needs Final Review"
        else:
            return result.get("status")
    else:
        return "Error"


@templatekeyword("phabsignalstatus")
def showphabsignalstatus(repo, ctx, templ, **args):
    """String. Return the diff Signal status for a given @prog@ rev"""
    diffnum = getdiffnum(repo, ctx)
    if diffnum is None:
        return None
    populateresponseforphab(repo, diffnum)

    result = getdiffstatus(repo, diffnum)[0]
    if isinstance(result, dict):
        return result.get("signal_status")


@templatekeyword("phabcommit")
def showphabcommit(repo, ctx, templ, **args):
    """String. Return the remote commit in Phabricator
    if any
    """
    # local = ctx.hex()
    # Copied from showsyncstatus
    if not ctx.mutable():
        return None

    diffnum = getdiffnum(repo, ctx)
    if diffnum is None:
        return None

    populateresponseforphab(repo, diffnum)
    results = getdiffstatus(repo, diffnum)
    try:
        result = results[0]
        remote = result["hash"]
    except (IndexError, KeyError, ValueError, TypeError):
        # We got no result back, or it did not contain all required fields
        return None

    return remote


@templatekeyword("syncstatus")
def showsyncstatus(repo, ctx, templ, **args) -> Optional[str]:
    """String. Return whether the local revision is in sync
    with the remote (phabricator) revision
    """
    if not ctx.mutable():
        return None

    diffnum = getdiffnum(repo, ctx)
    if diffnum is None:
        return None

    populateresponseforphab(repo, diffnum)
    results = getdiffstatus(repo, diffnum)
    try:
        result = results[0]
        remote = result.get("hash")
        status = result["status"]
    except (IndexError, KeyError, ValueError, TypeError, AttributeError):
        # We got no result back, or it did not contain all required fields
        return "Error"

    local = ctx.hex()
    if local == remote:
        return "sync"
    elif status == "Committed":
        return "committed"
    else:
        return "unsync"


def getdiffnum(repo, ctx):
    return diffprops.parserevfromcommitmsg(ctx.description())


def extsetup(ui) -> None:
    smartset.prefetchtemplatekw.update(
        {
            "phabsignalstatus": ["phabstatus"],
            "phabstatus": ["phabstatus"],
            "syncstatus": ["phabstatus"],
            "phabcommit": ["phabstatus"],
        }
    )
    smartset.prefetchtable["phabstatus"] = _prefetch


def _prefetch(repo, ctxstream):
    peekahead = repo.ui.configint("phabstatus", "logpeekaheadlist", 30)
    for batch in hgutil.eachslice(ctxstream, peekahead):
        cached = getattr(repo, "_phabstatuscache", {})
        diffids = [getdiffnum(repo, ctx) for ctx in batch]
        diffids = {i for i in diffids if i is not None and i not in cached}
        if diffids:
            repo.ui.debug("prefetch phabstatus for %r\n" % sorted(diffids))
            # @memorize writes results to repo._phabstatuscache
            getdiffstatus(repo, *diffids)
        for ctx in batch:
            yield ctx
