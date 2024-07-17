/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#![allow(non_camel_case_types)]

//! Python bindings for treestate.
//!
//! This is a Rust implementation of the dirstate concept for Mercurial, using a tree structure
//! in an append-only storage back-end.
//!
//! The directory state stores information for all files in a working copy that are of interest
//! to Mercurial.  In particular, for each file in the working copy it stores the mode flags,
//! size, and modification time of the file.  These can be compared with current values to
//! determine if the file has changed.
//!
//! The directory state also stores files that are in the working copy parent manifest but have
//! been marked as removed.

use std::sync::Arc;

use ::treestate::filestate::FileStateV2;
use ::treestate::filestate::StateFlags;
use ::treestate::store::BlockId;
use ::treestate::tree::VisitorResult;
use ::treestate::treestate::TreeState;
use anyhow::Error;
use cpython::*;
use cpython_ext::AnyhowResultExt;
use cpython_ext::PyPath;
use cpython_ext::PyPathBuf;
use cpython_ext::ResultPyErrExt;
use parking_lot::Mutex;
use pathmatcher::DirectoryMatch;
use pathmatcher::Matcher;
use pypathmatcher::PythonMatcher;
use types::RepoPathBuf;
use vfs::VFS;

type Result<T, E = Error> = std::result::Result<T, E>;

pub fn init_module(py: Python, package: &str) -> PyResult<PyModule> {
    let name = [package, "treestate"].join(".");
    let m = PyModule::new(py, &name)?;
    m.add_class::<treestate>(py)?;
    m.add(py, "EXIST_P1", StateFlags::EXIST_P1.to_bits())?;
    m.add(py, "EXIST_P2", StateFlags::EXIST_P2.to_bits())?;
    m.add(py, "EXIST_NEXT", StateFlags::EXIST_NEXT.to_bits())?;
    m.add(py, "IGNORED", StateFlags::IGNORED.to_bits())?;
    m.add(py, "NEED_CHECK", StateFlags::NEED_CHECK.to_bits())?;
    m.add(py, "COPIED", StateFlags::COPIED.to_bits())?;
    m.add(py, "tohgstate", py_fn!(py, flags_to_hg_state(flags: u16)))?;
    Ok(m)
}

impl treestate {
    pub fn get_state(&self, py: Python) -> Arc<Mutex<TreeState>> {
        self.state(py).clone()
    }
}

py_class!(pub class treestate |py| {
    data state: Arc<Mutex<TreeState>>;

    // This should only be used for hg doctor.
    @staticmethod
    def openraw(path: PyPathBuf, root_id: u64) -> PyResult<treestate> {
        let treestate = TreeState::open(
            path.as_path(),
            BlockId(root_id),
            VFS::new(path.as_path().to_path_buf()).map_pyerr(py)?.case_sensitive(),
        ).map_pyerr(py)?;
        Self::create_instance(py, Arc::new(Mutex::new(treestate)))
    }

    // This should only be used for tests.
    @staticmethod
    def new(directory: PyPathBuf) -> PyResult<treestate> {
        let treestate = TreeState::new(
            directory.as_path(),
            VFS::new(directory.as_path().to_path_buf()).map_pyerr(py)?.case_sensitive(),
        ).map_pyerr(py)?.0;
        Self::create_instance(py, Arc::new(Mutex::new(treestate)))
    }

    def flush(&self) -> PyResult<u64> {
        // Save changes to the existing file.
        let mut state = self.state(py).lock();
        let root_id = convert_result(py, state.flush())?;
        Ok(root_id.0)
    }

    def reset(&self) -> PyResult<u64> {
        let mut treestate = self.state(py).lock();
        let root_id = convert_result(py, treestate.reset())?;
        Ok(root_id.0)
    }

    def filename(&self) -> PyResult<String> {
        convert_result(py, self.state(py).lock().file_name())
    }

    def saveas(&self, directory: &PyPath) -> PyResult<u64> {
        // Save as a new file. Return `BlockId` that can be used in constructor.
        let mut state = self.state(py).lock();
        let root_id = convert_result(py, state.write_new(directory.as_path()))?;
        Ok(root_id.0)
    }

    def __len__(&self) -> PyResult<usize> {
        Ok(self.state(py).lock().len())
    }

    def __contains__(&self, path: PyPathBuf) -> PyResult<bool> {
        let mut state = self.state(py).lock();
        let file = convert_result(py, state.get(path.as_utf8_bytes()))?;
        // A lot of places require "__contains__(path)" to be "False" if "path" is "?" state
        let visible_flags = StateFlags::EXIST_P1 | StateFlags::EXIST_P2 | StateFlags::EXIST_NEXT;
        Ok(match file {
            Some(file) => file.state.intersects(visible_flags),
            None => false,
        })
    }

    def get(&self, path: &PyPath, default: Option<(u16, u32, i32, i32, Option<PyPathBuf>)>) -> PyResult<Option<(u16, u32, i32, i32, Option<PyPathBuf>)>> {
        let mut state = self.state(py).lock();
        let path = path.as_utf8_bytes();

        assert!(!path.ends_with(b"/"));

        let file = convert_result(py, state.get(path))?;
        Ok(file.map_or(default, |file|
                    Some((file.state.to_bits(),
                     file.mode,
                     file.size,
                     file.mtime,
                     file.copied.as_ref().map(|path| PyPathBuf::from_utf8_bytes(path.to_vec()).unwrap())))))
    }

    def insert(
        &self, path: &PyPath, bits: u16, mode: u32, size: i32, mtime: i32, copied: Option<PyPathBuf>
    ) -> PyResult<PyObject> {
        let mut flags = StateFlags::from_bits_truncate(bits);
        // For special mtime or size, mark them as "NEED_CHECK" automatically.
        if mtime < 0 || size < 0 {
            flags |= StateFlags::NEED_CHECK;
        }

        // Also fix-up COPIED bit so they stay consistent.
        if copied.as_ref().is_some() {
            flags |= StateFlags::COPIED;
        } else {
            flags -= StateFlags::COPIED;
        };

        let file = FileStateV2 { mode, size, mtime, copied: copied.map(|copied| copied.as_utf8_bytes().to_vec().into_boxed_slice()), state: flags };
        let path = path.as_utf8_bytes();
        let mut state = self.state(py).lock();
        convert_result(py, state.insert(path, &file))?;
        Ok(py.None())
    }

    def remove(&self, path: &PyPath) -> PyResult<bool> {
        let mut state = self.state(py).lock();
        convert_result(py, state.remove(path.as_utf8_bytes()))
    }

    def getdir(&self, path: &PyPath) -> PyResult<Option<(u16, u16)>> {
        let mut state = self.state(py).lock();
        let path = path.as_utf8_bytes();

        let dir = convert_result(py, state.get_dir(path))?;
        Ok(dir.map(|state| (state.union.to_bits(), state.intersection.to_bits())))
    }

    def hasdir(&self, path: &PyPath) -> PyResult<bool> {
        let mut state = self.state(py).lock();
        let path = path.as_utf8_bytes();
        convert_result(py, state.has_dir(path))
    }

    def walk(
        &self,
        setbits: u16,
        unsetbits: u16,
        dirfilter: Option<PyObject> = None
    ) -> PyResult<Vec<PyPathBuf>> {
        // Get all file paths with `setbits` all set, `unsetbits` all unset.
        // If dirfilter is provided. It is a callable that takes a directory path, and returns True
        // if the path should be skipped.
        assert_eq!(setbits & unsetbits, 0, "setbits cannot overlap with unsetbits");
        let setbits = StateFlags::from_bits_truncate(setbits);
        let unsetbits = StateFlags::from_bits_truncate(unsetbits);
        let mask = setbits | unsetbits;
        let mut state = self.state(py).lock();
        let mut result = Vec::new();
        convert_result(py, state.visit(
            &mut |components, _state| {
                let path = PyPathBuf::from_utf8_bytes(components.concat()).expect("path should be utf-8");
                result.push(path);
                Ok(VisitorResult::NotChanged)
            },
            &|components, dir| {
                if let Some(state) = dir.get_aggregated_state() {
                    if !state.union.contains(setbits) || state.intersection.intersects(unsetbits) {
                        return false;
                    }
                }
                if let Some(ref dirfilter) = dirfilter {
                    let path = PyPathBuf::from_utf8_bytes(components.concat()).expect("path should be utf-8");
                    if let Ok(result) = dirfilter.call(py, (path,), None) {
                        if let Ok(result) = result.is_true(py) {
                            if result {  // should skip
                                return false;  // do not visit
                            }
                        }
                    }
                }
                true  // do visit
            },
            &|_, file| file.state & mask == setbits,
        ))?;
        Ok(result)
    }

    /// Tracked files filtered by the matcher.
    def matches(&self, matcher: PyObject) -> PyResult<Vec<PyPathBuf>> {
        let matcher = PythonMatcher::new(py, matcher);

        let mut state = self.state(py).lock();
        let mut result = Vec::new();
        let mask = StateFlags::EXIST_P1 | StateFlags::EXIST_P2 | StateFlags::EXIST_NEXT;
        convert_result(py, state.visit(
            &mut |components, _state| {
                let path = RepoPathBuf::from_utf8(components.concat()).expect("path should be utf-8");
                if  matcher.matches_file(&path)? {
                    let path = PyPathBuf::from_string(path.into_string());
                    result.push(path);
                }
                Ok(VisitorResult::NotChanged)
            },
            &|components, dir| {
                if let Some(state) = dir.get_aggregated_state() {
                    if !state.union.intersects(mask) {
                        return false;
                    }
                }
                let mut binary_path = components.concat();
                // Remove the trailing slash.
                assert_eq!(binary_path.pop().unwrap_or(b'/'), b'/');
                let path = RepoPathBuf::from_utf8(binary_path).expect("path should be utf-8");
                match matcher.matches_directory(&path) {
                    Ok(DirectoryMatch::Nothing) => false,  // do not visit
                    _ => true,  // do visit
                }
            },
            &|_, file| file.state.intersects(mask),
        ))?;
        Ok(result)
    }

    def tracked(&self, prefix: &PyPath) -> PyResult<Vec<PyPathBuf>> {
        // prefix limits the result to given prefix (ex. ["dir1/", "dir2/"]). To get all tracked
        // files, set prefix to an empty list.
        // Not ideal as a special case. But the returned list is large and it needs to be fast.
        // It's basically walk(EXIST_P1, 0) + walk(EXIST_P2, 0) + walk(EXIST_NEXT).
        let mut state = self.state(py).lock();
        let mut result = Vec::new();
        let mask = StateFlags::EXIST_P1 | StateFlags::EXIST_P2 | StateFlags::EXIST_NEXT;
        let prefix = split_path(prefix.as_utf8_bytes());
        convert_result(py, state.visit(
            &mut |components, _state| {
                let path = PyPathBuf::from_utf8_bytes(components.concat()).expect("path should be utf-8");
                result.push(path);
                Ok(VisitorResult::NotChanged)
            },
            &|path, dir| {
                if path.iter().zip(prefix.iter()).any(|(a, b)| a != b) {
                    // common prefix does not match
                    false
                } else {
                    match dir.get_aggregated_state() {
                        None => true,
                        Some(state) => state.union.intersects(mask),
                    }
                }
            },
            &|path, file| {
                if path.len() == prefix.len() {
                    // must be an exact match
                    *path == prefix
                } else if path.len() < prefix.len() {
                    // file outside given prefix
                    false
                } else {
                    file.state.intersects(mask)
                }
            }
        ))?;
        Ok(result)
    }

    def getfiltered(
        &self, path: &PyPath, filter: PyObject, filterid: u64
    ) -> PyResult<Vec<PyPathBuf>> {
        let mut state = self.state(py).lock();

        let result = convert_result(py, state.get_filtered_key(
            path.as_utf8_bytes(),
            &mut |path| {
                let path = PyPathBuf::from_utf8_bytes(path.to_vec()).expect("path should be utf-8");
                let filtered = filter
                    .call(py, (&path,), None).into_anyhow_result()?
                    .extract::<PyPathBuf>(py).into_anyhow_result()?;
                Ok(filtered.into_utf8_bytes().into_boxed_slice())
            },
            filterid,
        ))?;

        Ok(result.into_iter().map(|o| PyPathBuf::from_utf8_bytes(o.into_vec()).expect("path should be utf-8")).collect())
    }

    def pathcomplete(
        &self, prefix: &PyPath, setbits: u16, unsetbits: u16, matchcallback: PyObject,
        fullpaths: bool
    ) -> PyResult<PyObject> {
        let setbits = StateFlags::from_bits_truncate(setbits);
        let unsetbits = StateFlags::from_bits_truncate(unsetbits);
        let mask = setbits | unsetbits;
        let mut state = self.state(py).lock();
        let prefix = prefix.as_utf8_bytes();

        convert_result(py, state.path_complete(
            prefix,
            fullpaths,
            &|file| file.state & mask == setbits,
            &mut |components| {
                let path = PyPathBuf::from_utf8_bytes(components.concat()).expect("path should be utf-8");
                matchcallback.call(py, (path,), None).into_anyhow_result()?;
                Ok(())
            },
        ))?;

        Ok(py.None())
    }

    // Import another map of dirstate tuples into this treestate. Note: copymap is not imported.
    def importmap(&self, old_map: PyObject) -> PyResult<Option<PyObject>> {
        let mut tree = self.state(py).lock();
        let items = old_map.call_method(py, "items", NoArgs, None)?;
        let iter = PyIterator::from_object(
            py, items.call_method(py, "__iter__", NoArgs, None)?)?;

        for item in iter {
            let item_tuple = item?.extract::<PyTuple>(py)?;
            let path = item_tuple.get_item(py, 0).extract::<PyPathBuf>(py)?;
            let data = item_tuple.get_item(py, 1).extract::<PySequence>(py)?;
            let state = data.get_item(py, 0)?.extract::<PyString>(py)?.data(py).to_string(py)?.bytes().next().unwrap();
            let mode = data.get_item(py, 1)?.extract::<u32>(py)?;
            let size = data.get_item(py, 2)?.extract::<i32>(py)?;
            let mtime = data.get_item(py, 3)?.extract::<i32>(py)?;
            // Mercurial uses special "size"s to represent "otherparent" if state is "n".
            // See "size = -2" in mercurial/dirstate.py
            let flags = match size {
                -2 => StateFlags::EXIST_P2,
                _ => StateFlags::EXIST_P1,
            };
            let flags = match state {
                b'n' => flags | StateFlags::EXIST_NEXT,
                b'm' => StateFlags::EXIST_P1 | StateFlags::EXIST_P2 | StateFlags::EXIST_NEXT,
                b'r' => flags,
                b'a' => StateFlags::EXIST_NEXT,
                _ => StateFlags::empty(),
            };
            if !flags.is_empty() {
                let file = FileStateV2 { mode, size, mtime, copied: None, state: flags };
                convert_result(py, tree.insert(path.as_utf8_bytes(), &file))?;
            }
        }
        Ok(None)
    }

    def invalidatemtime(&self, fsnow: i32) -> PyResult<PyObject> {
        let mut state = self.state(py).lock();
        convert_result(py, state.invalidate_mtime(fsnow))?;
        Ok(py.None())
    }

    def getmetadata(&self) -> PyResult<PyBytes> {
        let state = self.state(py).lock();
        let metadata = PyBytes::new(py, state.metadata_bytes());
        Ok(metadata)
    }

    def setmetadata(&self, metadata: PyBytes) -> PyResult<PyObject> {
        let mut state = self.state(py).lock();
        let metadata = metadata.data(py);
        state.set_metadata_bytes(metadata);
        Ok(py.None())
    }
});

/// Convert StateFlags to Mercurial dirstate state
fn flags_to_hg_state(_py: Python, flags: u16) -> PyResult<&'static str> {
    let flags = StateFlags::from_bits_truncate(flags);
    Ok(
        match (
            flags.intersects(StateFlags::EXIST_P1 | StateFlags::EXIST_P2),
            flags.contains(StateFlags::EXIST_P1 | StateFlags::EXIST_P2),
            flags.contains(StateFlags::EXIST_NEXT),
        ) {
            (true, true, true) => "m",  // merge
            (true, false, true) => "n", // normal
            (true, _, false) => "r",    // remove
            (false, _, true) => "a",    // add
            (false, _, false) => "?",   // untracked
        },
    )
}

/// Convert a Result to PyResult
fn convert_result<T>(py: Python, result: Result<T>) -> PyResult<T> {
    result.map_pyerr(py)
}

/// Convert "dir1/dir2/file1" to ["dir1/", "dir2/", "file1"]
fn split_path(path: &[u8]) -> Vec<&[u8]> {
    // convert prefix to a vec like ["dir/", "dir2/", "file"]
    if path == b"/" {
        return Vec::new();
    }
    let mut components = Vec::new();
    let mut next_index = 0;
    for (index, byte) in path.iter().enumerate() {
        if *byte == b'/' {
            components.push(&path[next_index..index + 1]);
            next_index = index + 1;
        }
    }
    if next_index < path.len() {
        components.push(&path[next_index..]);
    }
    components
}
