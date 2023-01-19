/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::hash_set::IntoIter;
use std::collections::HashSet;

use anyhow::Error;
use changeset_fetcher::ArcChangesetFetcher;
use context::CoreContext;
use futures_old::stream::Stream;
use futures_old::Async;
use futures_old::Poll;
use mononoke_types::ChangesetId;
use mononoke_types::Generation;

use crate::setcommon::*;
use crate::BonsaiNodeStream;

pub struct UnionNodeStream {
    inputs: Vec<(
        BonsaiInputStream,
        Poll<Option<(ChangesetId, Generation)>, Error>,
    )>,
    current_generation: Option<Generation>,
    accumulator: HashSet<ChangesetId>,
    drain: Option<IntoIter<ChangesetId>>,
}

impl UnionNodeStream {
    pub fn new<I>(ctx: CoreContext, changeset_fetcher: &ArcChangesetFetcher, inputs: I) -> Self
    where
        I: IntoIterator<Item = BonsaiNodeStream>,
    {
        let csid_and_gen = inputs.into_iter().map(move |i| {
            (
                add_generations_by_bonsai(ctx.clone(), i, changeset_fetcher.clone()),
                Ok(Async::NotReady),
            )
        });
        Self {
            inputs: csid_and_gen.collect(),
            current_generation: None,
            accumulator: HashSet::new(),
            drain: None,
        }
    }

    fn gc_finished_inputs(&mut self) {
        self.inputs.retain(|&(_, ref state)| {
            if let Ok(Async::Ready(None)) = *state {
                false
            } else {
                true
            }
        });
    }

    fn update_current_generation(&mut self) {
        if all_inputs_ready(&self.inputs) {
            self.current_generation = self
                .inputs
                .iter()
                .filter_map(|(_, state)| match state {
                    Ok(Async::Ready(Some((_, gen_id)))) => Some(*gen_id),
                    Ok(Async::NotReady) => panic!("All states ready, yet some not ready!"),
                    _ => None,
                })
                .max();
        }
    }

    fn accumulate_nodes(&mut self) {
        let mut found_csids = false;
        for &mut (_, ref mut state) in self.inputs.iter_mut() {
            if let Ok(Async::Ready(Some((csid, gen_id)))) = *state {
                if Some(gen_id) == self.current_generation {
                    found_csids = true;
                    self.accumulator.insert(csid);
                    *state = Ok(Async::NotReady);
                }
            }
        }
        if !found_csids {
            self.current_generation = None;
        }
    }
}

impl Stream for UnionNodeStream {
    type Item = ChangesetId;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // This feels wrong, but in practice it's fine - it should be quick to hit a return, and
        // the standard futures_old::executor expects you to only return NotReady if blocked on I/O.
        loop {
            // Start by trying to turn as many NotReady as possible into real items
            poll_all_inputs(&mut self.inputs);

            // Empty the drain if any - return all items for this generation
            let next_in_drain = self.drain.as_mut().and_then(|drain| drain.next());
            if next_in_drain.is_some() {
                return Ok(Async::Ready(next_in_drain));
            } else {
                self.drain = None;
            }

            // Return any errors
            {
                if self.inputs.iter().any(|&(_, ref state)| state.is_err()) {
                    let inputs = std::mem::take(&mut self.inputs);
                    let (_, err) = inputs
                        .into_iter()
                        .find(|&(_, ref state)| state.is_err())
                        .unwrap();
                    return Err(err.unwrap_err());
                }
            }

            self.gc_finished_inputs();

            // If any input is not ready (we polled above), wait for them all to be ready
            if !all_inputs_ready(&self.inputs) {
                return Ok(Async::NotReady);
            }

            match self.current_generation {
                None => {
                    if self.accumulator.is_empty() {
                        self.update_current_generation();
                    } else {
                        let full_accumulator = std::mem::take(&mut self.accumulator);
                        self.drain = Some(full_accumulator.into_iter());
                    }
                }
                Some(_) => self.accumulate_nodes(),
            }
            // If we cannot ever output another node, we're done.
            if self.inputs.is_empty() && self.drain.is_none() && self.accumulator.is_empty() {
                return Ok(Async::Ready(None));
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use context::CoreContext;
    use failure_ext::err_downcast;
    use fbinit::FacebookInit;
    use futures::compat::Stream01CompatExt;
    use futures::stream::StreamExt as _;
    use futures_ext::StreamExt;
    use futures_old::executor::spawn;
    use revset_test_helper::assert_changesets_sequence;
    use revset_test_helper::single_changeset_id;
    use revset_test_helper::string_to_bonsai;

    use super::*;
    use crate::errors::ErrorKind;
    use crate::fixtures::BranchEven;
    use crate::fixtures::BranchUneven;
    use crate::fixtures::BranchWide;
    use crate::fixtures::Linear;
    use crate::fixtures::TestRepoFixture;
    use crate::setcommon::NotReadyEmptyStream;
    use crate::setcommon::RepoErrorStream;
    use crate::tests::get_single_bonsai_streams;
    use crate::tests::TestChangesetFetcher;
    use crate::BonsaiNodeStream;

    #[fbinit::test]
    async fn union_identical_node(fb: FacebookInit) {
        let ctx = CoreContext::test_mock(fb);
        let repo = Linear::getrepo(fb).await;
        let changeset_fetcher: ArcChangesetFetcher =
            Arc::new(TestChangesetFetcher::new(repo.clone()));
        let repo = Arc::new(repo);

        let hash = "a5ffa77602a066db7d5cfb9fb5823a0895717c5a";
        let head_csid = string_to_bonsai(fb, &repo, hash).await;

        let inputs: Vec<BonsaiNodeStream> = vec![
            single_changeset_id(ctx.clone(), head_csid.clone(), &repo).boxify(),
            single_changeset_id(ctx.clone(), head_csid.clone(), &repo).boxify(),
        ];
        let nodestream =
            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs.into_iter()).boxify();

        assert_changesets_sequence(&ctx, &repo, vec![head_csid.clone()], nodestream).await;
    }

    #[fbinit::test]
    async fn union_error_node(fb: FacebookInit) {
        let ctx = CoreContext::test_mock(fb);
        let repo = Linear::getrepo(fb).await;
        let changeset_fetcher: ArcChangesetFetcher =
            Arc::new(TestChangesetFetcher::new(repo.clone()));
        let repo = Arc::new(repo);

        let hash = "a5ffa77602a066db7d5cfb9fb5823a0895717c5a";
        let expected_csid = string_to_bonsai(fb, &repo, hash).await;

        let inputs: Vec<BonsaiNodeStream> = vec![
            RepoErrorStream {
                item: expected_csid,
            }
            .boxify(),
            single_changeset_id(ctx.clone(), expected_csid.clone(), &repo).boxify(),
        ];
        let mut nodestream = spawn(
            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs.into_iter()).boxify(),
        );

        match nodestream.wait_stream() {
            Some(Err(err)) => match err_downcast!(err, err: ErrorKind => err) {
                Ok(ErrorKind::RepoChangesetError(cs)) => assert_eq!(cs, expected_csid),
                Ok(bad) => panic!("unexpected error {:?}", bad),
                Err(bad) => panic!("unknown error {:?}", bad),
            },
            Some(Ok(bad)) => panic!("unexpected success {:?}", bad),
            None => panic!("no result"),
        };
    }

    #[fbinit::test]
    async fn union_three_nodes(fb: FacebookInit) {
        let ctx = CoreContext::test_mock(fb);
        let repo = Linear::getrepo(fb).await;
        let changeset_fetcher: ArcChangesetFetcher =
            Arc::new(TestChangesetFetcher::new(repo.clone()));
        let repo = Arc::new(repo);

        let bcs_d0a = string_to_bonsai(fb, &repo, "d0a361e9022d226ae52f689667bd7d212a19cfe0").await;
        let bcs_3c1 = string_to_bonsai(fb, &repo, "3c15267ebf11807f3d772eb891272b911ec68759").await;
        let bcs_a947 =
            string_to_bonsai(fb, &repo, "a9473beb2eb03ddb1cccc3fbaeb8a4820f9cd157").await;
        // Note that these are *not* in generation order deliberately.
        let inputs: Vec<BonsaiNodeStream> = vec![
            single_changeset_id(ctx.clone(), bcs_a947, &repo).boxify(),
            single_changeset_id(ctx.clone(), bcs_3c1, &repo).boxify(),
            single_changeset_id(ctx.clone(), bcs_d0a, &repo).boxify(),
        ];
        let nodestream =
            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs.into_iter()).boxify();

        // But, once I hit the asserts, I expect them in generation order.
        assert_changesets_sequence(&ctx, &repo, vec![bcs_3c1, bcs_a947, bcs_d0a], nodestream).await;
    }

    #[fbinit::test]
    async fn union_nothing(fb: FacebookInit) {
        let ctx = CoreContext::test_mock(fb);
        let repo = Linear::getrepo(fb).await;
        let changeset_fetcher: ArcChangesetFetcher =
            Arc::new(TestChangesetFetcher::new(repo.clone()));
        let repo = Arc::new(repo);

        let inputs: Vec<BonsaiNodeStream> = vec![];
        let nodestream =
            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs.into_iter()).boxify();
        assert_changesets_sequence(&ctx, &repo, vec![], nodestream).await;
    }

    #[fbinit::test]
    async fn union_nesting(fb: FacebookInit) {
        let ctx = CoreContext::test_mock(fb);
        let repo = Linear::getrepo(fb).await;
        let changeset_fetcher: ArcChangesetFetcher =
            Arc::new(TestChangesetFetcher::new(repo.clone()));
        let repo = Arc::new(repo);

        let bcs_d0a = string_to_bonsai(fb, &repo, "d0a361e9022d226ae52f689667bd7d212a19cfe0").await;
        let bcs_3c1 = string_to_bonsai(fb, &repo, "3c15267ebf11807f3d772eb891272b911ec68759").await;
        // Note that these are *not* in generation order deliberately.
        let inputs: Vec<BonsaiNodeStream> = vec![
            single_changeset_id(ctx.clone(), bcs_d0a, &repo).boxify(),
            single_changeset_id(ctx.clone(), bcs_3c1, &repo).boxify(),
        ];

        let nodestream =
            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs.into_iter()).boxify();

        let bcs_a947 =
            string_to_bonsai(fb, &repo, "a9473beb2eb03ddb1cccc3fbaeb8a4820f9cd157").await;
        let inputs: Vec<BonsaiNodeStream> = vec![
            nodestream,
            single_changeset_id(ctx.clone(), bcs_a947, &repo).boxify(),
        ];
        let nodestream =
            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs.into_iter()).boxify();

        assert_changesets_sequence(&ctx, &repo, vec![bcs_3c1, bcs_a947, bcs_d0a], nodestream).await;
    }

    #[fbinit::test]
    async fn slow_ready_union_nothing(fb: FacebookInit) {
        // Tests that we handle an input staying at NotReady for a while without panicking
        let ctx = CoreContext::test_mock(fb);
        let repo = Linear::getrepo(fb).await;
        let changeset_fetcher: ArcChangesetFetcher = Arc::new(TestChangesetFetcher::new(repo));

        let inputs: Vec<BonsaiNodeStream> = vec![NotReadyEmptyStream::new(10).boxify()];
        let mut nodestream =
            UnionNodeStream::new(ctx, &changeset_fetcher, inputs.into_iter()).compat();

        assert!(nodestream.next().await.is_none());
    }

    #[fbinit::test]
    async fn union_branch_even_repo(fb: FacebookInit) {
        let ctx = CoreContext::test_mock(fb);
        let repo = BranchEven::getrepo(fb).await;
        let changeset_fetcher: ArcChangesetFetcher =
            Arc::new(TestChangesetFetcher::new(repo.clone()));
        let repo = Arc::new(repo);

        let nodes = vec![
            string_to_bonsai(fb, &repo, "4f7f3fd428bec1a48f9314414b063c706d9c1aed").await,
            string_to_bonsai(fb, &repo, "3cda5c78aa35f0f5b09780d971197b51cad4613a").await,
            string_to_bonsai(fb, &repo, "d7542c9db7f4c77dab4b315edd328edf1514952f").await,
        ];

        // Two nodes should share the same generation number
        let inputs: Vec<BonsaiNodeStream> = nodes
            .clone()
            .into_iter()
            .map(|cs| single_changeset_id(ctx.clone(), cs, &repo).boxify())
            .collect();
        let nodestream =
            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs.into_iter()).boxify();
        assert_changesets_sequence(&ctx, &repo, nodes, nodestream).await;
    }

    #[fbinit::test]
    async fn union_branch_uneven_repo(fb: FacebookInit) {
        let ctx = CoreContext::test_mock(fb);
        let repo = BranchUneven::getrepo(fb).await;
        let changeset_fetcher: ArcChangesetFetcher =
            Arc::new(TestChangesetFetcher::new(repo.clone()));
        let repo = Arc::new(repo);

        let cs_1 = string_to_bonsai(fb, &repo, "3cda5c78aa35f0f5b09780d971197b51cad4613a").await;
        let cs_2 = string_to_bonsai(fb, &repo, "d7542c9db7f4c77dab4b315edd328edf1514952f").await;
        let cs_3 = string_to_bonsai(fb, &repo, "4f7f3fd428bec1a48f9314414b063c706d9c1aed").await;
        let cs_4 = string_to_bonsai(fb, &repo, "bc7b4d0f858c19e2474b03e442b8495fd7aeef33").await;
        let cs_5 = string_to_bonsai(fb, &repo, "264f01429683b3dd8042cb3979e8bf37007118bc").await;
        // Two nodes should share the same generation number
        let inputs: Vec<BonsaiNodeStream> = vec![
            single_changeset_id(ctx.clone(), cs_1.clone(), &repo).boxify(),
            single_changeset_id(ctx.clone(), cs_2.clone(), &repo).boxify(),
            single_changeset_id(ctx.clone(), cs_3.clone(), &repo).boxify(),
            single_changeset_id(ctx.clone(), cs_4.clone(), &repo).boxify(),
            single_changeset_id(ctx.clone(), cs_5.clone(), &repo).boxify(),
        ];
        let nodestream =
            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs.into_iter()).boxify();

        assert_changesets_sequence(&ctx, &repo, vec![cs_5, cs_4, cs_3, cs_1, cs_2], nodestream)
            .await;
    }

    #[fbinit::test]
    async fn union_branch_wide_repo(fb: FacebookInit) {
        let ctx = CoreContext::test_mock(fb);
        let repo = BranchWide::getrepo(fb).await;
        let changeset_fetcher: ArcChangesetFetcher =
            Arc::new(TestChangesetFetcher::new(repo.clone()));
        let repo = Arc::new(repo);

        // Two nodes should share the same generation number
        let inputs = get_single_bonsai_streams(
            ctx.clone(),
            &repo,
            &[
                "49f53ab171171b3180e125b918bd1cf0af7e5449",
                "4685e9e62e4885d477ead6964a7600c750e39b03",
                "c27ef5b7f15e9930e5b93b1f32cc2108a2aabe12",
                "9e8521affb7f9d10e9551a99c526e69909042b20",
            ],
        )
        .await;
        let nodestream =
            UnionNodeStream::new(ctx.clone(), &changeset_fetcher, inputs.into_iter()).boxify();

        assert_changesets_sequence(
            &ctx,
            &repo,
            vec![
                string_to_bonsai(fb, &repo, "49f53ab171171b3180e125b918bd1cf0af7e5449").await,
                string_to_bonsai(fb, &repo, "c27ef5b7f15e9930e5b93b1f32cc2108a2aabe12").await,
                string_to_bonsai(fb, &repo, "4685e9e62e4885d477ead6964a7600c750e39b03").await,
                string_to_bonsai(fb, &repo, "9e8521affb7f9d10e9551a99c526e69909042b20").await,
            ],
            nodestream,
        )
        .await;
    }
}
