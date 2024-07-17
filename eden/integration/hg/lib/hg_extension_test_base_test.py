#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-unsafe

import os

from eden.integration.lib import hgrepo

from .hg_extension_test_base import EdenHgTestCase, hg_test


@hg_test
# pyre-ignore[13]: T62487924
class HgExtensionTestBaseTest(EdenHgTestCase):
    """Test to make sure that HgExtensionTestBase creates Eden mounts that are
    properly configured with the Hg extension.
    """

    def populate_backing_repo(self, repo: hgrepo.HgRepository) -> None:
        repo.write_file("hello.txt", "hola")
        repo.commit("Initial commit.")

    def test_setup(self) -> None:
        hg_dir = os.path.join(self.mount, ".hg")
        self.assertTrue(os.path.isdir(hg_dir))

        eden_extension = self.hg("config", "extensions.eden", check=False).rstrip()
        self.assertEqual("", eden_extension)

        self.assertTrue(os.path.isfile(self.get_path("hello.txt")))
