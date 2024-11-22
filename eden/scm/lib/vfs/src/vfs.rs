/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::fs;
use std::fs::create_dir_all;
use std::fs::remove_dir;
use std::fs::remove_dir_all;
#[cfg(unix)]
use std::fs::set_permissions;
use std::fs::symlink_metadata;
use std::fs::File;
use std::fs::Metadata;
use std::fs::OpenOptions;
#[cfg(unix)]
use std::fs::Permissions;
use std::io;
use std::io::ErrorKind;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::bail;
use anyhow::ensure;
use anyhow::Context;
use anyhow::Result;
use fsinfo::fstype;
use fsinfo::FsType;
use minibytes::Bytes;
use types::RepoPath;
use util::path::remove_file;

use crate::pathauditor::PathAuditor;

#[derive(Clone)]
pub struct VFS {
    inner: Arc<Inner>,
}

struct Inner {
    root: PathBuf,
    auditor: PathAuditor,
    supports_symlinks: bool,
    supports_executables: bool,
    case_sensitive: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum UpdateFlag {
    Regular,
    Symlink,
    Executable,
}

impl VFS {
    pub fn new(root: PathBuf) -> Result<Self> {
        let auditor = PathAuditor::new(&root);
        let fs_type =
            fstype(&root).with_context(|| format!("can't construct a VFS for {:?}", root))?;
        let supports_symlinks = supports_symlinks(root.as_path())?;
        let supports_executables = supports_executables(&fs_type);
        let case_sensitive = case_sensitive(&root, &fs_type)?;

        Ok(Self {
            inner: Arc::new(Inner {
                root,
                auditor,
                supports_symlinks,
                supports_executables,
                case_sensitive,
            }),
        })
    }

    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    pub fn case_sensitive(&self) -> bool {
        self.inner.case_sensitive
    }

    pub fn join(&self, path: &RepoPath) -> PathBuf {
        self.inner.root.join(path.as_str())
    }

    pub fn metadata(&self, path: &RepoPath) -> Result<Metadata> {
        tracing::trace!(?path, "fetching metadata");

        self.join(path).symlink_metadata().map_err(|e| {
            // If `path` contains a directory that doesn't actually exist on disk, it surfaces as a
            // NotADirectory error. This error type is unstable and can't actually be matched on.
            // See https://github.com/rust-lang/rust/issues/86442
            // For now, let's convert it to a NotFound error, users of vfs probably want to
            // treat it as such.
            #[cfg(unix)]
            const NOTDIR: i32 = 20; // ENOTDIR
            #[cfg(windows)]
            const NOTDIR: i32 = 267; // ERROR_DIRECTORY

            match e.raw_os_error() {
                Some(errno) if errno == NOTDIR => io::Error::from(ErrorKind::NotFound).into(),
                _ => e.into(),
            }
        })
    }

    pub fn exists(&self, path: &RepoPath) -> Result<bool> {
        Ok(self.join(path).try_exists()?)
    }

    pub fn is_file(&self, path: &RepoPath) -> Result<bool> {
        let filepath = self.inner.auditor.audit(path)?;
        Ok(filepath.is_file())
    }

    /// The file `path` can't be written to, attempt to fixup the directories and files so the file can
    /// be created.
    ///
    /// This is a slow operation, and should not be called before attempting to create `path`.
    fn clear_conflicts(&self, repo_path: &RepoPath) -> Result<()> {
        let full_path = self.join(repo_path);

        // Walk down our ancestors, removing the first regular file or symlink
        // we find. We have the invariant that path_buf contains no symlinks
        // since we remove the top most symlink we come across.
        let mut path_buf = self.inner.root.clone();
        for part in repo_path.components() {
            path_buf.push(part.as_str());

            let metadata = match symlink_metadata(&path_buf) {
                Ok(metadata) => metadata,
                Err(err) if err.kind() == ErrorKind::NotFound => break,
                Err(err) => return Err(err).with_context(|| format!("can't lstat {:?}", path_buf)),
            };

            let file_type = metadata.file_type();
            if file_type.is_file() || file_type.is_symlink() {
                remove_file(&path_buf)
                    .with_context(|| format!("can't remove file {:?}", path_buf))?;
                break;
            }

            // If the full destination is a directory, clear it out.
            if file_type.is_dir() && path_buf == full_path {
                remove_dir_all(&path_buf)
                    .with_context(|| format!("can't remove directory {:?}", path_buf))?;
                break;
            }
        }

        let dir = full_path.parent().unwrap();
        create_dir_all(dir).with_context(|| format!("can't create directory {:?}", dir))?;

        Ok(())
    }

    fn write_mode(
        &self,
        filepath: &Path,
        content: &[u8],
        #[allow(unused_variables)] exec: bool,
    ) -> Result<usize> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);

            // This sets file mode if file is created during "open".
            options.mode(Self::update_mode(util::file::apply_umask(0o666), exec));
        }

        let mut f = options.open(filepath)?;

        #[cfg(unix)]
        {
            let metadata = f.metadata()?;
            let mut permissions = metadata.permissions();
            let mode = Self::update_mode(permissions.mode(), exec);
            if mode != permissions.mode() {
                permissions.set_mode(mode);
                f.set_permissions(permissions)
                    .with_context(|| format!("Failed to set permissions on {:?}", filepath))?;
            }
        }

        f.write_all(content)
            .with_context(|| format!("can't write to {:?}", filepath))?;
        Ok(content.len())
    }

    #[cfg(unix)]
    fn update_mode(mode: u32, exec: bool) -> u32 {
        if exec {
            mode | util::file::apply_umask((mode & 0o444) >> 2)
        } else {
            mode & 0o666
        }
    }

    #[cfg(windows)]
    fn set_exec(&self, _: &Path, _: bool) -> Result<()> {
        return Ok(());
    }

    #[cfg(unix)]
    fn set_exec(&self, filepath: &Path, flag: bool) -> Result<()> {
        let mode = if flag { 0o755 } else { 0o644 };
        let perms = Permissions::from_mode(mode);
        set_permissions(filepath, perms)
            .with_context(|| format!("can't update exec flag({}) on {:?}", flag, filepath))?;
        Ok(())
    }

    /// On some OS/filesystems, symlinks aren't supported, we simply create a file where it's content
    /// is the symlink destination for these.
    fn plain_symlink_file(link_name: &Path, link_dest: &Path) -> Result<()> {
        let link_dest = match link_dest.to_str() {
            None => bail!("not a valid UTF-8 path: {:?}", link_dest),
            Some(s) => s,
        };

        Ok(File::create(link_name)?.write_all(link_dest.as_bytes())?)
    }

    /// Add a symlink `link_name` pointing to `link_dest`. On platforms that do not support symlinks,
    /// `link_name` will be a file containing the path to `link_dest`.
    fn symlink(&self, link_name: &Path, link_dest: &Path) -> Result<()> {
        let result = if self.inner.supports_symlinks && (cfg!(unix) || cfg!(windows)) {
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_file(
                    util::path::replace_slash_with_backslash(link_dest).as_path(),
                    link_name,
                )
                .map_err(Into::into)
            }
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(link_dest, link_name).map_err(Into::into)
            }
        } else {
            Self::plain_symlink_file(link_name, link_dest)
        };

        result.with_context(|| format!("can't symlink {:?} to {:?}", link_name, link_dest))
    }

    /// Write a symlink file at `filepath`. The destination is represented by `content`.
    fn write_symlink(&self, filepath: &Path, content: &[u8]) -> Result<usize> {
        let link_dest = Path::new(std::str::from_utf8(content)?);

        self.symlink(filepath, link_dest)?;
        Ok(filepath.as_os_str().len())
    }

    /// Overwrite the content of the file at `path` with `data`. The number of bytes written on
    /// disk will be returned.
    fn write_inner(&self, path: &RepoPath, data: &[u8], flags: UpdateFlag) -> Result<usize> {
        let filepath = self.inner.auditor.audit(path)?;

        match flags {
            UpdateFlag::Regular => self.write_mode(&filepath, data, false),
            UpdateFlag::Executable => self.write_mode(&filepath, data, true),
            UpdateFlag::Symlink => self.write_symlink(&filepath, data),
        }
    }

    /// Overwrite content of the file, try to clear conflicts if attempt fails
    ///
    /// Return an error if fails to overwrite after clearing conflicts, or if clear conflicts fail
    pub fn write(&self, path: &RepoPath, data: &[u8], flag: UpdateFlag) -> Result<usize> {
        // Fast path: let's try to open the file directly, we'll handle the failure only if this fails.
        match self.write_inner(path, data, flag) {
            Ok(size) => Ok(size),
            Err(e) => {
                // Ideally, we shouldn't need to retry for some failures, but this is the slow path, any
                // failures not due to a conflicting file would show up here again, so let's not worry
                // about it.
                self.clear_conflicts(path).with_context(|| {
                    format!("can't clear conflicts after handling error \"{:#}\"", e)
                })?;

                self.write_inner(path, data, flag)
            }
        }
    }

    pub fn set_executable(&self, path: &RepoPath, flag: bool) -> Result<()> {
        let filepath = self.inner.auditor.audit(path)?;

        self.set_exec(&filepath, flag)
    }

    /// Remove the file at `path`.
    ///
    /// If file does not exist, returns without an error
    ///
    /// The parent directories of this file will be removed recursively if they are empty.
    pub fn remove(&self, path: &RepoPath) -> Result<()> {
        let mut filepath = self.inner.auditor.audit(path)?;
        self.remove_keep_path(&filepath)?;

        // Mercurial doesn't track empty directories, remove them
        // recursively.
        loop {
            if !filepath.pop() || filepath == self.inner.root {
                break;
            }

            if remove_dir(&filepath).is_err() {
                break;
            }
        }
        Ok(())
    }

    /// Rewrite over a symlink that already exists.
    ///
    /// Care is taken to not accidentally write _through_ the symlink.
    pub fn rewrite_symlink(&self, path: &RepoPath, data: &[u8], flag: UpdateFlag) -> Result<usize> {
        if !cfg!(unix) {
            // unix supports O_NOFOLLOW when opening. For Windows, just remove the file first.
            let filepath = self.inner.auditor.audit(path)?;
            self.remove_keep_path(&filepath)?;
        }
        self.write(path, data, flag)
    }

    // Reads file content
    pub fn read(&self, path: &RepoPath) -> Result<Bytes> {
        Ok(self.read_with_metadata(path)?.0)
    }

    // Reads file content and metadata
    pub fn read_with_metadata(&self, path: &RepoPath) -> Result<(Bytes, Metadata)> {
        let filepath = self.inner.auditor.audit(path)?;
        let metadata = self.metadata(path)?;
        let content = if metadata.is_symlink() {
            match std::fs::read_link(&filepath)?.to_str() {
                Some(p) => {
                    let p = if cfg!(windows) {
                        p.replace('\\', "/")
                    } else {
                        p.to_owned()
                    };
                    p.as_bytes().to_vec()
                }
                None => bail!("invalid path during vfs::read {:?}", filepath),
            }
        } else {
            std::fs::read(filepath)?
        };
        Ok((content.into(), metadata))
    }

    /// Removes file, but unlike Self::remove, does not delete empty directories.
    fn remove_keep_path(&self, filepath: &PathBuf) -> Result<()> {
        if let Ok(metadata) = symlink_metadata(filepath) {
            let file_type = metadata.file_type();
            if file_type.is_file() || file_type.is_symlink() {
                let result = remove_file(filepath)
                    .with_context(|| format!("can't remove file {:?}", filepath));
                if let Err(e) = result {
                    if let Some(io_error) = e.downcast_ref::<io::Error>() {
                        ensure!(io_error.kind() == ErrorKind::NotFound, e);
                    } else {
                        return Err(e);
                    };
                }
            }
        }

        Ok(())
    }

    pub fn supports_symlinks(&self) -> bool {
        self.inner.supports_symlinks
    }

    pub fn supports_executables(&self) -> bool {
        self.inner.supports_executables
    }
}

#[cfg(unix)]
#[cfg(test)]
mod unix_tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_symlink_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let vfs = VFS::new(tmp.path().to_path_buf()).unwrap();
        let path = RepoPath::from_str("a").unwrap();
        vfs.write(path, b"abc", UpdateFlag::Symlink).unwrap();
        vfs.write(path, &[1, 2, 3], UpdateFlag::Regular).unwrap();
        let metadata = fs::symlink_metadata(vfs.join(path)).unwrap();
        assert!(metadata.file_type().is_file())
    }

    #[test]
    fn test_ancestor_symlink_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let vfs = VFS::new(tmp.path().to_path_buf()).unwrap();

        let dir = RepoPath::from_str("a").unwrap();
        let file = RepoPath::from_str("a/b").unwrap();

        vfs.write(dir, b"abc", UpdateFlag::Symlink).unwrap();
        vfs.write(file, &[1, 2, 3], UpdateFlag::Regular).unwrap();
        let metadata = fs::symlink_metadata(vfs.join(file)).unwrap();
        assert!(metadata.file_type().is_file())
    }

    #[test]
    fn test_symlink_read() {
        let tmp = tempfile::tempdir().unwrap();
        let vfs = VFS::new(tmp.path().to_path_buf()).unwrap();
        let path = RepoPath::from_str("a").unwrap();
        vfs.write(path, b"abc", UpdateFlag::Symlink).unwrap();
        let buf = vfs.read(path).unwrap();
        assert_eq!(buf, b"abc")
    }

    #[test]
    fn test_exec_overwrite() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let vfs = VFS::new(tmp.path().to_path_buf()).unwrap();
        let path = RepoPath::from_str("a").unwrap();
        vfs.write(path, "abc".as_bytes(), UpdateFlag::Executable)
            .unwrap();
        vfs.write(path, &[1, 2, 3], UpdateFlag::Regular).unwrap();
        let mut buf = tmp.path().to_path_buf();
        buf.push("a");
        let metadata = fs::symlink_metadata(buf).unwrap();
        assert_eq!(0, metadata.permissions().mode() & 0o111)
    }

    #[test]
    fn test_update_mode() {
        assert_eq!(0o644, VFS::update_mode(0o644, false));
        assert_eq!(0o755, VFS::update_mode(0o755, true));

        assert_eq!(0o755, VFS::update_mode(0o644, true));
        assert_eq!(0o644, VFS::update_mode(0o755, false));
    }
}

fn supports_symlinks(path: &Path) -> Result<bool> {
    if std::env::var("SL_DEBUG_DISABLE_SYMLINKS").is_ok() {
        return Ok(false);
    }

    if !cfg!(windows) {
        return Ok(true);
    }

    return Ok(if let Some((root, ident)) = identity::sniff_root(path)? {
        // This assumes that at this point symlinks are already properly checked for as part of
        // the repo initialization
        fs::read_to_string(root.join(ident.dot_dir()).join("requires"))?
            .split_whitespace()
            .any(|s| s == "windowssymlinks")
    } else {
        // There are some cases (e.g., tests) where we do not have an actual repo and thus
        // no dot_dir directory. Gracefully fail in this case.
        false
    });
}

/// Since Windows determines if a file is executable based on its extension, it doesn't support
/// marking files as executable.
fn supports_executables(fs_type: &FsType) -> bool {
    match *fs_type {
        FsType::NTFS => false,
        FsType::EDENFS => !cfg!(windows),
        _ => true,
    }
}

/// determines whether FS located at root is case sensitive
fn case_sensitive(root: &Path, fs_type: &FsType) -> Result<bool> {
    // Logic in this function is consistent with util.fscasesensitive in Python
    // For some FS we know they are case (in)sensitive, so we just return based on fs type
    // For rest of the FS we see if lstat on the upper/lower case variant differs
    match *fs_type {
        FsType::EDENFS => return Ok(cfg!(target_os = "linux")),
        FsType::BTRFS => return Ok(true),
        FsType::EXT4 => return Ok(true),
        FsType::XFS => return Ok(true),
        FsType::UFS => return Ok(true),
        FsType::TMPFS => return Ok(true),
        _ => {}
    }
    detect_case_sensitive(root)
}

fn detect_case_sensitive(root: &Path) -> Result<bool> {
    let original_lstat = root.symlink_metadata()?;
    let root_str = root.to_str().expect("Can't convert root path to string");
    let mut case_different = root_str.to_lowercase();
    if case_different == root_str {
        case_different = root_str.to_uppercase();
    }
    let case_different = PathBuf::from(case_different);
    let case_different_lstat = case_different.symlink_metadata();
    if let Ok(case_different_lstat) = case_different_lstat {
        Ok(!metadata_eq(&case_different_lstat, &original_lstat)?)
    } else {
        Ok(true)
    }
}

/// Roughly compares metadata, only for internal vfs usage
/// Do not make this fn public
fn metadata_eq(m1: &Metadata, m2: &Metadata) -> Result<bool> {
    Ok(m1.modified()? == m2.modified()?
        && m1.accessed()? == m2.accessed()?
        && m1.created()? == m2.created()?
        && m1.file_type() == m2.file_type())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_case_sensitive() {
        let tmp = tempfile::tempdir().unwrap();
        let case_sensitive = detect_case_sensitive(tmp.path()).unwrap();
        #[cfg(target_os = "linux")]
        assert!(case_sensitive);
        #[cfg(windows)]
        assert!(!case_sensitive);
        #[cfg(target_os = "macos")]
        assert!(!case_sensitive);
    }
}
