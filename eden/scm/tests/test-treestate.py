# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

from __future__ import absolute_import

import itertools
import os
import posixpath
import random
import tempfile
import unittest

import silenttestrunner
from bindings import treestate
from sapling import pycompat


testtmp = os.getenv("TESTTMP") or tempfile.mkdtemp("test-treestate")


def randname():
    length = random.randint(1, 4)
    return "".join(random.sample("abcdef", 1)[0] for i in range(length))


def randpath(path=""):
    # pop components from path
    for i in range(1 + random.randrange(path.count("/") + 1)):
        path = os.path.dirname(path)

    # push new components to path
    maxlevel = 4
    for i in range(1 + random.randrange(max([1, maxlevel - path.count("/")]))):
        path = posixpath.join(path, randname())

    if not path:
        path = randname()

    return path


def genpaths():
    """generate random paths"""
    path = ""
    while True:
        nextpath = randpath(path)
        yield nextpath
        path = nextpath


def genfiles():
    """generate random tuple of (path, bits, mode, size, mtime, copied)"""
    pathgen = genpaths()
    while True:
        path = next(pathgen)
        bits = 0
        mode = random.randint(0, 0o777)
        size = random.randint(0, 1 << 31)
        mtime = random.randint(-1, 1 << 31)
        copied = None

        # bits (StateFlags)
        for bit in [
            treestate.EXIST_P1,
            treestate.EXIST_P2,
            treestate.EXIST_NEXT,
            treestate.IGNORED,
            treestate.NEED_CHECK,
        ]:
            if random.randint(0, 1):
                bits |= bit
        if random.randint(0, 1):
            bits |= treestate.COPIED
            copied = next(pathgen)

        yield (path, bits, mode, size, mtime, copied)


class testtreestate(unittest.TestCase):
    def testempty(self):
        tree = treestate.treestate.new(testtmp)
        self.assertEqual(len(tree), 0)
        self.assertEqual(tree.getmetadata(), b"")
        self.assertEqual(tree.walk(0, 0), [])
        self.assertTrue(tree.hasdir("/"))
        for path in ["", "a", "/", "b/c", "d/"]:
            self.assertFalse(path in tree)
            if path and path != "/":
                self.assertFalse(tree.hasdir(path))
            if path != "/":
                if path.endswith("/"):
                    self.assertIsNone(tree.getdir(path))
                else:
                    self.assertIsNone(tree.get(path, None))

    def testinsert(self):
        tree = treestate.treestate.new(testtmp)
        count = 5000
        files = list(itertools.islice(genfiles(), count))
        expected = {}
        for path, bits, mode, size, mtime, copied in files:
            tree.insert(path, bits, mode, size, mtime, copied)
            expected[path] = (bits, mode, size, mtime, copied)
        self.assertEqual(len(tree), len(expected))
        for path in tree.walk(0, 0):
            self.assertTrue(tree.hasdir(os.path.dirname(path) + "/"))
            self.assertEqual(tree.get(path, None), expected[path])

    def testremove(self):
        tree = treestate.treestate.new(testtmp)
        count = 5000
        files = list(itertools.islice(genfiles(), count))
        expected = {}
        for path, bits, mode, size, mtime, copied in files:
            tree.insert(path, bits, mode, size, mtime, copied)
            if (mtime & 1) == 0:
                tree.remove(path)
                if path in expected:
                    del expected[path]
            else:
                expected[path] = (bits, mode, size, mtime, copied)
        self.assertEqual(len(tree), len(expected))
        for path in tree.walk(0, 0):
            self.assertTrue(tree.hasdir(os.path.dirname(path) + "/"))
            self.assertEqual(tree.get(path, None), expected[path])

    def testwalk(self):
        tree = treestate.treestate.new(testtmp)
        treepath = os.path.join(testtmp, tree.filename())
        count = 5000
        files = list(itertools.islice(genfiles(), count))
        expected = {}
        for path, bits, mode, size, mtime, copied in files:
            tree.insert(path, bits, mode, size, mtime, copied)
            expected[path] = (bits, mode, size, mtime, copied)

        def walk(setbits, unsetbits):
            return sorted(
                k
                for k, v in pycompat.iteritems(expected)
                if ((v[0] & unsetbits) == 0 and (v[0] & setbits) == setbits)
            )

        def check(setbits, unsetbits):
            self.assertEqual(
                walk(setbits, unsetbits), sorted(tree.walk(setbits, unsetbits))
            )

        for i in ["in-memory", "flushed"]:
            for bit in [treestate.IGNORED, treestate.COPIED]:
                check(0, bit)
                check(bit, 0)
            check(treestate.EXIST_P1, treestate.EXIST_P2)
            rootid = tree.flush()
            tree = treestate.treestate.openraw(treepath, rootid)

    def testdirfilter(self):
        tree = treestate.treestate.new(testtmp)
        files = ["a/b", "a/b/c", "b/c", "c/d"]
        for path in files:
            tree.insert(path, 1, 2, 3, 4, None)
        self.assertEqual(tree.walk(1, 0, None), files)
        self.assertEqual(
            tree.walk(1, 0, lambda dir: dir in {"a/b/", "c/"}), ["a/b", "b/c"]
        )
        self.assertEqual(tree.walk(1, 0, lambda dir: True), [])

    def testflush(self):
        tree = treestate.treestate.new(testtmp)
        treepath = os.path.join(testtmp, tree.filename())
        tree.insert("a", 1, 2, 3, 4, None)
        tree.setmetadata(b"1")
        rootid1 = tree.flush()

        tree.remove("a")
        tree.insert("b", 1, 2, 3, 4, None)
        tree.setmetadata(b"2")
        rootid2 = tree.flush()

        tree = treestate.treestate.openraw(treepath, rootid1)
        self.assertTrue("a" in tree)
        self.assertFalse("b" in tree)
        self.assertEqual(tree.getmetadata(), b"1")

        tree = treestate.treestate.openraw(treepath, rootid2)
        self.assertFalse("a" in tree)
        self.assertTrue("b" in tree)
        self.assertEqual(tree.getmetadata(), b"2")

    def testsaveas(self):
        tree = treestate.treestate.new(testtmp)
        treepath = os.path.join(testtmp, tree.filename())
        tree.insert("a", 1, 2, 3, 4, None)
        tree.setmetadata(b"1")
        tree.flush()

        tree.insert("b", 1, 2, 3, 4, None)
        tree.remove("a")
        tree.setmetadata(b"2")
        rootid = tree.saveas(testtmp)
        treepath = os.path.join(testtmp, tree.filename())

        tree = treestate.treestate.openraw(treepath, rootid)
        self.assertFalse("a" in tree)
        self.assertTrue("b" in tree)
        self.assertEqual(tree.getmetadata(), b"2")

    def testfiltered(self):
        tree = treestate.treestate.new(testtmp)
        tree.insert("a/B/c", 1, 2, 3, 4, None)
        filtered = tree.getfiltered("A/B/C", lambda x: x.upper(), 1)
        self.assertEqual(filtered, ["a/B/c"])
        filtered = tree.getfiltered("A/B/C", lambda x: x, 2)
        self.assertEqual(filtered, [])

    def testpathcomplete(self):
        tree = treestate.treestate.new(testtmp)
        paths = ["a/b/c", "a/b/d", "a/c", "de"]
        for path in paths:
            tree.insert(path, 1, 2, 3, 4, None)

        def complete(prefix, fullpath=False):
            completed = []
            tree.pathcomplete(prefix, 0, 0, completed.append, fullpath)
            return completed

        self.assertEqual(complete(""), ["a/", "de"])
        self.assertEqual(complete("d"), ["de"])
        self.assertEqual(complete("a/"), ["a/b/", "a/c"])
        self.assertEqual(complete("a/b/"), ["a/b/c", "a/b/d"])
        self.assertEqual(complete("a/b/c"), ["a/b/c"])
        self.assertEqual(complete("", True), paths)

    def testgetdir(self):
        tree = treestate.treestate.new(testtmp)
        tree.insert("a/b/c", 3, 0, 0, 0, None)
        tree.insert("a/d", 5, 0, 0, 0, None)
        self.assertEqual(tree.getdir("/"), (3 | 5, 3 & 5))
        self.assertEqual(tree.getdir("a/"), (3 | 5, 3 & 5))
        self.assertEqual(tree.getdir("a/b/"), (3, 3))
        self.assertIsNone(tree.getdir("a/b/c/"))
        tree.insert("a/e/f", 10, 0, 0, 0, None)
        self.assertEqual(tree.getdir("a/"), (3 | 5 | 10, 3 & 5 & 10))
        tree.remove("a/e/f")
        self.assertEqual(tree.getdir("a/"), (3 | 5, 3 & 5))

    def testsubdirquery(self):
        tree = treestate.treestate.new(testtmp)
        paths = ["a/b/c", "a/b/d", "a/c", "de"]
        for path in paths:
            tree.insert(path, 1, 2, 3, 4, None)
        self.assertEqual(tree.tracked(""), paths)
        self.assertEqual(tree.tracked("de"), ["de"])
        self.assertEqual(tree.tracked("a"), [])
        self.assertEqual(tree.tracked("a/"), ["a/b/c", "a/b/d", "a/c"])
        self.assertEqual(tree.tracked("a/b/"), ["a/b/c", "a/b/d"])
        self.assertEqual(tree.tracked("a/b"), [])
        self.assertEqual(tree.tracked("a/c/"), [])
        self.assertEqual(tree.tracked("a/c"), ["a/c"])


if __name__ == "__main__":
    silenttestrunner.main(__name__)
