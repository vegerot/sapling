/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::borrow::Cow;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::path::PathBuf;

use winapi::shared::minwindef::DWORD;
use winapi::shared::ntdef::LPSTR;
use winapi::um::stringapiset::MultiByteToWideChar;
use winapi::um::stringapiset::WideCharToMultiByte;
use winapi::um::winnls::CP_ACP;

const MB_ERR_INVALID_CHARS: DWORD = 0x00000008;
const WC_COMPOSITECHECK: DWORD = 0x00000200;

/// Convert bytes in the local encoding to an `OsStr`.
///
/// On Unix, this is a zero-copy operation and cannot fail.  The encoding of the `OsStr` matches
/// that of the original local bytes.
///
/// On Windows, it uses the ANSI Code Page (ACP) to perform the conversion to UTF-16,
/// which is then stored in an `OsString`.  Note that unlike the function in `local_encoding`,
/// this function does not intermediately convert to a Unicode `String`, therefore it is
/// "more native" from Windows' perspective and is more performant.
#[inline]
pub fn local_bytes_to_osstring(bytes: &[u8]) -> io::Result<Cow<OsStr>> {
    if bytes.len() == 0 {
        return Ok(Cow::Owned(OsString::new()));
    }
    let codepage = CP_ACP;
    let len = unsafe {
        MultiByteToWideChar(
            codepage,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr() as LPSTR,
            bytes.len() as i32,
            std::ptr::null_mut(),
            0,
        )
    };
    if len == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut wide: Vec<u16> = Vec::with_capacity(len as usize);
    let len = unsafe {
        wide.set_len(len as usize);
        MultiByteToWideChar(
            codepage,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr() as LPSTR,
            bytes.len() as i32,
            wide.as_mut_ptr(),
            len,
        )
    };
    if len as usize == wide.len() {
        Ok(Cow::Owned(OsString::from_wide(&wide)))
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Convert bytes in the local encoding to a `Path`.
///
/// On Unix, this is a zero-copy operation and cannot fail.
///
/// On Windows, this converts the local bytes to an `OsString` and then converts the
/// `OsString` to a `PathBuf`, possibly returning the same errors as `local_bytes_to_osstring`.
///
/// Note that local bytes are what Mercurial stores in manifests, and are affected
/// by the "Language for non-Unicode programs" setting on Windows at commit time.
/// New programs should normalize paths to UTF-8 before storing them.
#[inline]
pub fn local_bytes_to_path(bytes: &[u8]) -> io::Result<Cow<Path>> {
    Ok(Cow::Owned(PathBuf::from(
        local_bytes_to_osstring(bytes)?.into_owned(),
    )))
}

/// Convert an `OsStr` to bytes in the local encoding.
///
/// On Unix, this is a zero-copy operation and cannot fail.  The encoding of the local bytes
/// matches that of the original `OsStr`.
///
/// On Windows, it uses the ANSI Code Page (ACP) to perform the conversion
/// into bytes.  Note that unlike the function in `local_encoding`, this function
/// does not intermediately convert to a Unicode `String`, therefore it is "more native"
/// from Windows' perspective and is more performant.
#[inline]
pub fn osstring_to_local_bytes(s: &OsStr) -> io::Result<Cow<[u8]>> {
    let codepage = CP_ACP;
    if s.len() == 0 {
        return Ok(Cow::Owned(Vec::new()));
    }
    let wstr: Vec<u16> = s.encode_wide().collect();
    let len = unsafe {
        WideCharToMultiByte(
            codepage,
            WC_COMPOSITECHECK,
            wstr.as_ptr(),
            wstr.len() as i32,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
        )
    };
    if len == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut astr: Vec<u8> = Vec::with_capacity(len as usize);
    let len = unsafe {
        astr.set_len(len as usize);
        WideCharToMultiByte(
            codepage,
            WC_COMPOSITECHECK,
            wstr.as_ptr(),
            wstr.len() as i32,
            astr.as_mut_ptr() as LPSTR,
            len,
            std::ptr::null(),
            std::ptr::null_mut(),
        )
    };
    if len as usize == astr.len() {
        Ok(Cow::Owned(astr))
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Convert a `Path` to bytes in the local encoding.
///
/// On Unix, this is a zero-copy operation and cannot fail.
///
/// On Windows, this converts the path to an `OsString` and then converts the
/// `OsString` to local bytes, possibly returning the same errors as
/// `osstring_to_local_bytes`.
///
/// Note that local bytes are what Mercurial stores in manifests, and are affected
/// by the "Language for non-Unicode programs" Windows setting at commit time.
/// New programs should normalize paths to UTF-8 before storing them.
#[inline]
pub fn path_to_local_bytes(path: &Path) -> io::Result<Cow<[u8]>> {
    osstring_to_local_bytes(&path.as_os_str())
}
