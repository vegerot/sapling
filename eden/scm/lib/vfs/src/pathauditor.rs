/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::borrow::Cow;
use std::fs::symlink_metadata;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use types::RepoPath;
use types::RepoPathBuf;

/// Audit repositories path to make sure that it is safe to write/remove through them.
///
/// This uses caching internally to avoid the heavy cost of querying the OS for each directory in
/// the path of a file.
///
/// The cache is concurrent and is shared between cloned instances of PathAuditor
pub struct PathAuditor {
    root: PathBuf,
    audited: DashMap<RepoPathBuf, ()>,
}

#[cfg(not(windows))]
const SEPARATORS: [char; 1] = ['/'];
#[cfg(windows)]
const SEPARATORS: [char; 2] = ['/', '\\'];

static WINDOWS_SHORTNAME_ALIASES: Lazy<Vec<&'static str>> =
    Lazy::new(|| identity::all().iter().map(|i| i.cli_name()).collect());

static INVALID_COMPONENTS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    let components: [&'static str; 2] = [".", ".."];
    components
        .into_iter()
        .chain(identity::all().iter().map(|i| i.dot_dir()))
        .collect()
});

// From encoding.py: These unicode characters are ignored by HFS+ (Apple Technote 1150,
// "Unicode Subtleties"), so we need to ignore them in some places for sanity.
const IGNORED_HFS_CHARS: [char; 16] = [
    '\u{200c}', '\u{200d}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}',
    '\u{202e}', '\u{206a}', '\u{206b}', '\u{206c}', '\u{206d}', '\u{206e}', '\u{206f}', '\u{feff}',
];

#[derive(thiserror::Error, Debug)]
pub enum AuditError {
    #[error("can't read/write file through ancestor symlink \"{0}\"")]
    ThroughSymlink(RepoPathBuf),
    #[error("invalid path component \"{0}\"")]
    InvalidComponent(String),
}

impl PathAuditor {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let audited = Default::default();
        let root = root.as_ref().to_owned();
        Self { root, audited }
    }

    /// Slow path, query the filesystem for unsupported path. Namely, writing through a symlink
    /// outside of the repo is not supported.
    /// XXX: more checks
    fn audit_fs(&self, path: &RepoPath) -> Result<(), AuditError> {
        let full_path = self.root.join(path.as_str());

        // XXX: Maybe filter by specific errors?
        if let Ok(metadata) = symlink_metadata(full_path) {
            if metadata.file_type().is_symlink() {
                return Err(AuditError::ThroughSymlink(path.to_owned()));
            }
        }

        Ok(())
    }

    /// Make sure that it is safe to write/remove `path` from the repo.
    pub fn audit(&self, path: &RepoPath) -> Result<PathBuf> {
        audit_invalid_components(path.as_str())
            .with_context(|| format!("invalid component in \"{}\"", path))?;

        let mut needs_recording_index = std::usize::MAX;
        for (i, parent) in path.reverse_parents().enumerate() {
            // First fast check w/ read lock
            if !self.audited.contains_key(parent) {
                // If fast check failed, do the stat syscall.
                self.audit_fs(parent)
                    .with_context(|| format!("path \"{}\" failed audit", path))?;

                // If it passes the audit, we can't record them as audited just yet, since a parent
                // may still fail the audit. Later we'll loop through and record successful audits.
                needs_recording_index = i;
            } else {
                // path.parents() yields the results in deepest-first order, so if we hit a path
                // that has been audited, we know all the future ones have been audited and we can
                // bail early.
                break;
            }
        }

        if needs_recording_index != std::usize::MAX {
            for (i, parent) in path.reverse_parents().enumerate() {
                self.audited.entry(parent.to_owned()).or_default();
                if needs_recording_index == i {
                    break;
                }
            }
        }

        let mut filepath = self.root.to_owned();
        filepath.push(path.as_str());
        Ok(filepath)
    }
}

/// Checks that shortnames (e.g. `SL~1`) are not a component on Windows and that files don't end in
/// a dot (e.g. `sigh....`)
fn valid_windows_component(component: &str) -> bool {
    if cfg!(not(windows)) {
        return true;
    }
    if let Some((l, r)) = component.split_once('~') {
        if r.chars().any(|c| c.is_numeric()) && WINDOWS_SHORTNAME_ALIASES.contains(&l) {
            return false;
        }
    }
    !component.ends_with('.')
}

/// Makes sure that the path does not contain any of the following components:
/// - ``, empty, implies that paths can't start with, end or contain consecutive `SEPARATOR`s
/// - `.`, dot/period, unix current directory
/// - `..`, double dot, unix parent directory
/// - `.sl` or `.hg`,
/// It also checks that no trailing dots are part of the component and checks that shortnames
/// on Windows are valid.
fn audit_invalid_components(path: &str) -> Result<(), AuditError> {
    let path: Cow<str> = if cfg!(not(windows)) {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(path.to_lowercase())
    };
    for s in path.split(SEPARATORS) {
        let s = if s.contains(IGNORED_HFS_CHARS) {
            Cow::Owned(s.replace(IGNORED_HFS_CHARS, ""))
        } else {
            Cow::Borrowed(s)
        };
        if s.is_empty() || INVALID_COMPONENTS.contains(&&*s) || !valid_windows_component(&s) {
            return Err(AuditError::InvalidComponent(s.into_owned()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_audit_valid() -> Result<()> {
        let root = TempDir::new()?;

        let auditor = PathAuditor::new(&root);

        let repo_path = RepoPath::from_str("a/b")?;
        assert_eq!(
            auditor.audit(repo_path)?,
            root.as_ref().join(repo_path.as_str())
        );

        Ok(())
    }

    #[test]
    fn test_audit_invalid_components() -> Result<()> {
        assert!(audit_invalid_components("a/../b").is_err());
        assert!(audit_invalid_components("a/./b").is_err());
        assert!(audit_invalid_components("a/.sl/b").is_err());
        assert!(audit_invalid_components("a/.hg/b").is_err());
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_audit_windows() -> Result<()> {
        let root = TempDir::new()?;

        let auditor = PathAuditor::new(&root);

        let repo_path = RepoPath::from_str("..\\foobar")?;
        assert!(auditor.audit(repo_path).is_err());
        let repo_path = RepoPath::from_str("x/y/SL~123/z")?;
        assert!(auditor.audit(repo_path).is_err());
        let repo_path = RepoPath::from_str("sl~12345/baz")?;
        assert!(auditor.audit(repo_path).is_err());
        let repo_path = RepoPath::from_str("a/.sL")?;
        assert!(auditor.audit(repo_path).is_err());
        let repo_path = RepoPath::from_str("Sure...")?;
        assert!(auditor.audit(repo_path).is_err());

        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn test_audit_invalid_symlink() -> Result<()> {
        let root = TempDir::new()?;
        let other = TempDir::new()?;

        let auditor = PathAuditor::new(&root);

        let link = root.as_ref().join("a");
        std::os::unix::fs::symlink(&other, &link)?;
        let canonical_other = other.as_ref().canonicalize()?;
        assert_eq!(fs::read_link(&link)?.canonicalize()?, canonical_other);

        let repo_path = RepoPath::from_str("a/b")?;
        assert!(auditor.audit(repo_path).is_err());

        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn test_audit_caching() -> Result<()> {
        let root = TempDir::new()?;
        let other = TempDir::new()?;

        let path = root.as_ref().join("a");
        fs::create_dir_all(&path)?;

        let auditor = PathAuditor::new(&root);

        // Populate the auditor cache.
        let repo_path = RepoPath::from_str("a/b")?;
        auditor.audit(repo_path)?;

        fs::remove_dir_all(&path)?;

        let link = root.as_ref().join("a");
        std::os::unix::fs::symlink(&other, &link)?;
        let canonical_other = other.as_ref().canonicalize()?;
        assert_eq!(fs::read_link(&link)?.canonicalize()?, canonical_other);

        // Even though "a" is now a symlink to outside the repo, the audit will succeed due to the
        // one performed just above.
        let repo_path = RepoPath::from_str("a/b")?;
        auditor.audit(repo_path)?;

        Ok(())
    }
}
