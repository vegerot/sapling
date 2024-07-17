#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-strict

import binascii
import unittest
from typing import Tuple

import eden.dirstate
import eden.fs.cli.doctor as doctor
from eden.fs.cli.config import EdenCheckout
from eden.fs.cli.doctor.test.lib.fake_eden_instance import FakeEdenInstance
from eden.fs.cli.test.lib.fake_proc_utils import FakeProcUtils
from eden.fs.cli.test.lib.output import TestOutput
from eden.test_support.temporary_directory import TemporaryDirectoryMixin


class DoctorTestBase(unittest.TestCase, TemporaryDirectoryMixin):
    def create_fixer(self, dry_run: bool) -> Tuple[doctor.ProblemFixer, TestOutput]:
        out = TestOutput()
        instance = FakeEdenInstance(self.make_temporary_directory())
        if not dry_run:
            fixer = doctor.ProblemFixer(instance, out)
        else:
            fixer = doctor.DryRunFixer(instance, out)
        return fixer, out

    def assert_results(
        self,
        fixer: doctor.ProblemFixer,
        num_problems: int = 0,
        num_fixed_problems: int = 0,
        num_failed_fixes: int = 0,
        num_manual_fixes: int = 0,
        num_no_fixes: int = 0,
        num_advisory_fixes: int = 0,
    ) -> None:
        self.assertEqual(num_problems, fixer.num_problems)
        self.assertEqual(num_fixed_problems, fixer.num_fixed_problems)
        self.assertEqual(num_failed_fixes, fixer.num_failed_fixes)
        self.assertEqual(num_manual_fixes, fixer.num_manual_fixes)
        self.assertEqual(num_no_fixes, fixer.num_no_fixes)
        self.assertEqual(num_advisory_fixes, fixer.num_advisory_fixes)

    def assert_dirstate_p0(self, checkout: EdenCheckout, commit: str) -> None:
        dirstate_path = checkout.path / ".hg" / "dirstate"
        with dirstate_path.open("rb") as f:
            parents, _tuples_dict, _copymap = eden.dirstate.read(f, str(dirstate_path))
        self.assertEqual(binascii.hexlify(parents[0]).decode("utf-8"), commit)

    def make_proc_utils(self) -> FakeProcUtils:
        return FakeProcUtils(self.make_temporary_directory())
