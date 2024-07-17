# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# shallowrepo.py - shallow repository that uses remote filelogs
from __future__ import absolute_import

from sapling import match, progress, util
from sapling.i18n import _
from sapling.node import hex, nullid

from . import fileserverclient, remotefilectx, remotefilelog


requirement = "remotefilelog"


def wraprepo(repo) -> None:
    class shallowrepository(repo.__class__):
        @util.propertycache
        def name(self):
            return self.ui.config("remotefilelog", "reponame", "unknown")

        @util.propertycache
        def fallbackpath(self):
            path = self.ui.config(
                "remotefilelog",
                "fallbackpath",
                # fallbackrepo is the old, deprecated name
                self.ui.config(
                    "remotefilelog", "fallbackrepo", self.ui.config("paths", "default")
                ),
            )
            if not path:
                # raise AttributeError insteal of error.Abort, so `getattr(repo, "fallbackpath", None)`
                # will not break
                raise AttributeError("fallbackpath")

            return path

        @util.propertycache
        def fileslog(self):
            return remotefilelog.remotefileslog(self)

        def maybesparsematch(self, *revs, **kwargs):
            """
            A wrapper that allows the remotefilelog to invoke sparsematch() if
            this is a sparse repository, or returns None if this is not a
            sparse repository.
            """
            if hasattr(self, "sparsematch"):
                return self.sparsematch(*revs, **kwargs)

            return None

        def file(self, f):
            if f[0] == "/":
                f = f[1:]

            return remotefilelog.remotefilelog(self.svfs, f, self)

        def filectx(self, path, changeid=None, fileid=None):
            return remotefilectx.remotefilectx(self, path, changeid, fileid)

        def close(self):
            result = super(shallowrepository, self).close()
            if "fileslog" in self.__dict__:
                self.fileslog.abortpending()
            return result

        def commitpending(self):
            super(shallowrepository, self).commitpending()

        def commitctx(self, ctx, error=False):
            """Add a new revision to current repository.
            Revision information is passed via the context argument.
            """

            # some contexts already have manifest nodes, they don't need any
            # prefetching (for example if we're just editing a commit message
            # we can reuse manifest
            if not ctx.manifestnode():
                # prefetch files that will likely be compared
                m1 = ctx.p1().manifest()
                files = []
                for f in ctx.modified() + ctx.added():
                    fparent1 = m1.get(f, nullid)
                    if fparent1 != nullid:
                        files.append((f, fparent1))
                self.fileservice.prefetch(files)
            return super(shallowrepository, self).commitctx(ctx, error=error)

        def backgroundprefetch(self, revs, base=None, pats=None, opts=None):
            """Runs prefetch in background"""
            cmd = [util.hgexecutable(), "-R", self.origroot, "prefetch"]
            if revs:
                cmd += ["-r", revs]
            if base:
                cmd += ["-b", base]

            util.spawndetached(cmd)

        def prefetch(self, revs, base=None, matcher=None):
            """Prefetches all the necessary file revisions for the given revs"""
            with self._lock(
                self.svfs,
                "prefetchlock",
                True,
                None,
                None,
                _("prefetching in %s") % self.origroot,
            ):
                self._prefetch(revs, base, matcher)

        def _prefetch(self, revs, base=None, matcher=None):
            # Copy the skip set to start large and avoid constant resizing,
            # and since it's likely to be very similar to the prefetch set.

            if len(revs) > 1:
                files = set()
            else:
                files = []

            basemf = self[base or nullid].manifest()
            with progress.bar(self.ui, _("prefetching"), total=len(revs)) as prog:
                for rev in sorted(revs):
                    ctx = self[rev]
                    if matcher is None:
                        matcher = self.maybesparsematch(rev)

                    mfctx = ctx.manifestctx()
                    mf = mfctx.read()

                    with progress.spinner(self.ui, _("computing files")):
                        if base is None and hasattr(mf, "walkfiles"):
                            # If there is no base, skip diff and use more efficient walk.
                            walked = mf.walkfiles(matcher)
                            if type(files) is set:
                                files.update(walked)
                            elif type(files) is list:
                                # we know len(revs) == 1, so avoid copy and assign
                                files = walked
                        else:
                            for path, (new, _old) in mf.diff(basemf, matcher).items():
                                if new[0]:
                                    if type(files) is set:
                                        files.add((path, new[0]))
                                    elif type(files) is list:
                                        files.append((path, new[0]))

                    prog.value += 1

            if files:
                with progress.spinner(self.ui, _("ensuring files fetched")):
                    self.fileservice.prefetch(files, fetchhistory=False)

    repo.__class__ = shallowrepository

    repo.fileservice = fileserverclient.fileserverclient(repo)
