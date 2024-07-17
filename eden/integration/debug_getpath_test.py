#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-unsafe

import os
import stat
import time

from facebook.eden.ttypes import TimeSpec

from .lib import edenclient, testcase


@testcase.eden_repo_test
class DebugGetPathTest(testcase.EdenRepoTest):
    def populate_repo(self) -> None:
        self.repo.write_file("hello", "hola\n")
        self.repo.commit("Initial commit.")

    def test_getpath_root_inode(self) -> None:
        """
        Test that calling `eden debug getpath 1` returns the path to the eden
        mount, and indicates that the inode is loaded.
        """
        output = self.eden.run_cmd("debug", "getpath", "1", cwd=self.mount)

        self.assertEqual("loaded " + self.mount + "\n", output)

    def test_getpath_dot_eden_inode(self) -> None:
        """
        Test that calling `eden debug getpath ${ino}` returns the path to the
        .eden directory, and indicates that the inode is loaded.
        """
        st = os.lstat(os.path.join(self.mount, ".eden"))
        self.assertTrue(stat.S_ISDIR(st.st_mode))
        ino = st.st_ino

        output = self.eden.run_cmd("debug", "getpath", str(ino), cwd=self.mount)

        self.assertEqual("loaded " + os.path.join(self.mount, ".eden") + "\n", output)

    def test_getpath_invalid_inode(self) -> None:
        """
        Test that calling `eden debug getpath 1234` raises an error since
        1234 is not a valid inode number
        """
        with self.assertRaises(edenclient.EdenCommandError) as context:
            self.eden.run_cmd("debug", "getpath", "1234", cwd=self.mount)
            self.assertIn(
                "unknown inode number 1234", context.exception.stderr.decode()
            )

    def test_getpath_unloaded_inode(self) -> None:
        """
        Test that calling `eden debug getpath` on an unloaded inode returns the
        correct path and indicates that it is unloaded
        """
        dirpath = os.path.join(self.mount, "dir")
        filepath = os.path.join(dirpath, "file")

        # Create the file
        self.write_file(os.path.join("dir", "file"), "blah")
        # Get the inodeNumber
        stat = os.stat(filepath)
        self.unload_one_inode_under("dir")

        # Get the path for dir/file from its inodeNumber
        output = self.eden.run_cmd("debug", "getpath", str(stat.st_ino), cwd=self.mount)

        self.assertEqual(f"unloaded {filepath}\n", output)

    def test_getpath_unloaded_inode_rename_parent(self) -> None:
        """
        Test that when an unloaded inode has one of its parents renamed,
        `eden debug getpath` returns the new path
        """
        # Create the file
        self.write_file(os.path.join("foo", "bar", "test.txt"), "blah")
        dirpath = os.path.join(self.mount, "foo", "bar")
        # Get the inodeNumber
        stat = os.stat(os.path.join(dirpath, "test.txt"))

        self.unload_one_inode_under(os.path.join("foo", "bar"))

        # Rename the foo directory
        os.rename(os.path.join(self.mount, "foo"), os.path.join(self.mount, "newname"))
        # Get the new path for the file from its inodeNumber
        output = self.eden.run_cmd("debug", "getpath", str(stat.st_ino), cwd=self.mount)

        self.assertEqual(
            "unloaded " + os.path.join(self.mount, "newname", "bar", "test.txt") + "\n",
            output,
        )

    def unload_one_inode_under(self, path: str) -> None:
        # TODO: To support unloading more than one inode, sum the return value
        # until count is reached our the attempt limit has been reached.
        remaining_attempts = 5
        while True:
            age = TimeSpec()  # zero
            with self.eden.get_thrift_client_legacy() as client:
                count = client.unloadInodeForPath(
                    os.fsencode(self.mount), os.fsencode(path), age
                )
            if remaining_attempts == 1:
                self.assertEqual(1, count)
            elif count == 1:
                break
            else:
                remaining_attempts -= 1
                time.sleep(1)
                continue

    def test_getpath_unlinked_inode(self) -> None:
        """
        Test that when an inode is unlinked, `eden debug getpath` indicates
        that it is unlinked
        """
        # Create the file
        path = os.path.join("foo", "bar", "test.txt")
        self.write_file(path, "blah")
        # Keep an open file handle so that the inode doesn't become invalid
        abspath = os.path.join(self.mount, path)
        f = open(abspath)
        # Get the inodeNumber
        stat = os.stat(abspath)
        # Unlink the file
        os.unlink(abspath)
        output = self.eden.run_cmd("debug", "getpath", str(stat.st_ino), cwd=self.mount)
        # Close the file handle
        f.close()

        if self.use_nfs():
            # With NFS, the client will rename an opened file to a "silly name"
            # on unlink. Only when the file is actually deleted the silly name
            # is removed.
            self.assertRegex(output, f"loaded {os.path.dirname(abspath)}/\\.nfs.*\n")
        else:
            self.assertEqual(output, "loaded [unlinked]\n")
