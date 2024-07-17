# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# crdump.py - dump changesets information to filesystem
#
from __future__ import absolute_import

import json
import re
import shutil
import tempfile
from os import path

from sapling import encoding, error, extensions, phases, registrar, scmutil
from sapling.i18n import _
from sapling.node import hex

from . import commitcloud


DIFFERENTIAL_REGEX = re.compile(
    "Differential Revision: http.+?/"  # Line start, URL
    "D(?P<id>[0-9]+)"  # Differential ID, just numeric part
)
cmdtable = {}
command = registrar.command(cmdtable)


@command(
    "debugcrdump",
    [
        ("r", "rev", [], _("revisions to dump")),
        # We use 1<<15 for "as much context as possible"
        ("U", "unified", 1 << 15, _("number of lines of context to show"), _("NUM")),
        ("l", "lfs", False, "Provide sha256 for lfs files instead of dumping"),
        ("", "obsolete", False, "add obsolete markers related to the given revisions"),
        ("", "nobinary", False, "do not dump binary files"),
    ],
    _("@prog@ debugcrdump [OPTION]... [-r] [REV]"),
)
def crdump(ui, repo, *revs, **opts):
    """
    Dump the info about the revisions in format that's friendly for sending the
    patches for code review.

    The output is a JSON list with dictionary for each specified revision: ::

        {
          "output_directory": an output directory for all temporary files
          "commits": [
          {
            "node": commit hash,
            "date": date in format [unixtime, timezone offset],
            "desc": commit message,
            "patch_file": path to file containing patch in unified diff format
                          relative to output_directory,
            "commit_cloud": true if the commit is in commit cloud,
            "files": list of files touched by commit,
            "binary_files": [
              {
                "filename": path to file relative to repo root,
                "old_file": path to file (relative to output_directory) with
                            a dump of the old version of the file,
                "new_file": path to file (relative to output_directory) with
                            a dump of the newversion of the file,
              },
              ...
            ],
            "user": commit author,
            "p1": {
              "node": hash,
              "differential_revision": xxxx
            },
            "public_base": {
              "node": public base commit hash,
              "svnrev": svn revision of public base (if hgsvn repo),
            },
            "obsolete": {
                "date": [
                    time,
                    timezone
                ],
                "flag": marker's flags,
                "metadata": {
                    "operation": changes made,
                    "user": user name
                },
                "prednode": predecessor commit in hash,
                "succnodes": [
                    successors in hash
                ]
            }
          },
          ...
          ]
        }
    """

    revs = list(revs)
    revs.extend(opts["rev"])

    if not revs:
        raise error.Abort(_("revisions must be specified"))
    revs = scmutil.revrange(repo, revs)

    if "unified" in opts:
        contextlines = opts["unified"]

    cdata = []
    outdir = tempfile.mkdtemp(suffix="hg.crdump")
    try:
        lfs = None
        if opts["lfs"]:
            try:
                lfs = extensions.find("lfs")
            except KeyError:
                pass  # lfs extension is not enabled

        # notbackedup is a revset
        notbackedup = revs
        if ui.configbool("crdump", "commitcloud", False):
            try:
                oldquiet = repo.ui.quiet
                # Silence any output from commitcloud
                repo.ui.quiet = True
                _backedup, notbackedup = commitcloud.upload.upload(repo, revs)
            except Exception:
                if ui.configbool("crdump", "commitcloudrequired"):
                    raise
                # Don't let commit cloud exceptions block crdump
                pass
            finally:
                repo.ui.quiet = oldquiet

        for rev in revs:
            ctx = repo[rev]
            rdata = {
                "node": hex(ctx.node()),
                "date": list(map(int, ctx.date())),
                "desc": encoding.fromlocal(ctx.description()),
                "files": ctx.files(),
                "p1": {"node": ctx.p1().hex()},
                "user": encoding.fromlocal(ctx.user()),
                "bookmarks": list(map(encoding.fromlocal, ctx.bookmarks())),
                "commit_cloud": False if ctx.rev() in notbackedup else True,
                "manifest_node": hex(ctx.manifestnode()),
            }
            if ctx.p1().phase() != phases.public:
                # we need this only if parent is in the same draft stack
                rdata["p1"]["differential_revision"] = phabricatorrevision(ctx.p1())

            rdata["branch"] = ""

            pbctx = publicbase(repo, ctx)
            if pbctx:
                rdata["public_base"] = {"node": hex(pbctx.node())}
                try:
                    globalrevs = extensions.find("globalrevs")
                    globalrev = globalrevs.getglobalrev(ui, pbctx)
                    rdata["public_base"]["svnrev"] = globalrev
                except KeyError:
                    pass

                if extensions.isenabled(ui, "remotenames"):
                    downstreams = repo.revs(
                        "sort(%n:: & remotebookmark())", pbctx.node()
                    )
                    downstreambookmarks = []
                    for r in downstreams:
                        downstreambookmarks.extend(
                            repo.names["hoistednames"].names(repo, repo[r].node())
                        )

                    # Caveat: In Sapling it's impossible to know for certain which
                    # remote bookmark a local commit was made against. The best we
                    # can do is a heuristic.  The heuristicis as follows:
                    #   1. If 'master' is in downstreambookmarks, then use it.
                    #   2. Otherwise report the first bookmark as the current branch.
                    #      For draft commit, this should be (best guess) the remote
                    #      bookmark on which the draft commit was based if user didn't
                    #      run `pull` from remote server.
                    if downstreambookmarks:
                        if "master" in downstreambookmarks:
                            rdata["branch"] = "master"
                        else:
                            rdata["branch"] = downstreambookmarks[0]

            rdata["patch_file"] = dumppatch(ui, repo, ctx, outdir, contextlines)
            if not opts["nobinary"]:
                rdata["binary_files"] = dumpbinaryfiles(ui, repo, ctx, outdir, lfs)
            cdata.append(rdata)

        ui.write(
            json.dumps(
                {"output_directory": outdir, "commits": cdata},
                sort_keys=True,
                indent=4,
                separators=(",", ": "),
            )
        )
        ui.write("\n")
    except Exception:
        shutil.rmtree(outdir)
        raise


def dumppatch(ui, repo, ctx, outdir, contextlines):
    chunks = ctx.diff(git=True, unified=contextlines, binary=False)
    patchfile = "%s.patch" % hex(ctx.node())
    with open(path.join(outdir, patchfile), "wb") as f:
        for chunk in chunks:
            f.write(chunk)
    return patchfile


def dumpfctx(outdir, fctx):
    outfile = "%s" % hex(fctx.filenode())
    writepath = path.join(outdir, outfile)
    if not path.isfile(writepath):
        with open(writepath, "wb") as f:
            f.write(fctx.data())
    return outfile


def _getfilesizeandsha256(flog, ctx, fname, lfs):
    try:
        fnode = ctx.filenode(fname)
    except error.ManifestLookupError:
        return None, None

    if lfs.wrapper._islfs(flog, node=fnode):  # if file was uploaded to lfs
        rawtext = flog.revision(fnode, raw=True)
        gitlfspointer = lfs.pointer.deserialize(rawtext)
        sha256_hash = gitlfspointer.oid()
        filesize = gitlfspointer.size()
        return sha256_hash, filesize
    return None, None


def dumpbinaryfiles(ui, repo, ctx, outdir, lfs):
    binaryfiles = []
    pctx = ctx.p1()
    with ui.configoverride({("remotefilelog", "dolfsprefetch"): False}):
        for fname in ctx.files():
            oldfile = newfile = None
            dump = False
            oldfilesha256 = newfilesha256 = None
            oldfilesize = newfilesize = None
            if lfs is not None:
                flog = repo.file(fname)
                newfilesha256, newfilesize = _getfilesizeandsha256(
                    flog, ctx, fname, lfs
                )
                oldfilesha256, oldfilesize = _getfilesizeandsha256(
                    flog, pctx, fname, lfs
                )
            # if one of the versions is binary file which is not in lfs
            # the whole change will show up as binary in diff output
            if not newfilesha256:
                fctx = ctx[fname] if fname in ctx else None
                if fctx and fctx.isbinary():
                    dump = True

            if not oldfilesha256:
                pfctx = pctx[fname] if fname in pctx else None
                if pfctx and pfctx.isbinary():
                    dump = True

            if dump:
                if not newfilesha256 and fctx:
                    newfile = dumpfctx(outdir, fctx)
                if not oldfilesha256 and pfctx:
                    oldfile = dumpfctx(outdir, pfctx)

            if lfs is None and dump:
                binaryfiles.append(
                    {"file_name": fname, "old_file": oldfile, "new_file": newfile}
                )
            elif newfile or newfilesha256 or oldfile or oldfilesha256:
                binaryfiles.append(
                    {
                        "file_name": fname,
                        "old_file": oldfile,
                        "new_file": newfile,
                        "new_file_sha256": newfilesha256,
                        "old_file_sha256": oldfilesha256,
                        "new_file_size": newfilesize,
                        "old_file_size": oldfilesize,
                    }
                )

    return binaryfiles


def phabricatorrevision(ctx):
    match = DIFFERENTIAL_REGEX.search(ctx.description())
    return match.group(1) if match else ""


def publicbase(repo, ctx):
    base = repo.revs("max(::%d & public())", ctx.rev())
    if len(base):
        return repo[base.first()]
    return None


def dumpmarkers(rawmarkers):
    markers = []
    for rm in rawmarkers:
        marker = {
            "date": rm.date(),
            "flag": rm.flags(),
            "metadata": rm.metadata(),
            "prednode": hex(rm.prednode()),
        }
        if rm.succnodes():
            marker["succnodes"] = list(map(hex, rm.succnodes()))
        if rm.parentnodes():
            marker["parents"] = list(map(hex, rm.parentnodes()))

        markers.append(marker)

    return markers
