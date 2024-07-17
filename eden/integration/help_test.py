#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-unsafe

from .lib import edenclient, testcase


@testcase.eden_test
class HelpTest(testcase.IntegrationTestCase):
    """
    This test verifies the Eden CLI can at least load its Python code.
    It can be removed when the remaining integration tests are enabled
    on sandcastle.
    """

    def test_eden_cli_help_returns_without_error(self) -> None:
        with edenclient.EdenFS() as client:
            cmd_result = client.run_unchecked("help")
            self.assertEqual(0, cmd_result.returncode)
