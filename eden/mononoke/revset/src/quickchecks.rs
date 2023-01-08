/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use crate::tests::*;

#[cfg(test)]
mod test {
    use std::collections::HashSet;
    use std::sync::Arc;

    use anyhow::Error;
    use blobrepo::BlobRepo;
    use bookmarks::BookmarksMaybeStaleExt;
    use bookmarks::BookmarksRef;
    use changeset_fetcher::ArcChangesetFetcher;
    use changeset_fetcher::ChangesetFetcherArc;
    use cloned::cloned;
    use context::CoreContext;
    use fbinit::FacebookInit;
    use futures::compat::Stream01CompatExt;
    use futures::stream::StreamExt as _;
    use futures::TryStreamExt;
    use futures_ext::BoxFuture;
    use futures_ext::BoxStream;
    use futures_ext::StreamExt;
    use futures_old::future::ok;
    use futures_old::Stream;
    use mononoke_types::ChangesetId;
    use quickcheck::quickcheck;
    use quickcheck::Arbitrary;
    use quickcheck::Gen;
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use rand::Rng;
    use revset_test_helper::single_changeset_id;
    use skiplist::SkiplistIndex;

    use super::*;
    use crate::ancestors::AncestorsNodeStream;
    use crate::ancestorscombinators::DifferenceOfUnionsOfAncestorsNodeStream;
    use crate::fixtures::BranchEven;
    use crate::fixtures::BranchUneven;
    use crate::fixtures::BranchWide;
    use crate::fixtures::Linear;
    use crate::fixtures::MergeEven;
    use crate::fixtures::MergeUneven;
    use crate::fixtures::TestRepoFixture;
    use crate::fixtures::UnsharedMergeEven;
    use crate::fixtures::UnsharedMergeUneven;
    use crate::intersectnodestream::IntersectNodeStream;
    use crate::setdifferencenodestream::SetDifferenceNodeStream;
    use crate::unionnodestream::UnionNodeStream;
    use crate::validation::ValidateNodeStream;
    use crate::BonsaiNodeStream;

    #[derive(Clone, Copy, Debug)]
    enum RevsetEntry {
        SingleNode(Option<ChangesetId>),
        SetDifference,
        Intersect(usize),
        Union(usize),
    }

    #[derive(Clone, Debug)]
    pub struct RevsetSpec {
        rp_entries: Vec<RevsetEntry>,
    }

    async fn get_changesets_from_repo(ctx: CoreContext, repo: &BlobRepo) -> Vec<ChangesetId> {
        let mut all_changesets_stream = repo
            .bookmarks()
            .get_heads_maybe_stale(ctx.clone())
            .compat() // conversion is needed as AncestorsNodeStream is an OldStream
            .map({
                cloned!(ctx);
                move |head| {
                    AncestorsNodeStream::new(ctx.clone(), &repo.changeset_fetcher_arc(), head)
                }
            })
            .flatten()
            .compat();

        let mut all_changesets: Vec<ChangesetId> = Vec::new();
        loop {
            all_changesets.push(match all_changesets_stream.next().await {
                None => break,
                Some(changeset) => changeset.expect("Failed to get changesets from repo"),
            });
        }

        assert!(!all_changesets.is_empty(), "Repo has no changesets");
        all_changesets
    }

    impl RevsetSpec {
        pub async fn add_hashes<G>(&mut self, ctx: CoreContext, repo: &BlobRepo, random: &mut G)
        where
            G: Rng,
        {
            let all_changesets = get_changesets_from_repo(ctx, repo).await;
            for elem in self.rp_entries.iter_mut() {
                if let &mut RevsetEntry::SingleNode(None) = elem {
                    *elem =
                        RevsetEntry::SingleNode(all_changesets.as_slice().choose(random).cloned());
                }
            }
        }

        pub fn as_hashes(&self) -> HashSet<ChangesetId> {
            let mut output: Vec<HashSet<ChangesetId>> = Vec::new();
            for entry in self.rp_entries.iter() {
                match *entry {
                    RevsetEntry::SingleNode(None) => panic!("You need to add_hashes first!"),
                    RevsetEntry::SingleNode(Some(hash)) => {
                        let mut item = HashSet::new();
                        item.insert(hash);
                        output.push(item)
                    }
                    RevsetEntry::SetDifference => {
                        let keep = output.pop().expect("No keep for setdifference");
                        let remove = output.pop().expect("No remove for setdifference");
                        output.push(keep.difference(&remove).copied().collect())
                    }
                    RevsetEntry::Union(size) => {
                        let idx = output.len() - size;
                        let mut inputs = output.split_off(idx).into_iter();
                        let first = inputs.next().expect("No first element");
                        output.push(inputs.fold(first, |a, b| a.union(&b).copied().collect()))
                    }
                    RevsetEntry::Intersect(size) => {
                        let idx = output.len() - size;
                        let mut inputs = output.split_off(idx).into_iter();
                        let first = inputs.next().expect("No first element");
                        output
                            .push(inputs.fold(first, |a, b| a.intersection(&b).copied().collect()))
                    }
                }
            }
            assert!(
                output.len() == 1,
                "output should have been length 1, was {}",
                output.len()
            );
            output.pop().expect("No revisions").into_iter().collect()
        }

        pub fn as_revset(&self, ctx: CoreContext, repo: BlobRepo) -> BonsaiNodeStream {
            let mut output: Vec<BonsaiNodeStream> = Vec::with_capacity(self.rp_entries.len());
            let changeset_fetcher: ArcChangesetFetcher =
                Arc::new(TestChangesetFetcher::new(repo.clone()));
            for entry in self.rp_entries.iter() {
                let next_node = ValidateNodeStream::new(
                    ctx.clone(),
                    match *entry {
                        RevsetEntry::SingleNode(None) => panic!("You need to add_hashes first!"),
                        RevsetEntry::SingleNode(Some(hash)) => {
                            single_changeset_id(ctx.clone(), hash, &repo).boxify()
                        }
                        RevsetEntry::SetDifference => {
                            let keep = output.pop().expect("No keep for setdifference");
                            let remove = output.pop().expect("No remove for setdifference");
                            SetDifferenceNodeStream::new(
                                ctx.clone(),
                                &changeset_fetcher,
                                keep,
                                remove,
                            )
                            .boxify()
                        }
                        RevsetEntry::Union(size) => {
                            let idx = output.len() - size;
                            let inputs = output.split_off(idx);

                            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs).boxify()
                        }
                        RevsetEntry::Intersect(size) => {
                            let idx = output.len() - size;
                            let inputs = output.split_off(idx);
                            IntersectNodeStream::new(
                                ctx.clone(),
                                &repo.changeset_fetcher_arc(),
                                inputs,
                            )
                            .boxify()
                        }
                    },
                    &repo.changeset_fetcher_arc(),
                )
                .boxify();
                output.push(next_node);
            }
            assert!(
                output.len() == 1,
                "output should have been length 1, was {}",
                output.len()
            );
            output.pop().expect("No revset entries")
        }
    }

    impl Arbitrary for RevsetSpec {
        fn arbitrary(g: &mut Gen) -> Self {
            let mut revset: Vec<RevsetEntry> = Vec::with_capacity(g.size());
            let mut revspecs_in_set: usize = 0;

            for _ in 0..g.size() {
                if revspecs_in_set == 0 {
                    // Can't add a set operator if we have don't have at least one node
                    revset.push(RevsetEntry::SingleNode(None));
                } else {
                    let input_count = (usize::arbitrary(g) % revspecs_in_set) + 1;
                    revset.push(
                        // Bias towards SingleNode if we only have 1 rev
                        match g.choose(&[0, 1, 2, 3]).unwrap() {
                            0 => RevsetEntry::SingleNode(None),
                            1 => {
                                if revspecs_in_set >= 2 {
                                    revspecs_in_set -= 2;
                                    RevsetEntry::SetDifference
                                } else {
                                    RevsetEntry::SingleNode(None)
                                }
                            }
                            2 => {
                                revspecs_in_set -= input_count;
                                RevsetEntry::Intersect(input_count)
                            }
                            3 => {
                                revspecs_in_set -= input_count;
                                RevsetEntry::Union(input_count)
                            }
                            _ => panic!("Range returned too wide a variation"),
                        },
                    );
                }
                revspecs_in_set += 1;
            }
            assert!(revspecs_in_set > 0, "Did not produce enough revs");

            if revspecs_in_set > 1 {
                revset.push(match bool::arbitrary(g) {
                    true => RevsetEntry::Intersect(revspecs_in_set),
                    false => RevsetEntry::Union(revspecs_in_set),
                });
            }

            RevsetSpec { rp_entries: revset }
        }

        // TODO(simonfar) We should implement shrink(), but we face the issue of ensuring that the
        // resulting revset only contains one final item.
        // Rough sketch: Take the last element of the Vec, so that we're using the same final reduction
        // type. Vector shrink the rest of the Vec using the standard shrinker. Re-add the final
        // reduction type. Note that we then need to handle the case where the final reduction type
        // is a SetDifference by pure chance.
    }

    async fn match_streams(
        expected: BoxStream<ChangesetId, Error>,
        actual: BoxStream<ChangesetId, Error>,
    ) -> bool {
        let mut expected = {
            let mut nodestream = expected.compat();

            let mut expected = HashSet::new();
            loop {
                let hash = nodestream.next().await;
                match hash {
                    Some(hash) => {
                        let hash = hash.expect("unexpected error");
                        expected.insert(hash);
                    }
                    None => {
                        break;
                    }
                }
            }
            expected
        };

        let mut nodestream = actual.compat();

        while !expected.is_empty() {
            match nodestream.next().await {
                Some(hash) => {
                    let hash = hash.expect("unexpected error");
                    if !expected.remove(&hash) {
                        return false;
                    }
                }
                None => {
                    return false;
                }
            }
        }
        nodestream.next().await.is_none() && expected.is_empty()
    }

    async fn match_hashset_to_revset(
        ctx: CoreContext,
        repo: BlobRepo,
        mut set: RevsetSpec,
    ) -> bool {
        set.add_hashes(ctx.clone(), &repo, &mut thread_rng()).await;
        let mut hashes = set.as_hashes();
        let mut nodestream = set.as_revset(ctx, repo).compat();

        while !hashes.is_empty() {
            let hash = nodestream
                .next()
                .await
                .expect("Unexpected end of stream")
                .expect("Unexpected error");
            if !hashes.remove(&hash) {
                return false;
            }
        }
        nodestream.next().await.is_none() && hashes.is_empty()
    }

    // This is slightly icky. I would like to construct $test_name as setops_$repo, but concat_idents!
    // does not work the way I'd like it to. For now, make the user of this macro pass in both idents
    macro_rules! quickcheck_setops {
        ($test_name:ident, $repo:ident) => {
            #[test]
            fn $test_name() {
                #[tokio::main(flavor = "current_thread")]
                async fn prop(fb: FacebookInit, set: RevsetSpec) -> bool {
                    let ctx = CoreContext::test_mock(fb);
                    let repo = $repo::getrepo(fb).await;
                    match_hashset_to_revset(ctx, repo, set).await
                }

                quickcheck(prop as fn(FacebookInit, RevsetSpec) -> bool)
            }
        };
    }

    quickcheck_setops!(setops_branch_even, BranchEven);
    quickcheck_setops!(setops_branch_uneven, BranchUneven);
    quickcheck_setops!(setops_branch_wide, BranchWide);
    quickcheck_setops!(setops_linear, Linear);
    quickcheck_setops!(setops_merge_even, MergeEven);
    quickcheck_setops!(setops_merge_uneven, MergeUneven);
    quickcheck_setops!(setops_unshared_merge_even, UnsharedMergeEven);
    quickcheck_setops!(setops_unshared_merge_uneven, UnsharedMergeUneven);

    // Given a list of hashes, generates all possible combinations where each hash can be included,
    // excluded or discarded. So for [h1] outputs are:
    // ([h1], [])
    // ([], [h1])
    // ([], [])
    struct IncludeExcludeDiscardCombinationsIterator {
        hashes: Vec<ChangesetId>,
        index: u64,
    }

    impl IncludeExcludeDiscardCombinationsIterator {
        fn new(hashes: Vec<ChangesetId>) -> Self {
            Self { hashes, index: 0 }
        }

        fn generate_include_exclude(&self) -> (Vec<ChangesetId>, Vec<ChangesetId>) {
            let mut val = self.index;
            let mut include = vec![];
            let mut exclude = vec![];
            for i in (0..self.hashes.len()).rev() {
                let i_commit_state = val / 3_u64.pow(i as u32);
                val %= 3_u64.pow(i as u32);
                match i_commit_state {
                    0 => {
                        // Do nothing
                    }
                    1 => {
                        include.push(self.hashes[i].clone());
                    }
                    2 => {
                        exclude.push(self.hashes[i].clone());
                    }
                    _ => panic!(""),
                }
            }
            (include, exclude)
        }
    }

    impl Iterator for IncludeExcludeDiscardCombinationsIterator {
        type Item = (Vec<ChangesetId>, Vec<ChangesetId>);

        fn next(&mut self) -> Option<Self::Item> {
            let res = if self.index >= 3_u64.pow(self.hashes.len() as u32) {
                None
            } else {
                Some(self.generate_include_exclude())
            };
            self.index += 1;
            res
        }
    }

    macro_rules! ancestors_check {
        ($test_name:ident, $repo:ident) => {
            #[fbinit::test]
            async fn $test_name(fb: FacebookInit) {
                let ctx = CoreContext::test_mock(fb);

                let repo = $repo::getrepo(fb).await;
                let changeset_fetcher: ArcChangesetFetcher =
                    Arc::new(TestChangesetFetcher::new(repo.clone()));
                let repo = Arc::new(repo);

                let all_changesets = get_changesets_from_repo(ctx.clone(), &*repo).await;

                // Limit the number of changesets, otherwise tests take too much time
                let max_changesets = 7;
                let all_changesets: Vec<_> =
                    all_changesets.into_iter().take(max_changesets).collect();
                let iter = IncludeExcludeDiscardCombinationsIterator::new(all_changesets);
                for (include, exclude) in iter {
                    let difference_stream = create_skiplist(ctx.clone(), &repo)
                        .map({
                            cloned!(ctx, changeset_fetcher, exclude, include);
                            move |skiplist| {
                                DifferenceOfUnionsOfAncestorsNodeStream::new_with_excludes(
                                    ctx.clone(),
                                    &changeset_fetcher,
                                    skiplist,
                                    include.clone(),
                                    exclude.clone(),
                                )
                            }
                        })
                        .flatten_stream()
                        .boxify();

                    let actual =
                        ValidateNodeStream::new(ctx.clone(), difference_stream, &changeset_fetcher);

                    let mut includes = vec![];
                    for i in include.clone() {
                        includes.push(
                            AncestorsNodeStream::new(ctx.clone(), &changeset_fetcher, i).boxify(),
                        );
                    }

                    let mut excludes = vec![];
                    for i in exclude.clone() {
                        excludes.push(
                            AncestorsNodeStream::new(ctx.clone(), &changeset_fetcher, i).boxify(),
                        );
                    }
                    let includes =
                        UnionNodeStream::new(ctx.clone(), &changeset_fetcher, includes).boxify();
                    let excludes =
                        UnionNodeStream::new(ctx.clone(), &changeset_fetcher, excludes).boxify();
                    let expected = SetDifferenceNodeStream::new(
                        ctx.clone(),
                        &changeset_fetcher,
                        includes,
                        excludes,
                    )
                    .boxify();

                    assert!(
                        match_streams(expected, actual.boxify()).await,
                        "streams do not match for {:?} {:?}",
                        include,
                        exclude
                    );
                }
            }
        };
    }
    mod empty_skiplist_tests {
        use futures_ext::FutureExt;
        use futures_old::Future;

        use super::*;

        fn create_skiplist(
            _ctxt: CoreContext,
            _repo: &BlobRepo,
        ) -> BoxFuture<Arc<SkiplistIndex>, Error> {
            ok(Arc::new(SkiplistIndex::new())).boxify()
        }

        ancestors_check!(ancestors_check_branch_even, BranchEven);
        ancestors_check!(ancestors_check_branch_uneven, BranchUneven);
        ancestors_check!(ancestors_check_branch_wide, BranchWide);
        ancestors_check!(ancestors_check_linear, Linear);
        ancestors_check!(ancestors_check_merge_even, MergeEven);
        ancestors_check!(ancestors_check_merge_uneven, MergeUneven);
        ancestors_check!(ancestors_check_unshared_merge_even, UnsharedMergeEven);
        ancestors_check!(ancestors_check_unshared_merge_uneven, UnsharedMergeUneven);
    }

    mod full_skiplist_tests {
        use futures::stream::TryStreamExt;
        use futures_ext::FutureExt;
        use futures_old::Future;
        use futures_util::future::try_join_all;
        use futures_util::future::FutureExt as NewFutureExt;
        use futures_util::future::TryFutureExt;

        use super::*;

        fn create_skiplist(
            ctx: CoreContext,
            repo: &BlobRepo,
        ) -> BoxFuture<Arc<SkiplistIndex>, Error> {
            let changeset_fetcher = repo.changeset_fetcher_arc();
            let skiplist_index = Arc::new(SkiplistIndex::new());
            let max_index_depth = 100;

            cloned!(repo, ctx);
            async move {
                let heads = repo
                    .bookmarks()
                    .get_heads_maybe_stale(ctx.clone())
                    .try_collect::<Vec<_>>()
                    .await?;
                try_join_all(heads.into_iter().map(|head| {
                    cloned!(skiplist_index, ctx, changeset_fetcher);
                    async move {
                        skiplist_index
                            .add_node(&ctx, &changeset_fetcher, head, max_index_depth)
                            .await
                    }
                }))
                .await?;
                Ok(skiplist_index)
            }
            .boxed()
            .compat()
            .boxify()
        }

        ancestors_check!(ancestors_check_branch_even, BranchEven);
        ancestors_check!(ancestors_check_branch_uneven, BranchUneven);
        ancestors_check!(ancestors_check_branch_wide, BranchWide);
        ancestors_check!(ancestors_check_linear, Linear);
        ancestors_check!(ancestors_check_merge_even, MergeEven);
        ancestors_check!(ancestors_check_merge_uneven, MergeUneven);
        ancestors_check!(ancestors_check_unshared_merge_even, UnsharedMergeEven);
        ancestors_check!(ancestors_check_unshared_merge_uneven, UnsharedMergeUneven);
    }
}
