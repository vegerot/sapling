/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;

use anyhow::Result;
use bookmarks::BookmarkTransaction;
use bookmarks::BookmarkTransactionHook;
use bookmarks::BookmarkUpdateLogId;
use bookmarks::BookmarkUpdateReason;
use bookmarks_types::BookmarkKey;
use bookmarks_types::BookmarkKind;
use bytes::Bytes;
use context::CoreContext;
use hooks::CrossRepoPushSource;
use hooks::HookManager;
use mononoke_types::BonsaiChangeset;
use mononoke_types::ChangesetId;
use repo_authorization::AuthorizationContext;
use repo_authorization::RepoWriteOperation;
use repo_update_logger::find_draft_ancestors;
use repo_update_logger::BookmarkInfo;
use repo_update_logger::BookmarkOperation;

use crate::affected_changesets::AdditionalChangesets;
use crate::affected_changesets::AffectedChangesets;
use crate::repo_lock::check_repo_lock;
use crate::restrictions::check_bookmark_sync_config;
use crate::restrictions::BookmarkKindRestrictions;
use crate::BookmarkInfoData;
use crate::BookmarkInfoTransaction;
use crate::BookmarkMovementError;
use crate::Repo;
use crate::ALLOW_NON_FFWD_PUSHVAR;

/// The old and new changeset during a bookmark update.
///
/// This is a struct to make sure it is clear which is the old target and which is the new.
pub struct BookmarkUpdateTargets {
    pub old: ChangesetId,
    pub new: ChangesetId,
}

/// Which kinds of bookmark updates are allowed for a request.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BookmarkUpdatePolicy {
    /// Only allow fast-forward moves (updates where the new target is a descendant
    /// of the old target).
    FastForwardOnly,

    /// Allow any update that is permitted for the bookmark by repo config.
    AnyPermittedByConfig,
}

impl BookmarkUpdatePolicy {
    async fn check_update_permitted(
        &self,
        ctx: &CoreContext,
        repo: &impl Repo,
        bookmark: &BookmarkKey,
        targets: &BookmarkUpdateTargets,
        pushvars: &Option<&HashMap<String, Bytes>>,
    ) -> Result<(), BookmarkMovementError> {
        let fast_forward_only = match self {
            Self::FastForwardOnly => true,
            Self::AnyPermittedByConfig => repo.repo_bookmark_attrs().is_fast_forward_only(bookmark),
        };
        let bypass = pushvars.map_or(false, |pushvar| {
            pushvar.contains_key(ALLOW_NON_FFWD_PUSHVAR)
        });
        if fast_forward_only && !bypass && targets.old != targets.new {
            // Check that this move is a fast-forward move.
            if !repo
                .commit_graph()
                .is_ancestor(ctx, targets.old, targets.new)
                .await?
            {
                return Err(BookmarkMovementError::NonFastForwardMove {
                    bookmark: bookmark.clone(),
                    from: targets.old,
                    to: targets.new,
                });
            }
        }
        Ok(())
    }
}

#[must_use = "UpdateBookmarkOp must be run to have an effect"]
pub struct UpdateBookmarkOp<'op> {
    bookmark: BookmarkKey,
    targets: BookmarkUpdateTargets,
    update_policy: BookmarkUpdatePolicy,
    reason: BookmarkUpdateReason,
    kind_restrictions: BookmarkKindRestrictions,
    cross_repo_push_source: CrossRepoPushSource,
    affected_changesets: AffectedChangesets,
    pushvars: Option<&'op HashMap<String, Bytes>>,
    log_new_public_commits_to_scribe: bool,
    only_log_acl_checks: bool,
}

impl<'op> UpdateBookmarkOp<'op> {
    pub fn new(
        bookmark: BookmarkKey,
        targets: BookmarkUpdateTargets,
        update_policy: BookmarkUpdatePolicy,
        reason: BookmarkUpdateReason,
    ) -> UpdateBookmarkOp<'op> {
        UpdateBookmarkOp {
            bookmark,
            targets,
            update_policy,
            reason,
            kind_restrictions: BookmarkKindRestrictions::AnyKind,
            cross_repo_push_source: CrossRepoPushSource::NativeToThisRepo,
            affected_changesets: AffectedChangesets::new(),
            pushvars: None,
            log_new_public_commits_to_scribe: false,
            only_log_acl_checks: false,
        }
    }

    pub fn only_if_scratch(mut self) -> Self {
        self.kind_restrictions = BookmarkKindRestrictions::OnlyScratch;
        self
    }

    pub fn only_if_public(mut self) -> Self {
        self.kind_restrictions = BookmarkKindRestrictions::OnlyPublishing;
        self
    }

    pub fn with_pushvars(mut self, pushvars: Option<&'op HashMap<String, Bytes>>) -> Self {
        self.pushvars = pushvars;
        self
    }

    pub fn with_checks_bypassed(mut self) -> Self {
        self.affected_changesets
            .bypass_checks_on_additional_changesets();
        self
    }

    /// Include bonsai changesets for changesets that have just been added to
    /// the repository.
    pub fn with_new_changesets(
        mut self,
        changesets: HashMap<ChangesetId, BonsaiChangeset>,
    ) -> Self {
        self.affected_changesets.add_new_changesets(changesets);
        self
    }

    pub fn with_push_source(mut self, cross_repo_push_source: CrossRepoPushSource) -> Self {
        self.cross_repo_push_source = cross_repo_push_source;
        self
    }

    pub fn log_new_public_commits_to_scribe(mut self) -> Self {
        self.log_new_public_commits_to_scribe = true;
        self
    }

    pub fn only_log_acl_checks(mut self, only_log: bool) -> Self {
        self.only_log_acl_checks = only_log;
        self
    }

    pub async fn run_with_transaction(
        mut self,
        ctx: &'op CoreContext,
        authz: &'op AuthorizationContext,
        repo: &'op impl Repo,
        hook_manager: &'op HookManager,
        txn: Option<Box<dyn BookmarkTransaction>>,
        mut txn_hooks: Vec<BookmarkTransactionHook>,
    ) -> Result<BookmarkInfoTransaction, BookmarkMovementError> {
        let kind = self.kind_restrictions.check_kind(repo, &self.bookmark)?;

        if self.only_log_acl_checks {
            if authz
                .check_repo_write(ctx, repo, RepoWriteOperation::UpdateBookmark(kind))
                .await
                .is_denied()
            {
                ctx.scuba()
                    .clone()
                    .log_with_msg("Repo write ACL check would fail for bookmark update", None);
            }
        } else {
            authz
                .require_repo_write(ctx, repo, RepoWriteOperation::UpdateBookmark(kind))
                .await?;
        }
        authz
            .require_bookmark_modify(ctx, repo, &self.bookmark)
            .await?;

        check_bookmark_sync_config(ctx, repo, &self.bookmark, kind).await?;

        self.update_policy
            .check_update_permitted(ctx, repo, &self.bookmark, &self.targets, &self.pushvars)
            .await?;

        self.affected_changesets
            .check_restrictions(
                ctx,
                authz,
                repo,
                hook_manager,
                &self.bookmark,
                self.pushvars,
                self.reason,
                kind,
                AdditionalChangesets::Range {
                    head: self.targets.new,
                    base: self.targets.old,
                },
                self.cross_repo_push_source,
            )
            .await?;

        check_repo_lock(
            repo,
            kind,
            self.pushvars,
            ctx.metadata().identities(),
            authz,
        )
        .await?;

        let mut txn = txn.unwrap_or_else(|| repo.bookmarks().create_transaction(ctx.clone()));

        let commits_to_log = match kind {
            BookmarkKind::Scratch => {
                ctx.scuba()
                    .clone()
                    .add("bookmark", self.bookmark.to_string())
                    .log_with_msg("Updating scratch bookmark", None);
                txn.update_scratch(&self.bookmark, self.targets.new, self.targets.old)?;

                vec![]
            }
            BookmarkKind::Publishing | BookmarkKind::PullDefaultPublishing => {
                crate::restrictions::check_restriction_ensure_ancestor_of(
                    ctx,
                    repo,
                    &self.bookmark,
                    self.targets.new,
                )
                .await?;

                let txn_hook_fut = crate::git_mapping::populate_git_mapping_txn_hook(
                    ctx,
                    repo,
                    self.targets.new,
                    self.affected_changesets.new_changesets(),
                );

                let to_log = async {
                    if self.log_new_public_commits_to_scribe {
                        let res = find_draft_ancestors(ctx, repo, self.targets.new).await;
                        match res {
                            Ok(bcss) => bcss,
                            Err(err) => {
                                ctx.scuba().clone().log_with_msg(
                                    "Failed to find draft ancestors",
                                    Some(format!("{}", err)),
                                );
                                vec![]
                            }
                        }
                    } else {
                        vec![]
                    }
                };

                let (txn_hook_res, to_log) = futures::join!(txn_hook_fut, to_log);
                if let Some(txn_hook) = txn_hook_res? {
                    txn_hooks.push(txn_hook);
                }

                ctx.scuba()
                    .clone()
                    .add("bookmark", self.bookmark.to_string())
                    .log_with_msg("Updating public bookmark", None);

                txn.update(
                    &self.bookmark,
                    self.targets.new,
                    self.targets.old,
                    self.reason,
                )?;
                to_log
            }
        };
        let info = BookmarkInfo {
            bookmark_name: self.bookmark.clone(),
            bookmark_kind: kind,
            operation: BookmarkOperation::Update(self.targets.old, self.targets.new),
            reason: self.reason,
        };
        let info_data =
            BookmarkInfoData::new(info, self.log_new_public_commits_to_scribe, commits_to_log);

        Ok(BookmarkInfoTransaction::new(info_data, txn, txn_hooks))
    }

    pub async fn run(
        self,
        ctx: &'op CoreContext,
        authz: &'op AuthorizationContext,
        repo: &'op impl Repo,
        hook_manager: &'op HookManager,
    ) -> Result<BookmarkUpdateLogId, BookmarkMovementError> {
        let info_txn = self
            .run_with_transaction(ctx, authz, repo, hook_manager, None, vec![])
            .await?;
        info_txn.commit_and_log(ctx, repo).await
    }
}
