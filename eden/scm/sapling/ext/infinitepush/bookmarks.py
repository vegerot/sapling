# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

import struct

from sapling import extensions, json, node as nodemod, pycompat
from sapling.i18n import _


def remotebookmarksenabled(ui):
    return "remotenames" in extensions._extensions and ui.configbool(
        "remotenames", "bookmarks"
    )


def readremotebookmarks(ui, repo, other):
    if remotebookmarksenabled(ui):
        remotenamesext = extensions.find("remotenames")
        remotepath = remotenamesext.activepath(repo.ui, other)
        result = {}
        # Let's refresh remotenames to make sure we have it up to date
        # Seems that `repo.names['remotebookmarks']` may return stale bookmarks
        # and it results in deleting scratch bookmarks. Our best guess how to
        # fix it is to use `clearnames()`
        repo._remotenames.clearnames()
        for remotebookmark in repo.names["remotebookmarks"].listnames(repo):
            path, bookname = remotenamesext.splitremotename(remotebookmark)
            if path == remotepath and repo._scratchbranchmatcher.match(bookname):
                nodes = repo.names["remotebookmarks"].nodes(repo, remotebookmark)
                if nodes:
                    result[bookname] = nodemod.hex(nodes[0])
        return result
    else:
        return {}


def saveremotebookmarks(repo, newbookmarks, remote) -> None:
    remotenamesext = extensions.find("remotenames")
    remotepath = remotenamesext.activepath(repo.ui, remote)
    bookmarks = {}
    remotenames = remotenamesext.readremotenames(repo)
    for hexnode, nametype, remote, rname in remotenames:
        if remote != remotepath:
            continue
        if nametype == "bookmarks":
            if rname in newbookmarks:
                # It's possible if we have a normal bookmark that matches
                # scratch branch pattern. In this case just use the current
                # bookmark node
                del newbookmarks[rname]
            bookmarks[rname] = hexnode

    for bookmark, hexnode in pycompat.iteritems(newbookmarks):
        bookmarks[bookmark] = hexnode
    remotenamesext.saveremotenames(repo, {remotepath: bookmarks})


def savelocalbookmarks(repo, bookmarks) -> None:
    if not bookmarks:
        return
    with repo.wlock(), repo.lock(), repo.transaction("bookmark") as tr:
        changes = []
        for scratchbook, node in pycompat.iteritems(bookmarks):
            changectx = repo[node]
            changes.append((scratchbook, changectx.node()))
        repo._bookmarks.applychanges(repo, tr, changes)


def encodebookmarks(bookmarks) -> bytes:
    encoded = {}
    for bookmark, node in pycompat.iteritems(bookmarks):
        encoded[bookmark] = node
    dumped = pycompat.encodeutf8(json.dumps(encoded))
    result = struct.pack(">i", len(dumped)) + dumped
    return result


def decodebookmarks(stream):
    sizeofjsonsize = struct.calcsize(">i")
    size = struct.unpack(">i", stream.read(sizeofjsonsize))[0]
    unicodedict = json.loads(stream.read(size))
    # python json module always returns unicode strings. We need to convert
    # it back to bytes string
    result = {}
    for bookmark, node in pycompat.iteritems(unicodedict):
        result[bookmark] = node
    return result
