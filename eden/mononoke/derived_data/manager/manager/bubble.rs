/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::sync::Arc;

use context::CoreContext;
use ephemeral_blobstore::Bubble;
use ephemeral_blobstore::EphemeralChangesets;
use mononoke_types::ChangesetId;

use super::DerivationAssigner;
use super::DerivationAssignment;
use super::DerivedDataManager;
use super::DerivedDataManagerInner;
use super::SecondaryManagerData;

struct BubbleAssigner {
    changesets: Arc<EphemeralChangesets>,
}

#[async_trait::async_trait]
impl DerivationAssigner for BubbleAssigner {
    async fn assign(
        &self,
        _ctx: &CoreContext,
        cs: Vec<ChangesetId>,
    ) -> anyhow::Result<DerivationAssignment> {
        let in_bubble = self.changesets.fetch_gens(&cs).await?;
        let (in_bubble, not_in_bubble) = cs
            .into_iter()
            .partition(|cs_id| in_bubble.contains_key(cs_id));
        Ok(DerivationAssignment {
            primary: not_in_bubble,
            secondary: in_bubble,
        })
    }
}

impl DerivedDataManager {
    pub fn for_bubble(self, bubble: Bubble) -> Self {
        let changesets = Arc::new(bubble.changesets(
            self.repo_id(),
            self.repo_blobstore().clone(),
            self.changesets_arc(),
        ));
        let commit_graph = Arc::new(bubble.commit_graph(
            self.repo_id(),
            self.repo_blobstore().clone(),
            self.commit_graph(),
        ));
        let wrapped_blobstore = bubble.wrap_repo_blobstore(self.inner.repo_blobstore.clone());
        let mut derivation_context = self.inner.derivation_context.clone();
        derivation_context.bonsai_hg_mapping = None;
        derivation_context.filenodes = None;
        derivation_context.blobstore = wrapped_blobstore.boxed();

        // TODO (Pierre): Should we also clear bonsai_git_mapping? By symmetry, it appears so
        Self {
            inner: Arc::new(DerivedDataManagerInner {
                secondary: Some(SecondaryManagerData {
                    manager: Self {
                        inner: Arc::new(DerivedDataManagerInner {
                            bubble_id: Some(bubble.bubble_id()),
                            changesets: changesets.clone(),
                            commit_graph: commit_graph.clone(),
                            repo_blobstore: wrapped_blobstore,
                            derivation_context,
                            ..self.inner.as_ref().clone()
                        }),
                    },
                    assigner: Arc::new(BubbleAssigner {
                        changesets: changesets.clone(),
                    }),
                }),
                bubble_id: Some(bubble.bubble_id()),
                changesets,
                commit_graph,
                ..self.inner.as_ref().clone()
            }),
        }
    }
}
