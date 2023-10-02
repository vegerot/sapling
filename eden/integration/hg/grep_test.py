#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

import sys
from textwrap import dedent

from eden.integration.lib import hgrepo

from .lib.hg_extension_test_base import EdenHgTestCase, hg_test


@hg_test
# pyre-ignore[13]: T62487924
class GrepTest(EdenHgTestCase):
    def populate_backing_repo(self, repo: hgrepo.HgRepository) -> None:
        repo.write_file("file_in_root.txt", "\n".join(["apple", "  banana", "cat"]))
        repo.write_file("d1/d2/afile.txt", "\n".join(["banana", "  banana"]))
        repo.write_file("d1/d2/bfile.txt", "\n".join(["    banana", "cat", "dog"]))
        repo.commit("Initial commit.")

    def test_grep_file(self) -> None:
        stdout = self.hg("grep", "-n", "banana", "file_in_root.txt")
        self.assertEqual("file_in_root.txt:2:  banana\n", stdout)

    def test_grep_directory_from_root(self) -> None:
        stdout = self.hg("grep", "-n", "banana", "d1/d2")
        expected = dedent(
            """\
        d1/d2/afile.txt:1:banana
        d1/d2/afile.txt:2:  banana
        d1/d2/bfile.txt:1:    banana
        """
        )

        self.assertEqual(expected, stdout)

    def test_grep_directory_from_subdirectory(self) -> None:
        stdout = self.hg("grep", "-n", "banana", "d2", cwd=self.get_path("d1"))
        expected = dedent(
            """\
        d2/afile.txt:1:banana
        d2/afile.txt:2:  banana
        d2/bfile.txt:1:    banana
        """
        )

        self.assertEqual(expected, stdout)

    def test_grep_that_does_not_match_anything(self) -> None:
        with self.assertRaises(hgrepo.HgError) as context:
            self.hg("grep", "NOT IN THERE")
        self.assertEqual(b"", context.exception.stdout)
        self.assertEqual(b"", context.exception.stderr)
        # the returncode is forwarded from xargs. xargs on linux exits with 123
        # if the underlying command fails, xargs on mac exits with 1 :(.
        if sys.platform == "darwin":
            expected_returncode = 1
        else:
            expected_returncode = 123
        self.assertEqual(expected_returncode, context.exception.returncode)

    def test_grep_that_does_not_match_anything_in_directory(self) -> None:
        with self.assertRaises(hgrepo.HgError) as context:
            self.hg("grep", "NOT IN THERE", "d1")
        self.assertEqual(b"", context.exception.stdout)
        self.assertEqual(b"", context.exception.stderr)
        # the returncode is forwarded from xargs. xargs on linux exits with 123
        # if the underlying command fails, xargs on mac exits with 1 :(.
        if sys.platform == "darwin":
            expected_returncode = 1
        else:
            expected_returncode = 123
        self.assertEqual(expected_returncode, context.exception.returncode)
