/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! manifest - The contents of the repository at a specific commit.
//!
//! The history of the repository is recorded in the commit graph. Each commit has a manifest
//! associated with it. The manifest specifies the revision for all the files available in the
//! repository. The file path and file revision are then used to retrieve the contents of the
//! file thus achieving the reconstruction of the entire repository state.

#[cfg(any(test, feature = "for-tests"))]
pub mod testutil;

use anyhow::Result;
use pathmatcher::Matcher;
#[cfg(any(test, feature = "for-tests"))]
use quickcheck_arbitrary_derive::Arbitrary;
use types::HgId;
use types::PathComponentBuf;
use types::RepoPath;
use types::RepoPathBuf;

/// Manifest describes a mapping between file path ([`String`]) and file metadata ([`FileMetadata`]).
/// Fundamentally it is just a Map<file_path, file_metadata>.
///
/// It can be assumed that Manifest interacts with an underlying store for persistance. These
/// interactions may fail due to a variety of reasons. Such failures will be propagated up as Error
/// return statuses.
///
/// Another common failure is passing in a path that the manifest has labeled as a directory. File
/// paths composed of directory names and file names. Querying for paths that the Manifest has
/// determined previously to be directories will result in Errors.
// TODO: Add method for batch modification, takes iterator of added, removed, changed, or
// maybe (path, Option<FileMetadata>) where None signals removal.
// TODO: A batch API allows us to move to having all nodes have a computed hash without losing
// performance. It also allows us to get rid of the flush method.
pub trait Manifest {
    /// Inspects the manifest for the given path. Returns available metadata.
    /// If the path is pointing to an file then Some(FsNodeMetadata::File) is returned with then
    /// file_metadata associated with the file. If the path is poitning to a directory then
    /// Some(FsNodeMetadata::Directory) is returned. If the path is not found then None is
    /// returned.
    // TODO: add default implementation
    fn get(&self, path: &RepoPath) -> Result<Option<FsNodeMetadata>>;

    fn get_ignore_case(&self, path: &RepoPath) -> Result<Option<FsNodeMetadata>>;

    /// Lists the immediate contents of directory in a manifest (non-recursive).
    /// Given a path, the manifest will return:
    /// * List::NotFound when the path is not present in the manifest
    /// * List::File when the path points to a file
    /// * List::Directory when the path points to a directory
    ///    wraps the names of the files and directories in this directory
    // TODO: add default implementation
    fn list(&self, path: &RepoPath) -> Result<List>;

    /// Associates a file path with specific file metadata.
    /// A call with a file path that already exists results in an override or the old metadata.
    fn insert(&mut self, file_path: RepoPathBuf, file_metadata: FileMetadata) -> Result<()>;

    /// Removes a file from the manifest (equivalent to removing it from the repository).
    /// A call with a file path that does not exist in the manifest is a no-op.
    fn remove(&mut self, file_path: &RepoPath) -> Result<Option<FileMetadata>>;

    /// Persists the manifest so that it can be retrieved at a later time. Returns a note
    /// representing the identifier for saved manifest.
    fn flush(&mut self) -> Result<HgId>;

    /// Retrieve the FileMetadata associated with a path.
    /// Paths that were not set will return None.
    fn get_file(&self, file_path: &RepoPath) -> Result<Option<FileMetadata>> {
        let result = self.get(file_path)?.and_then(|fs_hgid| match fs_hgid {
            FsNodeMetadata::File(file_metadata) => Some(file_metadata),
            FsNodeMetadata::Directory(_) => None,
        });
        Ok(result)
    }

    /// Returns an iterator over all the files in the Manifest that satisfy the given Matcher.
    fn files<'a, M: 'static + Matcher + Sync + Send>(
        &'a self,
        matcher: M,
    ) -> Box<dyn Iterator<Item = Result<File>> + 'a>;

    /// Returns an iterator over all directories found in the paths of the files in the Manifest
    /// that satisfy the given Matcher.
    // TODO: add default implementation
    fn dirs<'a, M: 'static + Matcher + Sync + Send>(
        &'a self,
        matcher: M,
    ) -> Box<dyn Iterator<Item = Result<Directory>> + 'a>;

    /// Retuns an iterator of all the differences in files between two Manifest instances of the
    /// same type.
    // TODO: add default implementation
    fn diff<'a, M: Matcher>(
        &'a self,
        other: &'a Self,
        matcher: &'a M,
    ) -> Result<Box<dyn Iterator<Item = Result<DiffEntry>> + 'a>>;

    /// Calculates modified directories between two Manifests.
    ///
    /// A directory is considered "modified" if:
    /// - It only exists in one Manifest.
    /// - It exists in both Manifests. A direct sub-item (file or dir)
    ///   is added, removed, or renamed.
    ///
    /// Similar to when the OS needs to update directory mtime.
    /// Modifying content of a file alone does not count as modifying its
    /// parent directory.
    fn modified_dirs<'a, M: Matcher>(
        &'a self,
        other: &'a Self,
        matcher: &'a M,
    ) -> Result<Box<dyn Iterator<Item = Result<DirDiffEntry>> + 'a>>;
}

/// The result of a list operation. Given a path, the manifest will return:
/// * List::NotFound when the path is not present in the manifest
/// * List::File when the path points to a file
/// * List::Directory when the path points to a directory
// TODO: add FileMetadata to File and "DirectoryMetadata" to Directory
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum List {
    NotFound,
    File,
    Directory(Vec<(PathComponentBuf, FsNodeMetadata)>),
}

/// FsNodeMetadata short for file system node.
/// The manifest tracks a list of files. However file systems are hierarchical structures
/// composed of directories and files at the end. For different operations it is useful to have
/// a representation for file or directory. A good example is listing a directory. This structure
/// helps us represent that notion.
#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum FsNodeMetadata {
    File(FileMetadata),
    Directory(Option<HgId>),
}

/// A directory entry in a manifest.
///
/// Consists of the full path to the directory. Directories may or may not be assigned
/// identifiers. When an identifier is available it is also returned.
// TODO: Move hgid to a new DirectoryMetadata struct.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Directory {
    pub path: RepoPathBuf,
    pub hgid: Option<HgId>,
}

impl Directory {
    pub fn new(path: RepoPathBuf, hgid: Option<HgId>) -> Self {
        Self { path, hgid }
    }
}

/// A file entry in a manifest.
///
/// Consists of the full path to the file along with the associated file metadata.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct File {
    pub path: RepoPathBuf,
    pub meta: FileMetadata,
}

impl File {
    pub fn new(path: RepoPathBuf, meta: FileMetadata) -> Self {
        Self { path, meta }
    }
}

/// The contents of the Manifest for a file.
/// * hgid: used to determine the revision of the file in the repository.
/// * file_type: determines the type of the file.
#[derive(Clone, Copy, Debug, Default, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[cfg_attr(any(test, feature = "for-tests"), derive(Arbitrary))]
pub struct FileMetadata {
    pub hgid: HgId,
    pub file_type: FileType,
}

/// The types of files (leaf nodes in a tree).
///
/// The type needs to round-trip tree serialization.
#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[derive(Default)]
pub enum FileType {
    /// Regular files.
    #[default]
    Regular,
    /// Executable files. Like Regular files but with the executable flag set.
    Executable,
    /// Symlinks. Their targets are not limited to repository paths. They can point anywhere.
    Symlink,
    /// Git submodule. It's up to the higher layer to decide what to do with them.
    GitSubmodule,
}

impl FileMetadata {
    pub fn new(hgid: HgId, file_type: FileType) -> Self {
        Self { hgid, file_type }
    }

    /// Creates `FileMetadata` with file_type set to `FileType::Regular`.
    pub fn regular(hgid: HgId) -> Self {
        Self::new(hgid, FileType::Regular)
    }

    /// Creates `FileMetadata` with file_type set to `FileType::Executable`.
    pub fn executable(hgid: HgId) -> Self {
        Self::new(hgid, FileType::Executable)
    }

    /// Creates `FileMetadata` with file_type set to `FileType::Symlink`.
    pub fn symlink(hgid: HgId) -> Self {
        Self::new(hgid, FileType::Symlink)
    }
}

/// Represents a file that is different between two tree manifests.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct DiffEntry {
    pub path: RepoPathBuf,
    pub diff_type: DiffType,
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct DirDiffEntry {
    /// Path of the directory.
    pub path: RepoPathBuf,
    /// Exist on the left side.
    pub left: bool,
    /// Exist on the right side.
    pub right: bool,
}

impl DiffEntry {
    pub fn new(path: RepoPathBuf, diff_type: DiffType) -> Self {
        DiffEntry { path, diff_type }
    }

    pub fn left(file: File) -> Self {
        Self::new(file.path, DiffType::LeftOnly(file.meta))
    }

    pub fn right(file: File) -> Self {
        Self::new(file.path, DiffType::RightOnly(file.meta))
    }

    pub fn changed(left: File, right: File) -> Self {
        debug_assert!(left.path == right.path);
        Self::new(left.path, DiffType::Changed(left.meta, right.meta))
    }
}

#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum DiffType {
    LeftOnly(FileMetadata),
    RightOnly(FileMetadata),
    Changed(FileMetadata, FileMetadata),
}

impl DiffType {
    /// Returns the metadata of the file in the left manifest when it exists.
    pub fn left(&self) -> Option<FileMetadata> {
        match self {
            DiffType::LeftOnly(left_metadata) => Some(*left_metadata),
            DiffType::RightOnly(_) => None,
            DiffType::Changed(left_metadata, _) => Some(*left_metadata),
        }
    }

    /// Returns the metadata of the file in the right manifest when it exists.
    pub fn right(&self) -> Option<FileMetadata> {
        match self {
            DiffType::LeftOnly(_) => None,
            DiffType::RightOnly(right_metadata) => Some(*right_metadata),
            DiffType::Changed(_, right_metadata) => Some(*right_metadata),
        }
    }
}

#[cfg(any(test, feature = "for-tests"))]
impl quickcheck::Arbitrary for FileType {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        g.choose(&[FileType::Regular, FileType::Executable, FileType::Symlink])
            .unwrap()
            .to_owned()
    }
}
