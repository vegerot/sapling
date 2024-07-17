# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

from __future__ import absolute_import

from sapling import manifest, mdiff, util
from sapling.node import hex, nullid

from . import constants, shallowutil


class ChainIndicies:
    """A static class for easy reference to the delta chain indicies."""

    # The filename of this revision delta
    NAME = 0
    # The mercurial file node for this revision delta
    NODE = 1
    # The filename of the delta base's revision. This is useful when delta
    # between different files (like in the case of a move or copy, we can delta
    # against the original file content).
    BASENAME = 2
    # The mercurial file node for the delta base revision. This is the nullid if
    # this delta is a full text.
    BASENODE = 3
    # The actual delta or full text data.
    DATA = 4


class unioncontentstore:
    def __init__(self, *args, **kwargs):
        self.stores = list(args)

        # If allowincomplete==True then the union store can return partial
        # delta chains, otherwise it will throw a KeyError if a full
        # deltachain can't be found.
        self.allowincomplete = kwargs.get("allowincomplete", False)

    def get(self, name, node):
        """Fetches the full text revision contents of the given name+node pair.
        If the full text doesn't exist, throws a KeyError.

        Under the hood, this uses getdeltachain() across all the stores to build
        up a full chain to produce the full text.
        """
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        chain = self.getdeltachain(name, node)

        if chain[-1][ChainIndicies.BASENODE] != nullid:
            # If we didn't receive a full chain, throw
            raise KeyError((name, hex(node)))

        # The last entry in the chain is a full text, so we start our delta
        # applies with that.
        fulltext = chain.pop()[ChainIndicies.DATA]

        text = fulltext
        while chain:
            delta = chain.pop()[ChainIndicies.DATA]
            text = mdiff.patches(text, [delta])

        return text

    def getdelta(self, name, node):
        """Return the single delta entry for the given name/node pair."""
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        for store in self.stores:
            try:
                return store.getdelta(name, node)
            except KeyError:
                pass

        raise shallowutil.MissingNodesError([(name, node)])

    def getdeltachain(self, name, node):
        """Returns the deltachain for the given name/node pair.

        Returns an ordered list of:

          [(name, node, deltabasename, deltabasenode, deltacontent),...]

        where the chain is terminated by a full text entry with a nullid
        deltabasenode.
        """
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        chain = self._getpartialchain(name, node)
        while chain[-1][ChainIndicies.BASENODE] != nullid:
            x, x, deltabasename, deltabasenode, x = chain[-1]
            try:
                morechain = self._getpartialchain(deltabasename, deltabasenode)
                chain.extend(morechain)
            except KeyError:
                # If we allow incomplete chains, don't throw.
                if not self.allowincomplete:
                    raise
                break

        return chain

    def getmeta(self, name, node):
        """Returns the metadata dict for given node."""
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        for store in self.stores:
            try:
                return store.getmeta(name, node)
            except KeyError:
                pass
        raise KeyError((name, hex(node)))

    def _getpartialchain(self, name, node):
        """Returns a partial delta chain for the given name/node pair.

        A partial chain is a chain that may not be terminated in a full-text.
        """
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        for store in self.stores:
            try:
                return store.getdeltachain(name, node)
            except KeyError:
                pass

        raise KeyError((name, hex(node)))

    def add(self, name, node, data):
        raise RuntimeError("cannot add content only to remotefilelog " "contentstore")

    def getmissing(self, keys):
        missing = keys
        for store in self.stores:
            if missing:
                missing = store.getmissing(missing)
        return missing

    def markforrefresh(self):
        for store in self.stores:
            if hasattr(store, "markforrefresh"):
                store.markforrefresh()

    def addstore(self, store):
        self.stores.append(store)

    def removestore(self, store):
        self.stores.remove(store)

    def prefetch(self, keys):
        for store in self.stores:
            if hasattr(store, "prefetch"):
                store.prefetch(keys)
                break

    def flush(self):
        for store in self.stores:
            if hasattr(store, "flush"):
                store.flush()


class manifestrevlogstore:
    def __init__(self, repo):
        # It's important that we store the repo, and not just the changelog,
        # since the changelog may be mutated in memory. So we need to refetch
        # the changelog each time we want to access it.
        self.repo = repo
        self._revlogs = util.lrucachedict(100)
        self._repackstartlinkrev = 0

    def get(self, name, node):
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        return self._revlog(name).revision(node, raw=True)

    def getdelta(self, name, node):
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        revision = self.get(name, node)
        return revision, name, nullid, self.getmeta(name, node)

    def getdeltachain(self, name, node):
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        revision = self.get(name, node)
        return [(name, node, None, nullid, revision)]

    def getmeta(self, name, node):
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        rl = self._revlog(name)
        rev = rl.rev(node)
        return {
            constants.METAKEYFLAG: rl.flags(rev),
            constants.METAKEYSIZE: rl.rawsize(rev),
        }

    def getnodeinfo(self, name, node):
        assert isinstance(name, str)
        assert isinstance(node, bytes)
        rl = self._revlog(name)
        parents = rl.parents(node)
        if "invalidatelinkrev" in self.repo.storerequirements:
            linknode = nullid
        else:
            cl = self.repo.changelog
            linkrev = rl.linkrev(rl.rev(node))
            linknode = cl.node(linkrev)
        return (parents[0], parents[1], linknode, None)

    def add(self, *args):
        raise RuntimeError("cannot add to a revlog store")

    def _revlog(self, name):
        assert isinstance(name, str)
        rl = self._revlogs.get(name)
        if rl is None:
            indexfile = None
            if name == "":
                indexfile = "00manifesttree.i"
            rl = manifest.manifestrevlog(
                self.repo.svfs, dir=name, indexfile=indexfile, treemanifest=True
            )
            self._revlogs[name] = rl
        return rl

    def getmissing(self, keys):
        missing = []
        for name, node in keys:
            assert isinstance(name, str)
            assert isinstance(node, bytes)
            mfrevlog = self._revlog(name)
            if node not in mfrevlog.nodemap:
                missing.append((name, node))

        return missing

    def setrepacklinkrevrange(self, startrev, endrev):
        self._repackstartlinkrev = startrev
        self._repackendlinkrev = endrev

    def cleanup(self, ledger):
        pass
