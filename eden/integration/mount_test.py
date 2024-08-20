#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-unsafe

import os
import re
import shutil
import subprocess
from pathlib import Path
from typing import Optional, Set

from eden.fs.cli.util import poll_until
from eden.thrift.legacy import EdenClient, EdenNotRunningError
from facebook.eden.ttypes import (
    EdenError,
    EdenErrorType,
    FaultDefinition,
    GetBlockedFaultsRequest,
    MountState,
    ResetParentCommitsParams,
    SyncBehavior,
    UnblockFaultArg,
    WorkingDirectoryParents,
)
from fb303_core.ttypes import fb303_status
from thrift.Thrift import TException  # @manual=//thrift/lib/py:base

from .lib import testcase


@testcase.eden_repo_test
class MountTest(testcase.EdenRepoTest):
    # pyre-fixme[13]: Attribute `expected_mount_entries` is never initialized.
    expected_mount_entries: Set[str]
    enable_fault_injection: bool = True

    def populate_repo(self) -> None:
        self.maxDiff = None
        self.repo.write_file("hello", "hola\n")
        self.repo.write_file("adir/file", "foo!\n")
        self.repo.write_file("bdir/test.sh", "#!/bin/bash\necho test\n", mode=0o755)
        self.repo.write_file("bdir/noexec.sh", "#!/bin/bash\necho test\n")
        self.repo.symlink("slink", "hello")
        self.repo.commit("Initial commit.")

        self.expected_mount_entries = {".eden", "adir", "bdir", "hello", "slink"}

    def test_remove_unmounted_checkout(self) -> None:
        # Clone a second checkout mount point
        mount2 = os.path.join(self.mounts_dir, "mount2")
        self.eden.clone(self.repo.path, mount2)
        self.assertEqual(
            {self.mount: "RUNNING", mount2: "RUNNING"}, self.eden.list_cmd_simple()
        )

        # Now unmount it
        self.eden.run_cmd("unmount", mount2)
        self.assertEqual(
            {self.mount: "RUNNING", mount2: "NOT_RUNNING"}, self.eden.list_cmd_simple()
        )
        # The Eden README telling users what to do if their mount point is not mounted
        # should be present in the original mount point directory.
        self.assertTrue(os.path.exists(os.path.join(mount2, "README_EDEN.txt")))

        # Now use "eden remove" to destroy mount2
        self.eden.remove(mount2)
        self.assertEqual({self.mount: "RUNNING"}, self.eden.list_cmd_simple())
        self.assertFalse(os.path.exists(mount2))

    def test_unmount_remount(self) -> None:
        # write a file into the overlay to test that it is still visible
        # when we remount.
        filename = os.path.join(self.mount, "overlayonly")
        with open(filename, "w") as f:
            f.write("foo!\n")

        self.assert_checkout_root_entries(self.expected_mount_entries | {"overlayonly"})
        self.assertTrue(self.eden.in_proc_mounts(self.mount))

        # do a normal user-facing unmount, preserving state
        self.eden.run_cmd("unmount", self.mount)

        self.assertFalse(self.eden.in_proc_mounts(self.mount))
        entries = set(os.listdir(self.mount))
        self.assertEqual({"README_EDEN.txt"}, entries)

        # Now remount it with the mount command
        self.eden.run_cmd("mount", self.mount)

        self.assertTrue(self.eden.in_proc_mounts(self.mount))
        self.assert_checkout_root_entries(self.expected_mount_entries | {"overlayonly"})

        with open(filename, "r") as f:
            self.assertEqual("foo!\n", f.read(), msg="overlay file is correct")

    def test_double_unmount(self) -> None:
        # Test calling "unmount" twice.  The second should fail, but edenfs
        # should still work normally afterwards
        self.eden.run_cmd("unmount", self.mount)
        self.eden.run_unchecked("unmount", self.mount)

        # Now remount it with the mount command
        self.eden.run_cmd("mount", self.mount)

        self.assertTrue(self.eden.in_proc_mounts(self.mount))
        self.assert_checkout_root_entries({".eden", "adir", "bdir", "hello", "slink"})

    def test_unmount_succeeds_while_file_handle_is_open(self) -> None:
        fd = os.open(os.path.join(self.mount, "hello"), os.O_RDWR)
        # This test will fail or time out if unmounting times out.
        self.eden.run_cmd("unmount", self.mount)
        # Surprisingly, os.close does not return an error when the mount has
        # gone away.
        os.close(fd)

    def test_unmount_succeeds_while_dir_handle_is_open(self) -> None:
        fd = os.open(self.mount, 0)
        # This test will fail or time out if unmounting times out.
        self.eden.run_cmd("unmount", self.mount)
        # Surprisingly, os.close does not return an error when the mount has
        # gone away.
        os.close(fd)

    def test_mount_init_state(self) -> None:
        self.eden.run_cmd("unmount", self.mount)
        self.assertEqual({self.mount: "NOT_RUNNING"}, self.eden.list_cmd_simple())

        with self.eden.get_thrift_client_legacy() as client:
            fault = FaultDefinition(keyClass="mount", keyValueRegex=".*", block=True)
            client.injectFault(fault)

            # Run the "eden mount" CLI command.
            # This won't succeed until we unblock the mount.
            mount_cmd, edenfsctl_env = self.eden.get_edenfsctl_cmd_env(
                "mount", self.mount
            )
            mount_proc = subprocess.Popen(mount_cmd, env=edenfsctl_env)

            # Wait for the new mount to be reported by edenfs
            def mount_started() -> Optional[bool]:
                if self.eden.get_mount_state(Path(self.mount), client) is not None:
                    return True
                if mount_proc.poll() is not None:
                    raise Exception(
                        f"eden mount command finished (with status "
                        f"{mount_proc.returncode}) while mounting was "
                        f"still blocked"
                    )
                return None

            poll_until(mount_started, timeout=30)
            self.wait_on_fault_hit(key_class="mount")
            self.assertEqual({self.mount: "INITIALIZING"}, self.eden.list_cmd_simple())

            # Most thrift calls to access the mount should be disallowed while it is
            # still initializing.
            self._assert_thrift_calls_fail_during_mount_init(client)

            client.unblockFault(UnblockFaultArg(keyClass="mount", keyValueRegex=".*"))
            self._wait_for_mount_running(client)

            self.assertEqual({self.mount: "RUNNING"}, self.eden.list_cmd_simple())

            mount_proc.wait()

    def _assert_thrift_calls_fail_during_mount_init(self, client: EdenClient) -> None:
        error_regex = "mount point .* is still initializing"
        mount_path = Path(self.mount)
        null_commit = b"\00" * 20

        with self.assertRaisesRegex(EdenError, error_regex) as ctx:
            client.getFileInformation(
                mountPoint=bytes(mount_path), paths=[b""], sync=SyncBehavior()
            )
        self.assertEqual(EdenErrorType.POSIX_ERROR, ctx.exception.errorType)

        with self.assertRaisesRegex(EdenError, error_regex) as ctx:
            client.getScmStatus(
                mountPoint=bytes(mount_path), listIgnored=False, commit=null_commit
            )
        self.assertEqual(EdenErrorType.POSIX_ERROR, ctx.exception.errorType)

        parents = WorkingDirectoryParents(parent1=null_commit)
        params = ResetParentCommitsParams()
        with self.assertRaisesRegex(EdenError, error_regex) as ctx:
            client.resetParentCommits(
                mountPoint=bytes(mount_path), parents=parents, params=params
            )
        self.assertEqual(EdenErrorType.POSIX_ERROR, ctx.exception.errorType)

    def _wait_until_initializing(self, num_mounts: int = 1) -> None:
        """Wait until EdenFS is initializing mount points.
        This is primarily intended to be used to wait until the mount points are
        initializing when starting EdenFS with --fault_injection_block_mounts.
        """

        def is_initializing() -> Optional[bool]:
            try:
                with self.eden.get_thrift_client_legacy() as client:
                    # Return successfully when listMounts() reports the number of
                    # mounts that we expect.
                    mounts = client.listMounts()
                    if len(mounts) == num_mounts:
                        return True
                edenfs_process = self.eden._process
                assert edenfs_process is not None
                if edenfs_process.poll():
                    self.fail("eden exited before becoming healthy")
                return None
            except (EdenNotRunningError, TException):
                return None

        poll_until(is_initializing, timeout=60)

    def test_start_blocked_mount_init(self) -> None:
        self.eden.shutdown()
        self.eden.spawn_nowait(
            extra_args=["--enable_fault_injection", "--fault_injection_block_mounts"]
        )

        # Wait for eden to report the mount point in the listMounts() output
        self._wait_until_initializing()

        with self.eden.get_thrift_client_legacy() as client:
            # Since we blocked mount initialization the mount should still
            # report as INITIALIZING, and edenfs should report itself STARTING
            self.assertEqual({self.mount: "INITIALIZING"}, self.eden.list_cmd_simple())
            self.assertEqual(fb303_status.STARTING, client.getDaemonInfo().status)

            # Unblock mounting and wait for the mount to transition to running
            client.unblockFault(UnblockFaultArg(keyClass="mount", keyValueRegex=".*"))
            self._wait_for_mount_running(client)
            self._wait_until_alive(client)
            self.assertEqual(fb303_status.ALIVE, client.getDaemonInfo().status)

        self.assertEqual({self.mount: "RUNNING"}, self.eden.list_cmd_simple())

    def _wait_for_mount_running(
        self, client: EdenClient, path: Optional[Path] = None
    ) -> None:
        mount_path = path if path is not None else Path(self.mount)

        def mount_running() -> Optional[bool]:
            if self.eden.get_mount_state(mount_path, client) == MountState.RUNNING:
                return True
            return None

        poll_until(mount_running, timeout=60)

    def _wait_until_alive(self, client: EdenClient) -> None:
        def is_alive() -> Optional[bool]:
            if client.getDaemonInfo().status == fb303_status.ALIVE:
                return True
            return None

        poll_until(is_alive, timeout=60)

    def test_remount_creates_mount_point_dir(self) -> None:
        """Test that eden will automatically create the mount point directory if
        needed when it is setting up its mount points.
        """
        # Create a second checkout in a directory a couple levels deep so that
        # we can remove some of the parent directories of the checkout.
        checkout_path = Path(self.tmp_dir) / "checkouts" / "stuff" / "myproject"
        self.eden.clone(self.repo.path, str(checkout_path))
        self.assert_checkout_root_entries(self.expected_mount_entries, checkout_path)

        self.eden.run_cmd("unmount", str(checkout_path))
        self.assertEqual(
            {self.mount: "RUNNING", str(checkout_path): "NOT_RUNNING"},
            self.eden.list_cmd_simple(),
        )

        # Confirm that "eden mount" recreates the mount point directory
        shutil.rmtree(Path(self.tmp_dir) / "checkouts")
        self.eden.run_cmd("mount", str(checkout_path))
        self.assert_checkout_root_entries(self.expected_mount_entries, checkout_path)
        self.assertTrue(self.eden.in_proc_mounts(str(checkout_path)))
        self.assertEqual(
            {self.mount: "RUNNING", str(checkout_path): "RUNNING"},
            self.eden.list_cmd_simple(),
        )

        # Also confirm that Eden recreates the mount points on startup as well
        self.eden.shutdown()
        shutil.rmtree(Path(self.tmp_dir) / "checkouts")
        shutil.rmtree(self.mount_path)
        self.eden.start()
        self.assertEqual(
            {self.mount: "RUNNING", str(checkout_path): "RUNNING"},
            self.eden.list_cmd_simple(),
        )
        self.assert_checkout_root_entries(self.expected_mount_entries, checkout_path)

        # Check the behavior if Eden fails to create one of the mount point directories.
        # We just confirm that Eden still starts and mounts the other checkout normally
        # in this case.
        self.eden.shutdown()
        checkouts_dir = Path(self.tmp_dir) / "checkouts"
        shutil.rmtree(checkouts_dir)
        checkouts_dir.write_text("now a file\n")
        self.eden.start()
        self.assertEqual(
            {self.mount: "RUNNING", str(checkout_path): "NOT_RUNNING"},
            self.eden.list_cmd_simple(),
        )

    def test_start_with_mount_failures(self) -> None:
        # Clone a few other checkouts
        mount2 = os.path.join(self.mounts_dir, "extra_mount_1")
        self.eden.clone(self.repo.path, mount2)
        mount3 = os.path.join(self.mounts_dir, "extra_mount_2")
        self.eden.clone(self.repo.path, mount3)
        self.assertEqual(
            {self.mount: "RUNNING", mount2: "RUNNING", mount3: "RUNNING"},
            self.eden.list_cmd_simple(),
        )

        # Now restart EdenFS with mounting blocked
        self.eden.shutdown()
        self.eden.spawn_nowait(
            extra_args=["--enable_fault_injection", "--fault_injection_block_mounts"]
        )

        # Wait for eden to have started mount point initialization
        self._wait_until_initializing(num_mounts=3)

        with self.eden.get_thrift_client_legacy() as client:
            # Since we blocked mount initialization the mount should still
            # report as INITIALIZING, and edenfs should report itself STARTING
            self.assertEqual(
                {
                    self.mount: "INITIALIZING",
                    mount2: "INITIALIZING",
                    mount3: "INITIALIZING",
                },
                self.eden.list_cmd_simple(),
            )
            self.assertEqual(fb303_status.STARTING, client.getDaemonInfo().status)

            # Fail mounting of the additional 2 mounts we created
            client.unblockFault(
                UnblockFaultArg(
                    keyClass="mount",
                    keyValueRegex=".*/extra_mount.*",
                    errorType="runtime_error",
                    errorMessage="PC LOAD LETTER",
                )
            )
            # Unblock mounting of the first mount
            client.unblockFault(
                UnblockFaultArg(keyClass="mount", keyValueRegex=re.escape(self.mount))
            )
            # Wait until EdenFS reports itself as alive
            self._wait_until_alive(client)

        self.assertEqual(
            {self.mount: "RUNNING", mount2: "NOT_RUNNING", mount3: "NOT_RUNNING"},
            self.eden.list_cmd_simple(),
        )
        # The startup_mount_failures counter should indicate that 2 mounts failed to
        # remount.
        with self.eden.get_thrift_client_legacy() as client:
            mount_failures = client.getCounter("startup_mount_failures")
            self.assertEqual(2, mount_failures)
