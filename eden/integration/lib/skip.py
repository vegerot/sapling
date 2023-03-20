#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

import os
import sys
import unittest
from typing import Dict, List, Union


#
# Disabled tests definitions.
# This is a dictionary of class names. For each class the value can be set to True to
# skip all tests in this class, or a list of specific test functions to skip.
#
# We are currently skipping most existing test cases on Windows, but over time we
# should gradually remove tests from this list as we get them passing on Windows.
#
TEST_DISABLED: Dict[str, Union[List[str], bool]] = {}
if sys.platform == "win32":
    # Note that on Windows we also exclude some test source files entirely
    # in CMakeLists.txt, for tests that never make sense to run on Windows.
    TEST_DISABLED: Dict[str, Union[List[str], None]] = {
        #
        # Test classes from the main integration test binary
        #
        "basic_test.BasicTest": [
            "test_symlinks",
        ],
        "basic_test.PosixTest": True,
        "chown_test.ChownTest": True,
        "clone_test.CloneFakeEdenFSTestAdHoc": True,
        "clone_test.CloneFakeEdenFSTestManaged": True,
        "clone_test.CloneTestHg": True,
        "config_test.ConfigTest": True,
        "corrupt_overlay_test.CorruptOverlayTestDefault": True,
        "debug_getpath_test.DebugGetPathTestHg": True,
        "doteden_test.DotEdenTestHg": [
            "test_mkdir_fails",  # ProjectedFS doesn't allow refusing directory creation
            "test_create_file_fails",  # ProjectedFS doesn't allow refusing file creation
            "test_mknod_fails",  # mknod doesn't exist on Windows
            "test_symlink_fails",  # ProjectedFS doesn't allow refusing symlink creation
            "test_chown_fails",  # chown doesn't exist on Windows
        ],
        "edenclient_test.EdenClientTestHg": True,
        "facebook.buck.buck_test.BuckTestHg": True,
        "fsck_test.FsckTestDefault": True,
        "fsck_test.FsckTestNoEdenfs": True,
        "fsck.basic_snapshot_tests.Basic20210712Test": True,
        "health_test.HealthOfFakeEdenFSTestAdHoc": True,
        "health_test.HealthOfFakeEdenFSTestManaged": True,
        "info_test.InfoTestHg": True,
        "materialized_query_test.MaterializedQueryTestHg": True,
        "mmap_test.MmapTestHg": True,
        "mount_test.MountTestHg": True,
        "oexcl_test.OpenExclusiveTestHg": True,
        "patch_test.PatchTestHg": True,
        "persistence_test.PersistenceTestHg": [
            "test_does_not_reuse_inode_numbers_after_cold_restart"
        ],
        "rage_test.RageTestDefault": True,
        "rc_test.RCTestHg": True,
        "redirect_test.RedirectTestHg": ["test_disallow_bind_mount_outside_repo"],
        "remount_test.RemountTestHg": True,
        "rename_test.RenameTestHg": True,
        "restart_test.RestartTestAdHoc": True,
        "restart_test.RestartTestManaged": True,
        "sed_test.SedTestHg": True,
        "service_log_test.ServiceLogFakeEdenFSTestAdHoc": True,
        "service_log_test.ServiceLogFakeEdenFSTestManaged": True,
        "service_log_test.ServiceLogRealEdenFSTest": True,
        "setattr_test.SetAttrTestHg": True,
        "snapshot.test_snapshots.InfraTests": True,
        "snapshot.test_snapshots.Test": True,
        "stale_test.StaleTestDefault": True,
        "start_test.DirectInvokeTest": True,
        "start_test.StartFakeEdenFSTestAdHoc": True,
        "start_test.StartFakeEdenFSTestManaged": True,
        "start_test.StartTest": True,
        "start_test.StartWithRepoTestHg": True,
        "stats_test.GenericStatsTest": [
            "test_writing_untracked_file_bumps_write_counter",  # counter not implemented for PrjFS (T147665665)
            "test_summary_counters_available",  # counter not implemented for PrjFS (T147669123)
        ],
        "stop_test.AutoStopTest": True,
        "stop_test.StopTestAdHoc": True,
        "stop_test.StopTestManaged": True,
        "takeover_test.TakeoverRocksDBStressTestHg": True,
        "takeover_test.TakeoverTestHg": True,
        "takeover_test.TakeoverTestNoNFSServerHg": True,
        "thrift_test.ThriftTestHg": [
            "test_get_sha1_throws_for_symlink",
            "test_pid_fetch_counts",
            "test_unload_free_inodes",
            "test_unload_thrift_api_accepts_single_dot_as_root",
        ],
        "unixsocket_test.UnixSocketTestHg": True,
        "userinfo_test.UserInfoTest": True,
        "xattr_test.XattrTestHg": True,
        #
        # Test classes from the hg integration test binary
        #
        "hg.debug_clear_local_caches_test.DebugClearLocalCachesTestTreeOnly": True,
        "hg.debug_get_parents.DebugGetParentsTestTreeOnly": True,
        "hg.debug_hg_dirstate_test.DebugHgDirstateTestTreeOnly": True,
        "hg.diff_test.DiffTestTreeOnly": True,
        "hg.grep_test.GrepTestTreeOnly": [
            "test_grep_directory_from_root",
            "test_grep_directory_from_subdirectory",
        ],
        "hg.rebase_test.RebaseTestTreeOnly": [
            "test_rebase_commit_with_independent_folder"
        ],
        "hg.rm_test.RmTestTreeOnly": [
            "test_rm_directory_with_modification",
            "test_rm_modified_file_permissions",
        ],
        "hg.split_test.SplitTestTreeOnly": ["test_split_one_commit_into_two"],
        "hg.status_deadlock_test.StatusDeadlockTestTreeOnly": True,
        "hg.status_test.StatusTestTreeOnly": [
            # TODO: Opening a file with O_TRUNC inside an EdenFS mount fails on Windows
            "test_partial_truncation_after_open_modifies_file",
            # TODO: These tests do not report the file as modified after truncation
            "test_truncation_after_open_modifies_file",
            "test_truncation_upon_open_modifies_file",
        ],
        "hg.update_test.UpdateCacheInvalidationTestTreeOnly": [
            "test_changing_file_contents_creates_new_inode_and_flushes_dcache"
        ],
        "hg.update_test.UpdateTestTreeOnly": [
            # TODO: A \r\n is used
            "test_mount_state_during_unmount_with_in_progress_checkout",
        ],
        "stale_inode_test.StaleInodeTestHgNFS": True,
        "windows_fsck_test.WindowsFsckTestHg": [
            # T146967686
            "test_detect_removed_file_from_dirty_placeholder_directory",
            "test_detect_removed_file_from_placeholder_directory",
        ],
    }
elif sys.platform.startswith("linux") and not os.path.exists("/etc/redhat-release"):
    # The ChownTest.setUp() code tries to look up the "nobody" group, which doesn't
    # exist on Ubuntu.
    TEST_DISABLED["chown_test.ChownTest"] = True

    # These tests try to run "hg whereami", which isn't available on Ubuntu.
    # This command is provided by the scm telemetry wrapper rather than by hg
    # itself, and we currently don't install the telemetry wrapper on Ubuntu.
    TEST_DISABLED["hg.doctor_test.DoctorTestTreeOnly"] = [
        "test_eden_doctor_fixes_valid_mismatched_parents",
    ]

    TEST_DISABLED["hg.post_clone_test.SymlinkTestTreeOnly"] = [
        # This test fails with mismatched permissions (0775 vs 0755).
        # I haven't investigated too closely but it could be a umask configuration
        # issue.
        "test_post_clone_permissions"
    ]
elif sys.platform.startswith("darwin"):
    # OSERROR AF_UNIX path too long
    TEST_DISABLED["hg.status_test.StatusTestTreeOnly"] = [
        "test_status",
        "test_status_thrift_apis",
    ]

    # update fails because new file created while checkout operation in progress
    TEST_DISABLED["hg.update_test.UpdateTestTreeOnly"] = [
        "test_change_casing_with_untracked",
    ]

    # The remaining tests are failing due to Mercurial issue in asciitransform
    TEST_DISABLED["hg.add_test.AddTestTreeOnly"] = [
        "test_debugdirstate",
        "test_add",
        "test_add_file_that_would_normally_be_ignored",
        "test_add_ignored_directory_has_no_effect",
    ]
    TEST_DISABLED["hg.files_test.FilesTestTreeOnly"] = [
        "test_files_with_changes",
    ]
    TEST_DISABLED["hg.merge_test.MergeTestTreeOnly"] = [
        "test_resolve_merge",
    ]
    TEST_DISABLED["hg.move_test.MoveTestTreeOnly"] = [
        "test_replace_after_move_file_then_revert_it"
    ]
    TEST_DISABLED["hg.rebase_test.RebaseTestTreeOnly"] = [
        "test_rebase_stack_with_conflicts",
    ]
    TEST_DISABLED["hg.revert_test.RevertTestTreeOnly"] = [
        "test_revert_during_merge_resolution_succeeds"
    ]
    TEST_DISABLED["hg.status_test.StatusTestTreeOnly"] = [
        "test_status_thrift_apis",
        "test_status",
    ]
    TEST_DISABLED["hg.update_test.UpdateTestTreeOnly"] = [
        "test_update_clean_removes_added_and_removed_statuses",
    ]
    TEST_DISABLED["hg.split_test.SplitTestTreeOnly"] = True

    # hg tests with misc failures
    TEST_DISABLED["hg.debug_hg_dirstate_test.DebugHgDirstateTestTreeOnly"] = True

    # Assertion error and invalid argument
    TEST_DISABLED["snapshot.test_snapshots.InfraTestsDefault"] = [
        "test_snapshot",
        "test_verify_directory",
    ]
    TEST_DISABLED["snapshot.test_snapshots.Testbasic-20210712"] = True

    TEST_DISABLED["basic_test.PosixTest"] = [
        "test_create_using_mknod",  # PermissionDenied
        "test_statvfs",  # NFS block size appears to be too small.
    ]

    # `eden chown` requires the use of `sudo` for chowning redirections. We
    # don't have access to passwordless `sudo` on macOS Sandcastle hosts, so
    # we should disable these test.
    TEST_DISABLED["chown_test.ChownTest"] = True

    # T89441739
    TEST_DISABLED["corrupt_overlay_test.CorruptOverlayTest"] = [
        "test_unmount_succeeds",
        "test_unlink_deletes_corrupted_files",
    ]

    # CalledProcessError
    TEST_DISABLED["health_test.HealthOfFakeEdenFSTest"] = True

    # eden clone fails bc Git not supported?
    TEST_DISABLED["remount_test.RemountTest"] = [
        "test_git_and_hg",
    ]

    # EOF error?
    TEST_DISABLED["restart_test.RestartTestAdHoc"] = [
        "test_eden_restart_fails_if_edenfs_crashes_on_start",
        # timeout
        "test_restart_starts_edenfs_if_not_running",
        # timeout
        "test_restart_while_starting",
    ]
    TEST_DISABLED["restart_test.RestartTestManaged"] = [
        "test_eden_restart_fails_if_edenfs_crashes_on_start",
        # timeout
        "test_restart_starts_edenfs_if_not_running",
    ]

    # timeout
    TEST_DISABLED["restart_test.RestartTest"] = [
        "test_graceful_restart_unresponsive_thrift",
        "test_restart",
    ]

    # Broken on NFS since NFS will just hang with the message "nfs server
    # edenfs:: not responding". There was an attempt to fix this with commit
    # 1f5512cf74ca, but it seems like something in test teardown is causing
    # hangs still.
    TEST_DISABLED["stale_test.StaleTest"] = True

    # CalledProcessError (same as health_test above?). Seems like
    # FakeEdenFS is broken on MacOS
    TEST_DISABLED["service_log_test.ServiceLogFakeEdenFSTest"] = True
    TEST_DISABLED["service_log_test.ServiceLogRealEdenFSTest"] = True

    # Expect OSError but does not happen (T89439721)
    TEST_DISABLED["setattr_test.SetAttrTest"] = [
        "test_chown_gid_as_nonroot_fails_if_not_member",
        "test_chown_uid_as_nonroot_fails",
        "test_setuid_setgid_and_sticky_bits_fail_with_eperm",
    ]

    # On NFS, we don't have the per-mount live_request counters implemented.
    # We can enable this test once those are added (T147665665)
    TEST_DISABLED["stats_test.GenericStatsTest"] = [
        "test_summary_counters_available",
    ]

    # Various errors (See NFS specific skips for more takeover failures)
    TEST_DISABLED["takeover_test.TakeoverTestHg"] = True
    TEST_DISABLED["takeover_test.TakeoverTestNoNFSServer"] = [
        "test_takeover",
    ]

    # T89440575: We aren't able to get fetch counts by PID on NFS, which doesn't
    # provide any information about client processes.
    TEST_DISABLED["thrift_test.ThriftTest"] = [
        "test_pid_fetch_counts",
    ]

    # OSError: AF_UNIX path too long
    TEST_DISABLED["unixsocket_test.UnixSocketTest"] = True

    # EdenFS on macOS uses NFSv3, which doesn't support extended attributes.
    TEST_DISABLED["xattr_test.XattrTest"] = True

    # fsck doesn't work on macOS?
    TEST_DISABLED["fsck.basic_snapshot_tests.Basic20210712TestDefault"] = True

    # flakey (actual timing doesn't match expected timing)
    TEST_DISABLED["config_test.ConfigTest"] = True

# Windows specific tests
if sys.platform != "win32":
    TEST_DISABLED.update(
        {
            "invalidate_test.InvalidateTest": True,
            "windows_fsck_test.WindowsFsckTest": True,
            "windows_fsck_test.WindowsRebuildOverlayTest": True,
            "prjfs_stress.PrjFSStress": True,
            "prjfs_stress.PrjfsStressNoListenToFull": True,
            "projfs_buffer.PrjFSBuffer": True,
        }
    )

# We only run tests on linux currently, so we only need to disable them there.
if sys.platform.startswith("linux"):
    # tests to skip on nfs, this list allows us to avoid writing the nfs postfix
    # on the test and disables them for both Hg and Git as nfs tests generally
    # fail for both if they fail.
    NFS_TEST_DISABLED = {
        "takeover_test.TakeoverTest": [
            "test_takeover_doesnt_send_ping"  # this test uses protocol 3 of
            # graceful restart. Version 3 does not support restarting
            # NFS mounts. So we inherently expect this test to fail on
            # NFS.
        ],
        # EdenFS's NFS implementation is NFSv3, which doesn't support extended
        # attributes.
        "xattr_test.XattrTest": [  # T89439481
            "test_get_sha1_xattr",
            "test_get_sha1_xattr_succeeds_after_querying_xattr_on_dir",
        ],
        "setattr_test.SetAttrTest": [  # T89439721
            "test_chown_gid_as_nonroot_fails_if_not_member",
            "test_chown_uid_as_nonroot_fails",
            "test_setuid_setgid_and_sticky_bits_fail_with_eperm",
        ],
        "stats_test.GenericStatsTest": [
            # On NFS, we don't have the per-mount live_request counters.
            # We can enable this test once those are added (T147665665).
            "test_summary_counters_available",
        ],
        "stats_test.CountersTest": [
            # Same as above test: (T147665665).
            "test_mount_unmount_counters"
        ],
        # T89440575: We aren't able to get fetch counts by PID on NFS, which
        # doesn't provide any information about client processes.
        "thrift_test.ThriftTest": ["test_pid_fetch_counts"],
        "mount_test.MountTest": [  # T91790656
            "test_unmount_succeeds_while_file_handle_is_open",
            "test_unmount_succeeds_while_dir_handle_is_open",
        ],
    }

    for (testModule, disabled) in NFS_TEST_DISABLED.items():
        for vcs in ["Hg", "Git"]:
            TEST_DISABLED[testModule + "NFS" + vcs] = disabled

    # custom nfs tests that don't run on both hg and git that we also need to
    # disable
    TEST_DISABLED.update(
        {
            "corrupt_overlay_test.CorruptOverlayTestNFS": [  # T89441739
                "test_unlink_deletes_corrupted_files",
                "test_unmount_succeeds",
            ],
            "fsck_test.FsckTestNFS": [  # T89442010
                "test_fsck_force_and_check_only",
                "test_fsck_multiple_mounts",
            ],
            "stale_test.StaleTestNFS": True,  # T89442539
            "hg.debug_clear_local_caches_test.DebugClearLocalCachesTestTreeOnlyNFS": [
                "test_contents_are_the_same_if_handle_is_held_open"  # T89344844
            ],
            "hg.update_test.UpdateTestTreeOnlyNFS": [
                "test_mount_state_during_unmount_with_in_progress_checkout"  # T90881795
            ],
        }
    )

if "SANDCASTLE" in os.environ:
    # This test seems to leave behind unkillable processes on sandcastle.
    # Disable it for now.
    TEST_DISABLED["hg.update_test.UpdateTest"] = ["test_dir_locking"]

try:
    from eden.integration.facebook.lib.skip import add_fb_specific_skips

    add_fb_specific_skips(TEST_DISABLED)
except ImportError:
    pass


def is_class_disabled(class_name: str) -> bool:
    class_skipped = TEST_DISABLED.get(class_name)
    if class_skipped is None:
        return False
    if isinstance(class_skipped, bool):
        assert class_skipped is True
        # All classes in the test are skipped
        return True
    return False


def is_method_disabled(class_name: str, method_name: str) -> bool:
    method_skipped = TEST_DISABLED.get(class_name)
    if method_skipped is None:
        return False
    assert isinstance(method_skipped, list)
    return method_name in method_skipped
