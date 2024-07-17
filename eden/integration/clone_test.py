#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-unsafe

import json
import os
import subprocess
import sys
from pathlib import Path
from textwrap import dedent
from typing import Optional, Sequence, Set

from eden.integration.lib.hgrepo import HgRepository

from .lib import edenclient, testcase
from .lib.fake_edenfs import get_fake_edenfs_argv
from .lib.find_executables import FindExe
from .lib.service_test_case import service_test, ServiceTestCaseBase


@testcase.eden_repo_test
class CloneTest(testcase.EdenRepoTest):
    def populate_repo(self) -> None:
        self.repo.write_file("hello", "hola\n")
        self.repo.commit("Initial commit.")

    def test_clone_to_non_existent_directory(self) -> None:
        tmp = self.make_temporary_directory()
        non_existent_dir = os.path.join(tmp, "foo/bar/baz")

        self.eden.clone(self.repo.path, non_existent_dir)
        self.assertTrue(
            os.path.isfile(os.path.join(non_existent_dir, "hello")),
            msg="clone should succeed in non-existent directory",
        )

    def test_clone_to_dir_under_symlink(self) -> None:
        tmp = self.make_temporary_directory()
        empty_dir = os.path.join(tmp, "foo/bar")
        os.makedirs(empty_dir)

        symlink_dir = os.path.join(tmp, "food")
        os.symlink(os.path.join(tmp, "foo"), symlink_dir)

        symlinked_target = os.path.join(symlink_dir, "bar")

        self.eden.clone(self.repo.path, symlinked_target)
        self.assertTrue(
            os.path.isfile(os.path.join(empty_dir, "hello")),
            msg="clone should succeed in empty directory",
        )

        with self.get_thrift_client_legacy() as client:
            active_mount_points: Set[Optional[str]] = {
                os.fsdecode(mount.mountPoint) for mount in client.listMounts()
            }
            self.assertIn(
                empty_dir, active_mount_points, msg="mounted using the realpath"
            )

        self.eden.run_cmd("remove", "--yes", symlinked_target)

    def test_clone_to_existing_empty_directory(self) -> None:
        tmp = self.make_temporary_directory()
        empty_dir = os.path.join(tmp, "foo/bar/baz")
        os.makedirs(empty_dir)

        self.eden.clone(self.repo.path, empty_dir)
        self.assertTrue(
            os.path.isfile(os.path.join(empty_dir, "hello")),
            msg="clone should succeed in empty directory",
        )

    def test_clone_from_repo(self) -> None:
        # Specify the source of the clone as an existing local repo rather than
        # an alias for a config.
        destination_dir = self.make_temporary_directory()
        self.eden.clone(self.repo.path, destination_dir)
        self.assertTrue(
            os.path.isfile(os.path.join(destination_dir, "hello")),
            msg="clone should succeed in empty directory",
        )

    def test_clone_with_arcconfig(self) -> None:
        project_id = "special_project"

        # Add an .arcconfig file to the repository
        arcconfig_data = {
            "project_id": project_id,
            "conduit_uri": "https://phabricator.example.com/api/",
            "phutil_libraries": {"foo": "foo/arcanist/"},
        }
        self.repo.write_file(".arcconfig", json.dumps(arcconfig_data) + "\n")
        self.repo.commit("Add .arcconfig")

        # Add a config alias for a repo with some bind mounts.
        edenrc = os.path.join(self.home_dir, ".edenrc")
        with open(edenrc, "w") as f:
            f.write(
                dedent(
                    f"""\
            ["repository {project_id}"]
            path = "{self.repo.get_canonical_root()}"
            type = "{self.repo.get_type()}"

            ["bindmounts {project_id}"]
            mnt1 = "foo/stuff/build_output"
            mnt2 = "node_modules"
            """
                )
            )

        # Clone the repository using its path.
        # We should find the config from the project_id field in
        # the .arcconfig file
        eden_clone = self.make_temporary_directory()
        self.eden.clone(self.repo.path, eden_clone)
        self.assertFalse(
            os.path.isdir(os.path.join(eden_clone, "foo/stuff/build_output")),
            msg="clone should not create bind mounts from legacy config",
        )
        self.assertFalse(
            os.path.isdir(os.path.join(eden_clone, "node_modules")),
            msg="clone should not create bind mounts from legacy config",
        )

    def clone_rev(self, rev, repo, path) -> None:
        extra_args = []
        if self.use_nfs:
            extra_args.append("--nfs")
        self.eden.run_cmd("clone", "--rev", rev, repo, path, *extra_args)

    def test_clone_from_eden_repo(self) -> None:
        # Create an Eden mount from the config alias.
        eden_clone1 = self.make_temporary_directory()
        self.eden.clone(self.repo.path, eden_clone1)

        self.assertFalse(
            os.path.isdir(os.path.join(eden_clone1, "tmp/bm1")),
            msg="clone should not create bind mount from the legacy config",
        )

        # Clone the Eden clone! Note it should inherit its config.
        eden_clone2 = self.make_temporary_directory()
        self.clone_rev(self.repo.get_head_hash(), eden_clone1, eden_clone2)

    def test_clone_with_valid_revision_cmd_line_arg_works(self) -> None:
        tmp = self.make_temporary_directory()
        target = os.path.join(tmp, "foo/bar/baz")
        self.clone_rev(self.repo.get_head_hash(), self.repo.path, target)
        self.assertTrue(
            os.path.isfile(os.path.join(target, "hello")),
            msg="clone should succeed with --snapshop arg.",
        )

    def test_clone_with_short_revision_cmd_line_arg_works(self) -> None:
        tmp = self.make_temporary_directory()
        target = os.path.join(tmp, "foo/bar/baz")
        short = self.repo.get_head_hash()[:6]
        self.clone_rev(short, self.repo.path, target)
        self.assertTrue(
            os.path.isfile(os.path.join(target, "hello")),
            msg="clone should succeed with short --snapshop arg.",
        )

    def test_clone_to_non_empty_directory_fails(self) -> None:
        tmp = self.make_temporary_directory()
        non_empty_dir = os.path.join(tmp, "foo/bar/baz")
        os.makedirs(non_empty_dir)
        with open(os.path.join(non_empty_dir, "example.txt"), "w") as f:
            f.write("I am not empty.\n")

        with self.assertRaises(subprocess.CalledProcessError) as context:
            self.eden.clone(self.repo.path, non_empty_dir)
        stderr = context.exception.stderr
        self.assertRegex(
            stderr,
            "destination path .* is not empty",
            msg="clone into non-empty dir should fail",
        )

    def test_clone_with_invalid_revision_cmd_line_arg_fails(self) -> None:
        tmp = self.make_temporary_directory()
        empty_dir = os.path.join(tmp, "foo/bar/baz")
        os.makedirs(empty_dir)

        with self.assertRaises(subprocess.CalledProcessError) as context:
            self.clone_rev("X", self.repo.path, empty_dir)
        stderr = context.exception.stderr
        self.assertIn(
            "unable to find hash for commit 'X': ",
            stderr,
            msg="passing invalid commit on cmd line should fail",
        )

    def test_clone_to_file_fails(self) -> None:
        tmp = self.make_temporary_directory()
        non_empty_dir = os.path.join(tmp, "foo/bar/baz")
        os.makedirs(non_empty_dir)
        file_in_directory = os.path.join(non_empty_dir, "example.txt")
        with open(file_in_directory, "w") as f:
            f.write("I am not empty.\n")

        with self.assertRaises(subprocess.CalledProcessError) as context:
            self.eden.clone(self.repo.path, file_in_directory)
        stderr = context.exception.stderr
        self.assertIn(
            f"error: destination path {file_in_directory} is not a directory\n", stderr
        )

    def test_clone_to_non_existent_directory_that_is_under_a_file_fails(self) -> None:
        tmp = self.make_temporary_directory()
        non_existent_dir = os.path.join(tmp, "foo/bar/baz")
        with open(os.path.join(tmp, "foo"), "w") as f:
            f.write("I am not empty.\n")

        with self.assertRaises(subprocess.CalledProcessError) as context:
            self.eden.clone(self.repo.path, non_existent_dir)
        stderr = context.exception.stderr
        self.assertIn(
            f"error: destination path {non_existent_dir} is not a directory\n", stderr
        )

    def test_attempt_clone_invalid_repo_path(self) -> None:
        tmp = self.make_temporary_directory()
        repo_path = "/this/directory/does/not/exist"

        with self.assertRaises(edenclient.EdenCommandError) as context:
            self.eden.clone(repo_path, tmp)
        self.assertIn(
            f"error: {repo_path!r} does not look like a valid repository\n",
            context.exception.stderr,
        )

    def test_clone_should_start_daemon(self) -> None:
        # Shut down Eden.
        self.assertTrue(self.eden.is_healthy())
        self.eden.shutdown()
        self.assertFalse(self.eden.is_healthy())

        # Check `eden list`.
        list_output = self.eden.list_cmd_simple()
        self.assertEqual(
            {self.mount: "NOT_RUNNING"}, list_output, msg="Eden should have one mount."
        )

        extra_daemon_args = self.eden.get_extra_daemon_args()

        # Verify that clone starts the daemon.
        tmp = Path(self.make_temporary_directory())
        clone_output = self.eden.run_cmd(
            "clone",
            "--daemon-binary",
            FindExe.EDEN_DAEMON,
            self.repo.path,
            str(tmp),
            "--daemon-args",
            *extra_daemon_args,
        )
        self.exit_stack.callback(self.eden.run_cmd, "stop")
        self.assertIn("Starting...", clone_output)
        self.assertTrue(self.eden.is_healthy(), msg="clone should start EdenFS.")
        mount_points = {self.mount: "RUNNING", str(tmp): "RUNNING"}
        self.assertEqual(
            mount_points,
            self.eden.list_cmd_simple(),
            msg="Eden should have two mounts.",
        )
        self.assertEqual("hola\n", (tmp / "hello").read_text())

    def test_custom_not_mounted_readme(self) -> None:
        """Test that "eden clone" creates a README file in the mount point directory
        telling users what to do if their checkout is not currently mounted.
        """
        # Write a custom readme file in our etc config directory
        custom_readme_text = "If this is broken bug joe@example.com\n"
        with open(os.path.join(self.etc_eden_dir, "NOT_MOUNTED_README.txt"), "w") as f:
            f.write(custom_readme_text)

        # Perform a clone
        new_mount = Path(self.make_temporary_directory())
        readme_path = new_mount / "README_EDEN.txt"
        self.eden.clone(self.repo.path, str(new_mount))
        self.assertEqual("hola\n", (new_mount / "hello").read_text())
        self.assertFalse(os.path.exists(readme_path))

        # Now unmount the checkout and make sure we see the readme
        self.eden.run_cmd("unmount", str(new_mount))
        self.assertFalse((new_mount / "hello").exists())
        self.assertEqual(custom_readme_text, readme_path.read_text())

    def test_default_case_sensitivity(self) -> None:
        tmp = self.make_temporary_directory()
        empty_dir = os.path.join(tmp, "foo/bar/baz")
        os.makedirs(empty_dir)
        self.eden.clone(self.repo.path, empty_dir)

        self.assertEqual(
            self.eden.is_case_sensitive(empty_dir), sys.platform == "linux"
        )

    def test_force_case_sensitive(self) -> None:
        tmp = self.make_temporary_directory()
        empty_dir = os.path.join(tmp, "foo/bar/baz")
        os.makedirs(empty_dir)
        self.eden.clone(self.repo.path, empty_dir, case_sensitive=True)

        self.assertTrue(self.eden.is_case_sensitive(empty_dir))

    def test_force_case_insensitive(self) -> None:
        tmp = self.make_temporary_directory()
        empty_dir = os.path.join(tmp, "foo/bar/baz")
        os.makedirs(empty_dir)
        self.eden.clone(self.repo.path, empty_dir, case_sensitive=False)

        self.assertFalse(self.eden.is_case_sensitive(empty_dir))


class CloneFakeEdenFSTestBase(ServiceTestCaseBase):
    def setUp(self) -> None:
        super().setUp()
        self.eden_dir = Path(self.make_temporary_directory())

    def make_dummy_hg_repo(self) -> HgRepository:
        repo = HgRepository(path=self.make_temporary_directory())
        repo.init()
        repo.write_file("hello", "")
        repo.commit("Initial commit.")
        return repo

    def spawn_clone(
        self,
        repo_path: Path,
        mount_path: Path,
        extra_args: Optional[Sequence[str]] = None,
    ) -> subprocess.CompletedProcess:
        edenfsctl, env = FindExe.get_edenfsctl_env()
        fake_edenfs: str = FindExe.FAKE_EDENFS
        base_args = [
            edenfsctl,
            "--config-dir",
            str(self.eden_dir),
        ] + self.get_required_eden_cli_args()
        clone_cmd = base_args + [
            "clone",
            "--daemon-binary",
            fake_edenfs,
            str(repo_path),
            str(mount_path),
        ]
        if extra_args:
            clone_cmd.extend(extra_args)

        proc = subprocess.run(
            clone_cmd,
            env=env,
            encoding="utf-8",
            errors="replace",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        stop_cmd = base_args + ["stop"]
        self.exit_stack.callback(subprocess.call, stop_cmd)

        # Pass through any output from the clone command
        sys.stdout.write(proc.stdout)
        sys.stderr.write(proc.stderr)

        # Note that the clone operation will actually fail here, since we started
        # fake_edenfs rather than the real edenfs daemon, and it can't mount checkouts.
        # However we only care about testing that the daemon got started, so that's
        # fine.
        return proc


@service_test
class CloneFakeEdenFSTest(CloneFakeEdenFSTestBase):
    def test_daemon_command_arguments_should_forward_to_edenfs(self) -> None:
        repo = self.make_dummy_hg_repo()
        mount_path = Path(self.make_temporary_directory())

        extra_daemon_args = ["--allowExtraArgs", "hello world"]
        self.spawn_clone(
            repo_path=Path(repo.path),
            mount_path=mount_path,
            extra_args=["--daemon-args"] + extra_daemon_args,
        )

        argv = get_fake_edenfs_argv(self.eden_dir)
        expected = extra_daemon_args + [
            "--foreground",
            "--logPath",
            str(self.eden_dir / "logs/edenfs.log"),
            "--startupLoggerFd",
            "5",
        ]
        self.assertEqual(
            argv[-len(expected) :],
            expected,
            f"fake_edenfs should have received arguments verbatim\nargv: {argv}",
        )
