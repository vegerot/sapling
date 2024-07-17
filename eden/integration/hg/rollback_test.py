#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-unsafe

from eden.integration.lib import hgrepo

from .lib.hg_extension_test_base import EdenHgTestCase, hg_test


@hg_test
# pyre-ignore[13]: T62487924
class RollbackTest(EdenHgTestCase):
    _commit1: str

    def populate_backing_repo(self, repo: hgrepo.HgRepository) -> None:
        repo.write_file("first", "")
        self._commit1 = repo.commit("first commit")

    def test_commit_with_precommit_failure_should_trigger_rollback(self) -> None:
        original_commits = self.repo.log()

        self.repo.write_file("first", "THIS IS CHANGED")
        self.assert_status({"first": "M"})

        with self.assertRaises(hgrepo.HgError) as context:
            self.hg(
                "commit",
                "-m",
                "Precommit hook should fail, causing rollback.",
                "--config",
                "hooks.pretxncommit=false",
            )
        expected_msg = b"abort: pretxncommit hook exited with status 1\n"
        self.assertIn(expected_msg, context.exception.stderr)

        self.assertEqual(
            original_commits,
            self.repo.log(),
            msg="Failed precommit hook should abort the change and "
            "leave Hg in the original state.",
        )
        self.assert_status({"first": "M"})
