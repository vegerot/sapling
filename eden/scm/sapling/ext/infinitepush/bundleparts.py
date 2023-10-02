# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

import errno
import os
import tempfile
from typing import Optional

from sapling import (
    bundle2,
    changegroup,
    error,
    exchange,
    extensions,
    mutation,
    perftrace,
    pycompat,
    revsetlang,
    util,
)
from sapling.bundle2 import bundlepart
from sapling.i18n import _

from . import bookmarks, constants, server


def uisetup(ui) -> None:
    bundle2.capabilities[constants.scratchbranchparttype] = ()
    bundle2.capabilities[constants.scratchbookmarksparttype] = ()
    bundle2.capabilities[constants.scratchmutationparttype] = ()
    _exchangesetup()
    _bundlesetup()


def _exchangesetup() -> None:
    @exchange.b2partsgenerator(constants.scratchbranchparttype)
    @perftrace.tracefunc("scratchbranchpart")
    def partgen(pushop, bundler):
        bookmark = pushop.ui.config("experimental", "server-bundlestore-bookmark")
        bookmarknode = pushop.ui.config(
            "experimental", "server-bundlestore-bookmarknode"
        )
        create = pushop.ui.configbool("experimental", "server-bundlestore-create")
        scratchpush = pushop.ui.configbool("experimental", "infinitepush-scratchpush")
        if "changesets" in pushop.stepsdone or not scratchpush:
            return

        if constants.scratchbranchparttype not in bundle2.bundle2caps(pushop.remote):
            return

        pushop.stepsdone.add("changesets")
        pushop.stepsdone.add("treepack")
        if not bookmark and not pushop.outgoing.missing:
            pushop.ui.status(_("no changes found\n"))
            pushop.cgresult = 0
            return

        # This parameter tells the server that the following bundle is an
        # infinitepush. This let's it switch the part processing to our infinitepush
        # code path.
        bundler.addparam("infinitepush", "True")

        nonforwardmove = pushop.force or pushop.ui.configbool(
            "experimental", "non-forward-move"
        )
        scratchparts = getscratchbranchparts(
            pushop.repo,
            pushop.remote,
            pushop.outgoing,
            nonforwardmove,
            pushop.ui,
            bookmark,
            create,
            bookmarknode,
        )

        for scratchpart in scratchparts:
            bundler.addpart(scratchpart)

        def handlereply(op):
            # server either succeeds or aborts; no code to read
            pushop.cgresult = 1

        return handlereply


@perftrace.tracefunc("Get ScratchBranch parts")
def getscratchbranchparts(
    repo, peer, outgoing, confignonforwardmove, ui, bookmark, create, bookmarknode=None
):
    if constants.scratchbranchparttype not in bundle2.bundle2caps(peer):
        raise error.Abort(
            _("no server support for %r") % constants.scratchbranchparttype
        )

    # This is already measured by the perftreace, but let's also measure it
    # by `timesection` to be able to aggregate on this value in Scuba
    with ui.timesection("getscratchbranchparts"):
        return _getscratchbranchpartsimpl(
            repo,
            peer,
            outgoing,
            confignonforwardmove,
            ui,
            bookmark,
            create,
            bookmarknode=bookmarknode,
        )


def _getscratchbranchpartsimpl(
    repo,
    peer,
    outgoing,
    confignonforwardmove,
    ui,
    bookmark: str,
    create,
    bookmarknode: Optional[str] = None,
):
    _validaterevset(repo, revsetlang.formatspec("%ln", outgoing.missing), bookmark)

    supportedversions = changegroup.supportedoutgoingversions(repo)
    # Explicitly avoid using '01' changegroup version in infinitepush to
    # support general delta
    supportedversions.discard("01")
    cgversion = min(supportedversions)
    _handlelfs(repo, outgoing.missing)
    cg = changegroup.makestream(repo, outgoing, cgversion, "push")

    params = {}
    params["cgversion"] = cgversion
    if bookmark:
        params["bookmark"] = bookmark
        if bookmarknode:
            params["bookmarknode"] = bookmarknode
        if create:
            params["create"] = "1"
    if confignonforwardmove:
        params["force"] = "1"

    parts = []

    # .upper() marks this as a mandatory part: server will abort if there's no
    #  handler
    parts.append(
        bundle2.bundlepart(
            constants.scratchbranchparttype.upper(),
            advisoryparams=pycompat.iteritems(params),
            data=cg,
        )
    )

    if mutation.enabled(repo):
        entries = mutation.entriesfornodes(repo, outgoing.missing)
        if entries:
            if constants.scratchmutationparttype not in bundle2.bundle2caps(peer):
                repo.ui.warn(
                    _("no server support for %r - skipping\n")
                    % constants.scratchmutationparttype
                )
            else:
                parts.append(
                    bundle2.bundlepart(
                        constants.scratchmutationparttype,
                        data=mutation.bundleentries(entries),
                    )
                )

    try:
        treemod = extensions.find("treemanifest")
        remotefilelog = extensions.find("remotefilelog")
        sendtrees = remotefilelog.shallowbundle.cansendtrees(repo, outgoing.missing)
        if sendtrees != remotefilelog.shallowbundle.NoTrees:
            parts.append(
                treemod.createtreepackpart(
                    repo, outgoing, treemod.TREEGROUP_PARTTYPE2, sendtrees=sendtrees
                )
            )
    except KeyError:
        pass

    return parts


def getscratchbookmarkspart(peer, scratchbookmarks) -> bundlepart:
    if constants.scratchbookmarksparttype not in bundle2.bundle2caps(peer):
        raise error.Abort(
            _("no server support for %r") % constants.scratchbookmarksparttype
        )

    return bundle2.bundlepart(
        constants.scratchbookmarksparttype.upper(),
        data=bookmarks.encodebookmarks(scratchbookmarks),
    )


def _validaterevset(repo, revset, bookmark) -> None:
    """Abort if the revs to be pushed aren't valid for a scratch branch."""
    if not bookmark and not repo.revs(revset):
        raise error.Abort(_("nothing to push"))
    if bookmark:
        # Allow bundle with many heads only if no bookmark is specified
        heads = repo.revs("heads(%r)", revset)
        if len(heads) > 1:
            raise error.Abort(_("cannot push more than one head to a scratch branch"))


def _handlelfs(repo, missing) -> None:
    """Special case if lfs is enabled

    If lfs is enabled then we need to call prepush hook
    to make sure large files are uploaded to lfs
    """
    try:
        lfsmod = extensions.find("lfs")
    except KeyError:
        # Ignore if lfs extension is not enabled
        pass
    else:
        with perftrace.trace("Upload LFS Blobs"):
            lfsmod.wrapper.uploadblobsfromrevs(repo, missing)

    # But wait! LFS could also be provided via remotefilelog.
    try:
        remotefilelog = extensions.find("remotefilelog")
    except KeyError:
        # Ignore if remotefilelog extension is not enabled
        pass
    else:
        if remotefilelog.shallowrepo.requirement in repo.requirements:
            with perftrace.trace("Upload LFS Blobs"):
                remotefilelog.uploadblobs(repo, missing)


def _bundlesetup() -> None:
    @bundle2.b2streamparamhandler("infinitepush")
    def processinfinitepush(unbundler, param, value):
        """process the bundle2 stream level parameter containing whether this push
        is an infinitepush or not."""
        if value and unbundler.ui.configbool("infinitepush", "bundle-stream", False):
            pass

    @bundle2.parthandler(
        constants.scratchbranchparttype, ("bookmark", "create", "force", "cgversion")
    )
    def bundle2scratchbranch(op, part):
        """unbundle a bundle2 part containing a changegroup to store"""

        bundler = bundle2.bundle20(op.repo.ui)
        cgversion = part.params.get("cgversion", "01")
        cgpart = bundle2.bundlepart("changegroup", data=part.read())
        cgpart.addparam("version", cgversion)
        bundler.addpart(cgpart)
        buf = util.chunkbuffer(bundler.getchunks())

        fd, bundlefile = tempfile.mkstemp()
        try:
            try:
                fp = util.fdopen(fd, "wb")
                fp.write(buf.read())
            finally:
                fp.close()
            server.storebundle(op, part.params, bundlefile)
        finally:
            try:
                os.unlink(bundlefile)
            except OSError as e:
                if e.errno != errno.ENOENT:
                    raise

        return 1

    @bundle2.parthandler(constants.scratchbookmarksparttype)
    def bundle2scratchbookmarks(op, part):
        """Handler deletes bookmarks first then adds new bookmarks."""
        index = op.repo.bundlestore.index
        decodedbookmarks = bookmarks.decodebookmarks(part)
        toinsert = {}
        todelete = []
        for bookmark, node in pycompat.iteritems(decodedbookmarks):
            if node:
                toinsert[bookmark] = node
            else:
                todelete.append(bookmark)
        log = server._getorcreateinfinitepushlogger(op)
        with server.logservicecall(log, constants.scratchbookmarksparttype), index:
            if todelete:
                index.deletebookmarks(todelete)
            if toinsert:
                index.addmanybookmarks(toinsert, True)

    @bundle2.parthandler(constants.scratchmutationparttype)
    def bundle2scratchmutation(op, part):
        mutation.unbundle(op.repo, part.read())
