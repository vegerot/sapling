#!/usr/bin/env python
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

from __future__ import absolute_import

import hashlib
import os
import random
import shutil
import stat
import struct
import tempfile
import unittest

import silenttestrunner
from bindings import revisionstore
from sapling import error, pycompat
from sapling.ext.remotefilelog.metadatastore import unionmetadatastore
from sapling.node import nullid


SMALLFANOUTCUTOFF = int(2**16 / 8)
LARGEFANOUTPREFIX = 2

try:
    xrange(0)
except NameError:
    xrange = range


class histpacktestsbase:
    def __init__(self, historypackreader, historypackwriter):
        self.historypackreader = historypackreader
        self.historypackwriter = historypackwriter

    def setUp(self):
        self.tempdirs = []

    def tearDown(self):
        for d in self.tempdirs:
            shutil.rmtree(d)

    def makeTempDir(self):
        tempdir = tempfile.mkdtemp()
        self.tempdirs.append(tempdir)
        return tempdir

    def getHash(self, content):
        return hashlib.sha1(content).digest()

    def getFakeHash(self):
        return os.urandom(20)

    def createPack(self, revisions=None):
        """Creates and returns a historypack containing the specified revisions.

        `revisions` is a list of tuples, where each tuple contains a filanem,
        node, p1node, p2node, and linknode.
        """
        if revisions is None:
            revisions = [
                (
                    "filename",
                    self.getFakeHash(),
                    nullid,
                    nullid,
                    self.getFakeHash(),
                    None,
                )
            ]

        packdir = self.makeTempDir()
        packer = self.historypackwriter(packdir)

        for filename, node, p1, p2, linknode, copyfrom in revisions:
            packer.add(filename, node, p1, p2, linknode, copyfrom)

        path = packer.flush()[0]
        return self.historypackreader(path)

    def testAddSingle(self):
        """Test putting a single entry into a pack and reading it out."""
        filename = "foo"
        node = self.getFakeHash()
        p1 = self.getFakeHash()
        p2 = self.getFakeHash()
        linknode = self.getFakeHash()

        revisions = [(filename, node, p1, p2, linknode, None)]
        pack = self.createPack(revisions)

        actual = pack.getnodeinfo(filename, node)
        self.assertEqual(p1, actual[0])
        self.assertEqual(p2, actual[1])
        self.assertEqual(linknode, actual[2])

    def testAddMultiple(self):
        """Test putting multiple unrelated revisions into a pack and reading
        them out.
        """
        revisions = []
        for i in range(10):
            filename = "foo-%s" % i
            node = self.getFakeHash()
            p1 = self.getFakeHash()
            p2 = self.getFakeHash()
            linknode = self.getFakeHash()
            revisions.append((filename, node, p1, p2, linknode, None))

        pack = self.createPack(revisions)

        for filename, node, p1, p2, linknode, copyfrom in revisions:
            actual = pack.getnodeinfo(filename, node)
            self.assertEqual(p1, actual[0])
            self.assertEqual(p2, actual[1])
            self.assertEqual(linknode, actual[2])
            self.assertEqual(copyfrom, actual[3])

    def testPackMany(self):
        """Pack many related and unrelated ancestors."""
        # Build a random pack file
        allentries = {}
        ancestorcounts = {}
        revisions = []
        random.seed(0)
        for i in range(100):
            filename = "filename-%s" % i
            entries = []
            p2 = nullid
            linknode = nullid
            for j in range(random.randint(1, 100)):
                node = self.getFakeHash()
                p1 = nullid
                if len(entries) > 0:
                    p1 = entries[random.randint(0, len(entries) - 1)]
                entries.append(node)
                revisions.append((filename, node, p1, p2, linknode, None))
                allentries[(filename, node)] = (p1, p2, linknode)
                if p1 == nullid:
                    ancestorcounts[(filename, node)] = 1
                else:
                    newcount = ancestorcounts[(filename, p1)] + 1
                    ancestorcounts[(filename, node)] = newcount

        # Must add file entries in reverse topological order
        revisions = list(reversed(revisions))
        pack = self.createPack(revisions)
        store = unionmetadatastore(pack)

        # Verify the pack contents
        for (filename, node), (p1, p2, lastnode) in pycompat.iteritems(allentries):
            ap1, ap2, alinknode, acopyfrom = store.getnodeinfo(filename, node)
            ep1, ep2, elinknode = allentries[(filename, node)]
            self.assertEqual(ap1, ep1)
            self.assertEqual(ap2, ep2)
            self.assertEqual(alinknode, elinknode)
            self.assertEqual(acopyfrom, None)

    def testGetNodeInfo(self):
        revisions = []
        filename = "foo"
        lastnode = nullid
        for i in range(10):
            node = self.getFakeHash()
            revisions.append((filename, node, lastnode, nullid, nullid, None))
            lastnode = node

        pack = self.createPack(revisions)

        # Test that getnodeinfo returns the expected results
        for filename, node, p1, p2, linknode, copyfrom in revisions:
            ap1, ap2, alinknode, acopyfrom = pack.getnodeinfo(filename, node)
            self.assertEqual(ap1, p1)
            self.assertEqual(ap2, p2)
            self.assertEqual(alinknode, linknode)
            self.assertEqual(acopyfrom, copyfrom)

    def testGetMissing(self):
        """Test the getmissing() api."""
        revisions = []
        filename = "foo"
        for i in range(10):
            node = self.getFakeHash()
            p1 = self.getFakeHash()
            p2 = self.getFakeHash()
            linknode = self.getFakeHash()
            revisions.append((filename, node, p1, p2, linknode, None))

        pack = self.createPack(revisions)

        missing = pack.getmissing([(filename, revisions[0][1])])
        self.assertFalse(missing)

        missing = pack.getmissing(
            [(filename, revisions[0][1]), (filename, revisions[1][1])]
        )
        self.assertFalse(missing)

        fakenode = self.getFakeHash()
        missing = pack.getmissing([(filename, revisions[0][1]), (filename, fakenode)])
        self.assertEqual(missing, [(filename, fakenode)])

        # Test getmissing on a non-existent filename
        missing = pack.getmissing([("bar", fakenode)])
        self.assertEqual(missing, [("bar", fakenode)])

    def testBadVersionThrows(self):
        pack = self.createPack()
        path = pack.path() + ".histpack"
        with open(path, "rb") as f:
            raw = f.read()
        raw = struct.pack("!B", 255) + raw[1:]
        os.chmod(path, os.stat(path).st_mode | stat.S_IWRITE)
        with open(path, "wb+") as f:
            f.write(raw)

        try:
            pack = self.historypackreader(pack.path())
            self.assertTrue(False, "bad version number should have thrown")
        except error.UncategorizedNativeError:
            pass

    def testLargePack(self):
        """Test creating and reading from a large pack with over X entries.
        This causes it to use a 2^16 fanout table instead."""
        total = SMALLFANOUTCUTOFF + 1
        revisions = []
        for i in xrange(total):
            filename = "foo-%s" % i
            node = self.getFakeHash()
            p1 = self.getFakeHash()
            p2 = self.getFakeHash()
            linknode = self.getFakeHash()
            revisions.append((filename, node, p1, p2, linknode, None))

        pack = self.createPack(revisions)
        if hasattr(pack, "params"):
            self.assertEqual(pack.params.fanoutprefix, LARGEFANOUTPREFIX)

        for filename, node, p1, p2, linknode, copyfrom in revisions:
            actual = pack.getnodeinfo(filename, node)
            self.assertEqual(p1, actual[0])
            self.assertEqual(p2, actual[1])
            self.assertEqual(linknode, actual[2])
            self.assertEqual(copyfrom, actual[3])

    def testReadingMutablePack(self):
        """Tests that the data written into a mutablehistorypack can be read out
        before it has been finalized."""
        packdir = self.makeTempDir()
        packer = self.historypackwriter(packdir)

        revisions = []

        filename = "foo"
        lastnode = nullid
        for i in range(5):
            node = self.getFakeHash()
            revisions.append((filename, node, lastnode, nullid, nullid, None))
            lastnode = node

        filename = "bar"
        lastnode = nullid
        for i in range(5):
            node = self.getFakeHash()
            revisions.append((filename, node, lastnode, nullid, nullid, None))
            lastnode = node

        for filename, node, p1, p2, linknode, copyfrom in revisions:
            packer.add(filename, node, p1, p2, linknode, copyfrom)

        # Test getnodeinfo()
        for filename, node, p1, p2, linknode, copyfrom in revisions:
            entry = packer.getnodeinfo(filename, node)
            self.assertEqual(entry, (p1, p2, linknode, copyfrom))

        # Test getmissing()
        missingcheck = [(revisions[0][0], revisions[0][1]), ("foo", self.getFakeHash())]
        missing = packer.getmissing(missingcheck)
        self.assertEqual(missing, missingcheck[1:])


class rusthistpacktests(histpacktestsbase, unittest.TestCase):
    def __init__(self, *args, **kwargs):
        histpacktestsbase.__init__(
            self, revisionstore.historypack, revisionstore.mutablehistorystore
        )
        unittest.TestCase.__init__(self, *args, **kwargs)


if __name__ == "__main__":
    silenttestrunner.main(__name__)
