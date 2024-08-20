/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::fmt;

use anyhow::format_err;
use bytes::Bytes;
use bytes::BytesMut;
use cloned::cloned;
use context::CoreContext;
use filestore::get_metadata;
use filestore::FetchKey;
use futures::stream::TryStreamExt;
use futures::try_join;
use futures_lazy_shared::LazyShared;
/// A file's ID is its content id.
pub use mononoke_types::ContentId as FileId;
/// Metadata about a file.
pub use mononoke_types::ContentMetadataV2 as FileMetadata;
/// The type of a file.
pub use mononoke_types::FileType;
use repo_blobstore::RepoBlobstoreRef;

use crate::errors::MononokeError;
use crate::repo::RepoContext;
use crate::MononokeRepo;

#[derive(Clone)]
pub struct FileContext<R> {
    repo_ctx: RepoContext<R>,
    fetch_key: FetchKey,
    metadata: LazyShared<Result<FileMetadata, MononokeError>>,
}

impl<R: MononokeRepo> fmt::Debug for FileContext<R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "FileContext(repo={:?} fetch_key={:?})",
            self.repo_ctx().name(),
            self.fetch_key
        )
    }
}

/// Context for accessing a file in a repository.
///
/// Files are content-addressed, so if the same file occurs in multiple
/// places in the repository, this context represents all of them. As such,
/// it's not possible to go back to the commit or path from a `FileContext`.
///
/// See `ChangesetPathContentContext` if you need to refer to a specific file in a
/// specific commit.
impl<R: MononokeRepo> FileContext<R> {
    /// Create a new FileContext.  The file must exist in the repository,
    /// and the user must be known to have permission to access the path
    /// the file exists at.
    ///
    /// To construct a `FileContext` for a file that might not exist, use
    /// `new_check_exists`.
    pub(crate) fn new_authorized(repo_ctx: RepoContext<R>, fetch_key: FetchKey) -> Self {
        Self {
            repo_ctx,
            fetch_key,
            metadata: LazyShared::new_empty(),
        }
    }

    /// Create a new  FileContext using an ID that might not exist. Returns
    /// `None` if the file doesn't exist.
    pub(crate) async fn new_check_exists(
        repo_ctx: RepoContext<R>,
        fetch_key: FetchKey,
    ) -> Result<Option<Self>, MononokeError> {
        // Access to an arbitrary file requires full access to the repo,
        // as we do not know which path it corresponds to.
        repo_ctx
            .authorization_context()
            .require_full_repo_read(repo_ctx.ctx(), repo_ctx.repo())
            .await?;
        // Try to get the file metadata immediately to see if it exists.
        let file = get_metadata(repo_ctx.repo().repo_blobstore(), repo_ctx.ctx(), &fetch_key)
            .await?
            .map(|metadata| Self {
                repo_ctx,
                fetch_key,
                metadata: LazyShared::new_ready(Ok(metadata)),
            });
        Ok(file)
    }

    /// The context for this query.
    pub(crate) fn ctx(&self) -> &CoreContext {
        self.repo_ctx.ctx()
    }

    /// The `RepoContext` for this query.
    pub(crate) fn repo_ctx(&self) -> &RepoContext<R> {
        &self.repo_ctx
    }

    /// Return the ID of the file.
    pub async fn id(&self) -> Result<FileId, MononokeError> {
        let meta = self.metadata().await?;
        Ok(meta.content_id)
    }

    /// Return the metadata for a file.
    pub async fn metadata(&self) -> Result<FileMetadata, MononokeError> {
        self.metadata
            .get_or_init(|| {
                cloned!(self.repo_ctx, self.fetch_key);
                async move {
                    get_metadata(repo_ctx.repo().repo_blobstore(), repo_ctx.ctx(), &fetch_key)
                        .await
                        .map_err(MononokeError::from)
                        .and_then(|metadata| {
                            metadata.ok_or_else(|| content_not_found_error(&fetch_key))
                        })
                }
            })
            .await
    }

    /// Return the content for the file.
    ///
    /// This method buffers the full file content in memory, which may
    /// be expensive in the case of large files.
    pub async fn content_concat(&self) -> Result<Bytes, MononokeError> {
        let bytes = filestore::fetch_concat_opt(
            self.repo_ctx().repo().repo_blobstore(),
            self.ctx(),
            &self.fetch_key,
        )
        .await;

        match bytes {
            Ok(Some(bytes)) => Ok(bytes),
            Ok(None) => Err(content_not_found_error(&self.fetch_key)),
            Err(e) => Err(MononokeError::from(e)),
        }
    }

    /// Return the content for a range within the file.
    ///
    /// If the range goes past the end of the file, then content up to
    /// the end of the file is returned.  If the range starts past the
    /// end of the file, then an empty buffer is returned.
    pub async fn content_range_concat(
        &self,
        start: u64,
        size: u64,
    ) -> Result<Bytes, MononokeError> {
        let ret = filestore::fetch_range_with_size(
            self.repo_ctx().repo().repo_blobstore(),
            self.ctx(),
            &self.fetch_key,
            filestore::Range::sized(start, size),
        )
        .await;

        match ret {
            Ok(Some((stream, size))) => {
                let size = size.try_into().map_err(|_| {
                    MononokeError::from(format_err!("content too large: {:?}", self.fetch_key))
                })?;

                let bytes = stream
                    .map_err(MononokeError::from)
                    .try_fold(
                        BytesMut::with_capacity(size),
                        |mut buff, chunk| async move {
                            buff.extend_from_slice(&chunk);
                            Ok(buff)
                        },
                    )
                    .await?
                    .freeze();

                Ok(bytes)
            }
            Ok(None) => Err(content_not_found_error(&self.fetch_key)),
            Err(e) => Err(MononokeError::from(e)),
        }
    }
}

/// A diff between two files in headerless unified diff format
pub struct HeaderlessUnifiedDiff {
    /// Raw diff as bytes.
    pub raw_diff: Vec<u8>,
    /// One of the diffed files is binary, raw diff contains just a placeholder.
    pub is_binary: bool,
}

pub async fn headerless_unified_diff<R: MononokeRepo>(
    old_file: &FileContext<R>,
    new_file: &FileContext<R>,
    context_lines: usize,
) -> Result<HeaderlessUnifiedDiff, MononokeError> {
    let (old_diff_file, new_diff_file) =
        try_join!(old_file.content_concat(), new_file.content_concat(),)?;
    let is_binary = old_diff_file.contains(&0) || new_diff_file.contains(&0);
    let raw_diff = if is_binary {
        b"Binary files differ".to_vec()
    } else {
        let opts = xdiff::HeaderlessDiffOpts {
            context: context_lines,
        };
        xdiff::diff_unified_headerless(&old_diff_file, &new_diff_file, opts)
    };
    Ok(HeaderlessUnifiedDiff {
        raw_diff,
        is_binary,
    })
}

/// File contexts should only exist for files that are known to be in the
/// blobstore. If attempting to access the content results in an error, this
/// error is returned. This is an internal error, as it means either the data
/// has been lost from the blobstore, or the file context was erroneously
/// constructed.
fn content_not_found_error(fetch_key: &FetchKey) -> MononokeError {
    MononokeError::from(format_err!("content not found: {:?}", fetch_key))
}
