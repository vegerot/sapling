/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use bookmarks_types::BookmarkKey;
use bookmarks_types::BookmarkKind;
use context::CoreContext;
use mononoke_types::ChangesetId;

#[async_trait]
pub trait BookmarksSubscription: Send + Sync + 'static {
    /// Refresh this subscription with new updated bookmarks
    async fn refresh(&mut self, ctx: &CoreContext) -> Result<()>;

    /// Get current bookmarks.
    fn bookmarks(&self) -> &HashMap<BookmarkKey, (ChangesetId, BookmarkKind)>;
}
