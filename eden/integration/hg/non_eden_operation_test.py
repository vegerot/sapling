#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-unsafe

import os

from eden.integration.hg.lib.hg_extension_test_base import EdenHgTestCase, hg_test
from eden.integration.lib import hgrepo


@hg_test
# pyre-ignore[13]: T62487924
class NonEdenOperationTest(EdenHgTestCase):
    def populate_backing_repo(self, repo: hgrepo.HgRepository) -> None:
        repo.write_file("hello.txt", "hola")

    def test_hg_clone_non_eden_repo_within_eden_repo(self) -> None:
        """Regression test to ensure that running `hg` commands from an
        Eden-backed Hg repo on a non-Eden-backed Hg repo work as expected."""
        non_eden_hg_repo = os.path.join(self.tmp_dir, "non-eden-hg-repo")
        os.mkdir(non_eden_hg_repo)

        # Create the non-Eden Hg repo to clone.
        self.hg("init", "--config=format.use-eager-repo=True", cwd=non_eden_hg_repo)
        first_file = os.path.join(non_eden_hg_repo, "first.txt")
        with open(first_file, "w") as f:
            f.write("First file in non-Eden-backed Hg repo.\n")
        self.hg(
            "commit",
            "--config",
            "ui.username=Kevin Flynn <lightcyclist@example.com>",
            "--config=remotefilelog.reponame=dummy",
            "-Am",
            "first commit",
            cwd=non_eden_hg_repo,
        )
        self.hg(
            "bookmark",
            "main",
            cwd=non_eden_hg_repo,
        )

        # Run `hg clone` from the Eden repo.
        clone_of_non_eden_hg_repo = os.path.join(self.tmp_dir, "clone-target")
        self.hg(
            "clone",
            f"--config=remotefilelog.cachepath={os.path.join(self.tmp_dir, 'hgcache')}",
            non_eden_hg_repo,
            clone_of_non_eden_hg_repo,
            cwd=self.repo.path,
        )
        self.hg(
            "goto",
            "remote/main",
            f"--config=remotefilelog.cachepath={os.path.join(self.tmp_dir, 'hgcache')}",
            cwd=clone_of_non_eden_hg_repo,
        )

        dest_first_file = os.path.join(clone_of_non_eden_hg_repo, "first.txt")
        with open(dest_first_file, "r") as f:
            contents = f.read()
        self.assertEqual("First file in non-Eden-backed Hg repo.\n", contents)
