/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use commit_graph_types::edges::ChangesetEdges;
use commit_graph_types::storage::CommitGraphStorage;
use commit_graph_types::storage::FetchedChangesetEdges;
use commit_graph_types::storage::Prefetch;
use context::CoreContext;
use in_memory_commit_graph_storage::InMemoryCommitGraphStorage;
use mononoke_types::ChangesetId;
use mononoke_types::ChangesetIdPrefix;
use mononoke_types::ChangesetIdsResolvedFromPrefix;
use mononoke_types::RepositoryId;
use vec1::Vec1;

#[cfg(test)]
mod tests;

/// A storage backend for commit graph that never writes to its persistent storage:
/// It reads from its backing store and writes to a in-memory version of its backing store
pub struct MemWritesCommitGraphStorage {
    in_memory_storage: InMemoryCommitGraphStorage,
    persistent_storage: Arc<dyn CommitGraphStorage>,
}

impl MemWritesCommitGraphStorage {
    pub fn new(persistent_storage: Arc<dyn CommitGraphStorage>) -> Self {
        Self {
            in_memory_storage: InMemoryCommitGraphStorage::new(persistent_storage.repo_id()),
            persistent_storage,
        }
    }
}

#[async_trait]
impl CommitGraphStorage for MemWritesCommitGraphStorage {
    fn repo_id(&self) -> RepositoryId {
        self.persistent_storage.repo_id()
    }

    async fn add(&self, ctx: &CoreContext, edges: ChangesetEdges) -> Result<bool> {
        self.in_memory_storage.add(ctx, edges).await
    }

    async fn add_many(&self, ctx: &CoreContext, many_edges: Vec1<ChangesetEdges>) -> Result<usize> {
        self.in_memory_storage.add_many(ctx, many_edges).await
    }

    async fn fetch_edges(&self, ctx: &CoreContext, cs_id: ChangesetId) -> Result<ChangesetEdges> {
        match self.in_memory_storage.maybe_fetch_edges(ctx, cs_id).await? {
            Some(edges) => Ok(edges),
            None => self.persistent_storage.fetch_edges(ctx, cs_id).await,
        }
    }

    async fn maybe_fetch_edges(
        &self,
        ctx: &CoreContext,
        cs_id: ChangesetId,
    ) -> Result<Option<ChangesetEdges>> {
        match self.in_memory_storage.maybe_fetch_edges(ctx, cs_id).await? {
            Some(edges) => Ok(Some(edges)),
            None => self.persistent_storage.maybe_fetch_edges(ctx, cs_id).await,
        }
    }

    async fn fetch_many_edges(
        &self,
        ctx: &CoreContext,
        cs_ids: &[ChangesetId],
        prefetch: Prefetch,
    ) -> Result<HashMap<ChangesetId, FetchedChangesetEdges>> {
        let mut fetched_edges = self
            .in_memory_storage
            .maybe_fetch_many_edges(ctx, cs_ids, prefetch)
            .await?;

        let unfetched_ids = cs_ids
            .iter()
            .filter(|cs_id| !fetched_edges.contains_key(cs_id))
            .copied()
            .collect::<Vec<_>>();

        if !unfetched_ids.is_empty() {
            fetched_edges.extend(
                self.persistent_storage
                    .fetch_many_edges(ctx, unfetched_ids.as_slice(), prefetch)
                    .await?,
            )
        }

        Ok(fetched_edges)
    }

    async fn maybe_fetch_many_edges(
        &self,
        ctx: &CoreContext,
        cs_ids: &[ChangesetId],
        prefetch: Prefetch,
    ) -> Result<HashMap<ChangesetId, FetchedChangesetEdges>> {
        let mut fetched_edges = self
            .in_memory_storage
            .maybe_fetch_many_edges(ctx, cs_ids, prefetch)
            .await?;

        let unfetched_ids = cs_ids
            .iter()
            .filter(|cs_id| !fetched_edges.contains_key(cs_id))
            .copied()
            .collect::<Vec<_>>();

        if !unfetched_ids.is_empty() {
            fetched_edges.extend(
                self.persistent_storage
                    .maybe_fetch_many_edges(ctx, unfetched_ids.as_slice(), prefetch)
                    .await?,
            )
        }

        Ok(fetched_edges)
    }

    async fn find_by_prefix(
        &self,
        ctx: &CoreContext,
        cs_prefix: ChangesetIdPrefix,
        limit: usize,
    ) -> Result<ChangesetIdsResolvedFromPrefix> {
        match futures::try_join!(
            self.in_memory_storage.find_by_prefix(ctx, cs_prefix, limit),
            self.persistent_storage
                .find_by_prefix(ctx, cs_prefix, limit)
        )? {
            (in_memory_matches @ ChangesetIdsResolvedFromPrefix::TooMany(_), _) => {
                Ok(in_memory_matches)
            }
            (_, persistent_matches @ ChangesetIdsResolvedFromPrefix::TooMany(_)) => {
                Ok(persistent_matches)
            }
            (in_memory_matches, persistent_matches) => {
                Ok(ChangesetIdsResolvedFromPrefix::from_vec_and_limit(
                    in_memory_matches
                        .to_vec()
                        .into_iter()
                        .chain(persistent_matches.to_vec())
                        .collect(),
                    limit,
                ))
            }
        }
    }

    async fn fetch_children(
        &self,
        ctx: &CoreContext,
        cs_id: ChangesetId,
    ) -> Result<Vec<ChangesetId>> {
        Ok(self
            .in_memory_storage
            .fetch_children(ctx, cs_id)
            .await?
            .into_iter()
            .chain(self.persistent_storage.fetch_children(ctx, cs_id).await?)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect())
    }
}
