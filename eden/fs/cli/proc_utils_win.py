#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-strict

import ctypes
import datetime
import sys
import types
from ctypes.wintypes import (
    BOOL as _BOOL,
    DWORD as _DWORD,
    HANDLE as _HANDLE,
    LPDWORD as _LPDWORD,
    LPWSTR as _LPWSTR,
    PDWORD as _PDWORD,
    PHANDLE as _PHANDLE,
)
from pathlib import Path
from typing import Iterable, NoReturn, Optional, Type

from . import proc_utils


if sys.platform == "win32":
    _win32 = ctypes.windll.kernel32
    _win32.OpenProcess.argtypes = [_DWORD, _BOOL, _DWORD]
    _win32.OpenProcess.restype = _HANDLE

    _win32.GetExitCodeProcess.argtypes = [_HANDLE, _LPDWORD]
    _win32.GetExitCodeProcess.restype = _BOOL

    psapi = ctypes.windll.psapi
    psapi.GetProcessImageFileNameW.argstypes = [_HANDLE, _LPWSTR, _DWORD]
    psapi.GetProcessImageFileNameW.restype = _DWORD

    advapi32 = ctypes.windll.advapi32

    def raise_win_error() -> NoReturn:
        raise ctypes.WinError()

else:
    # This entire file is only ever imported in Windows.  However on our continuous
    # integration environments Pyre currently does all of its type checking assuming
    # Linux.  Define a fake kernel32 module just to allow it to still perform some type
    # checking for us.
    class _win32:
        @staticmethod
        def OpenProcess(
            desired_access: int, inherit_handle: bool, pid: int
        ) -> _HANDLE: ...

        @staticmethod
        def OpenProcessToken(
            handle: _HANDLE, desired_access: int, token_handle: _PHANDLE
        ) -> bool: ...

        @staticmethod
        def CloseHandle(handle: _HANDLE) -> None: ...

        @staticmethod
        def TerminateProcess(handle: _HANDLE, exit_code: int) -> bool: ...

        @staticmethod
        def GetExitCodeProcess(handle: _HANDLE, exit_code: _LPDWORD) -> _BOOL: ...

    class psapi:
        @staticmethod
        def GetProcessImageFileNameW(
            handle: _HANDLE, fileName: _LPWSTR, size: _DWORD
        ) -> _DWORD: ...

    class advapi32:
        @staticmethod
        def GetTokenInformation(
            token_handle: _HANDLE,
            token_information_class: int,
            token_information: ctypes._CArgObject,
            token_information_length: _DWORD,
            return_length: _PDWORD,
        ) -> bool: ...

    def raise_win_error() -> NoReturn: ...


_PROCESS_TERMINATE = 0x0001
_PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
_ERROR_ACCESS_DENIED = 5
_CREATE_NO_WINDOW = 0x08000000
_TOKEN_READ = 0x00020008
_TOKEN_ELEVATION = 20


class Handle:
    handle: _HANDLE

    def __init__(self, handle: _HANDLE) -> None:
        self.handle = handle

    @classmethod
    def make_empty(klass) -> "Handle":
        return klass(_HANDLE())

    def asref(self) -> "ctypes._Pointer[ctypes.c_void_p]":
        return ctypes.pointer(self.handle)

    def __enter__(self) -> "Handle":
        return self

    def __exit__(
        self,
        exc_type: Optional[Type[BaseException]],
        exc_value: Optional[BaseException],
        tb: Optional[types.TracebackType],
    ) -> None:
        self.close()

    def close(self) -> None:
        if self.handle:
            if _win32.CloseHandle(self.handle) == 0:
                raise_win_error()
            self.handle = _HANDLE()


def open_process(pid: int, access: int = _PROCESS_QUERY_LIMITED_INFORMATION) -> Handle:
    handle_value = _win32.OpenProcess(access, False, pid)
    if handle_value is None:
        raise_win_error()
    return Handle(handle_value)


def open_process_token(process_handle: Handle) -> Handle:
    token_handle = Handle.make_empty()
    if not _win32.OpenProcessToken(
        process_handle.handle,
        _TOKEN_READ,
        # pyre-fixme[6]: For 3rd argument expected `_PHANDLE` but got
        #  `_Pointer[c_void_p]`.
        token_handle.asref(),
    ):
        raise_win_error()

    return token_handle


def is_token_elevated(token_handle: Handle) -> bool:
    elevation = ctypes.c_int(0)
    output_size = _DWORD(0)
    if (
        advapi32.GetTokenInformation(
            token_handle.handle,
            _TOKEN_ELEVATION,
            ctypes.byref(elevation),
            ctypes.c_ulong(ctypes.sizeof(elevation)),
            # pyre-fixme[6]: For 5th argument expected `_PDWORD` but got
            #  `_Pointer[c_ulong]`.
            ctypes.pointer(output_size),
        )
        == 0
    ):
        raise_win_error()

    return elevation.value != 0


def get_process_name(handle: Handle) -> str:
    MAX_PATH = 260  # https://docs.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation?tabs=cmd
    name = ctypes.create_unicode_buffer(MAX_PATH)
    if (
        psapi.GetProcessImageFileNameW(
            handle.handle, ctypes.cast(name, _LPWSTR), _DWORD(MAX_PATH)
        )
        == 0
    ):
        raise_win_error()

    return name.value


def get_exit_code(handle: Handle) -> Optional[int]:
    """returns the integer exit code of a process iff that process has
    completed otherwise returns None.
    """
    STILL_ACTIVE = 259
    exit_code = _DWORD()
    # pyre-fixme[6]: For 2nd argument expected `_LPDWORD` but got `_Pointer[c_ulong]`.
    if _win32.GetExitCodeProcess(handle.handle, ctypes.pointer(exit_code)) == 0:
        raise_win_error()

    if exit_code.value == STILL_ACTIVE:
        return None
    return int(exit_code.value)


class WinProcUtils(proc_utils.ProcUtils):
    def get_edenfs_processes(self) -> Iterable[proc_utils.EdenFSProcess]:
        # TODO: Finding all EdenFS processes is not yet implemented on Windows
        # This function is primarily used by `eden doctor` and other tools looking for
        # stale EdenFS instances on the system.  Returning an empty list for now will
        # allow those tools to run but just not find any stale processes.
        return []

    def get_process_start_time(self, pid: int) -> float:
        raise NotImplementedError(
            "Windows does not currently implement get_process_start_time()"
        )

    def is_system_idle(
        self, tty_idle_timeout: datetime.timedelta, root_path: Path
    ) -> bool:
        raise NotImplementedError(
            "Windows does not currently implement is_system_idle()"
        )

    def kill_process(self, pid: int) -> None:
        with open_process(pid, _PROCESS_TERMINATE) as p:
            exit_code = 1
            if not _win32.TerminateProcess(p.handle, exit_code):
                raise_win_error()

    def is_process_alive(self, pid: int) -> bool:
        """Returns if a process is currently running."""
        try:
            with open_process(pid) as handle:
                return get_exit_code(handle) is None
        except PermissionError:
            # The process exists, but we don't have permission to query it.
            return True
        except OSError:
            return False

    def is_edenfs_process(self, pid: int) -> bool:
        """Returns true iff pid references a currently running edenfs process
        otherwise returns false.
        """
        try:
            with open_process(pid) as handle:
                # If the process has an exit code then it is not running
                # and the process can not be a functioning edenfs instance
                if get_exit_code(handle) is not None:
                    return False

                name = get_process_name(handle)
                if name is None:
                    return False
                return name.endswith("edenfs.exe")
        except Exception:
            return False
