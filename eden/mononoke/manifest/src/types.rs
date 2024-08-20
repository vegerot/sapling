/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::hash::Hash;
use std::hash::Hasher;

use anyhow::bail;
use anyhow::Result;
use async_trait::async_trait;
use blobstore::Blobstore;
use blobstore::Loadable;
use blobstore::LoadableError;
use blobstore::Storable;
use blobstore::StoreLoadable;
use context::CoreContext;
use either::Either;
use futures::stream;
use futures::stream::BoxStream;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;
use futures::try_join;
use mononoke_types::basename_suffix_skeleton_manifest_v3::BssmV3Directory;
use mononoke_types::basename_suffix_skeleton_manifest_v3::BssmV3Entry;
use mononoke_types::fsnode::Fsnode;
use mononoke_types::fsnode::FsnodeEntry;
use mononoke_types::fsnode::FsnodeFile;
use mononoke_types::sharded_map_v2::LoadableShardedMapV2Node;
use mononoke_types::skeleton_manifest::SkeletonManifest;
use mononoke_types::skeleton_manifest::SkeletonManifestEntry;
use mononoke_types::test_manifest::TestManifest;
use mononoke_types::test_manifest::TestManifestDirectory;
use mononoke_types::test_manifest::TestManifestEntry;
use mononoke_types::test_sharded_manifest::TestShardedManifest;
use mononoke_types::test_sharded_manifest::TestShardedManifestDirectory;
use mononoke_types::test_sharded_manifest::TestShardedManifestEntry;
use mononoke_types::unode::ManifestUnode;
use mononoke_types::unode::UnodeEntry;
use mononoke_types::FileUnodeId;
use mononoke_types::FsnodeId;
use mononoke_types::MPathElement;
use mononoke_types::ManifestUnodeId;
use mononoke_types::SkeletonManifestId;
use mononoke_types::SortedVectorTrieMap;
use mononoke_types::TrieMap;
use serde_derive::Deserialize;
use serde_derive::Serialize;
use smallvec::SmallVec;

#[async_trait]
pub trait TrieMapOps<Store, Value>: Sized {
    async fn expand(
        self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<(Option<Value>, Vec<(u8, Self)>)>;

    async fn into_stream(
        self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(SmallVec<[u8; 24]>, Value)>>>;

    fn is_empty(&self) -> bool;
}

#[async_trait]
impl<Store, V: Send> TrieMapOps<Store, V> for TrieMap<V> {
    async fn expand(
        self,
        _ctx: &CoreContext,
        _blobstore: &Store,
    ) -> Result<(Option<V>, Vec<(u8, Self)>)> {
        Ok(self.expand())
    }

    async fn into_stream(
        self,
        _ctx: &CoreContext,
        _blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(SmallVec<[u8; 24]>, V)>>> {
        Ok(stream::iter(self).map(Ok).boxed())
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

#[async_trait]
impl<Store, V: Clone + Send + Sync> TrieMapOps<Store, V> for SortedVectorTrieMap<V> {
    async fn expand(
        self,
        _ctx: &CoreContext,
        _blobstore: &Store,
    ) -> Result<(Option<V>, Vec<(u8, Self)>)> {
        SortedVectorTrieMap::expand(self)
    }

    async fn into_stream(
        self,
        _ctx: &CoreContext,
        _blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(SmallVec<[u8; 24]>, V)>>> {
        Ok(stream::iter(self).map(Ok).boxed())
    }

    fn is_empty(&self) -> bool {
        SortedVectorTrieMap::is_empty(self)
    }
}

#[async_trait]
impl<Store: Blobstore> TrieMapOps<Store, Entry<TestShardedManifestDirectory, ()>>
    for LoadableShardedMapV2Node<TestShardedManifestEntry>
{
    async fn expand(
        self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<(
        Option<Entry<TestShardedManifestDirectory, ()>>,
        Vec<(u8, Self)>,
    )> {
        let (entry, children) = self.expand(ctx, blobstore).await?;
        Ok((entry.map(convert_test_sharded_manifest), children))
    }

    async fn into_stream(
        self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<
        BoxStream<
            'async_trait,
            Result<(SmallVec<[u8; 24]>, Entry<TestShardedManifestDirectory, ()>)>,
        >,
    > {
        Ok(self
            .load(ctx, blobstore)
            .await?
            .into_entries(ctx, blobstore)
            .map_ok(|(k, v)| (k, convert_test_sharded_manifest(v)))
            .boxed())
    }

    fn is_empty(&self) -> bool {
        self.size() == 0
    }
}

#[async_trait]
impl<Store: Blobstore> TrieMapOps<Store, Entry<BssmV3Directory, ()>>
    for LoadableShardedMapV2Node<BssmV3Entry>
{
    async fn expand(
        self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<(Option<Entry<BssmV3Directory, ()>>, Vec<(u8, Self)>)> {
        let (entry, children) = self.expand(ctx, blobstore).await?;
        Ok((entry.map(bssm_v3_to_mf_entry), children))
    }

    async fn into_stream(
        self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(SmallVec<[u8; 24]>, Entry<BssmV3Directory, ()>)>>>
    {
        Ok(self
            .load(ctx, blobstore)
            .await?
            .into_entries(ctx, blobstore)
            .map_ok(|(k, v)| (k, bssm_v3_to_mf_entry(v)))
            .boxed())
    }

    fn is_empty(&self) -> bool {
        self.size() == 0
    }
}

#[async_trait]
pub trait AsyncManifest<Store: Send + Sync>: Sized + 'static {
    type TreeId: Send + Sync;
    type LeafId: Send + Sync;
    type TrieMapType: Send + Sync;

    async fn list(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>;
    /// List all subentries with a given prefix
    async fn list_prefix(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>;
    /// List all subentries with a given prefix after a specific key
    async fn list_prefix_after(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
        after: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>;
    /// List all subentries, skipping the first N
    async fn list_skip(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        skip: usize,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>;
    async fn lookup(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        name: &MPathElement,
    ) -> Result<Option<Entry<Self::TreeId, Self::LeafId>>>;
    async fn into_trie_map(self, ctx: &CoreContext, blobstore: &Store)
    -> Result<Self::TrieMapType>;
}

pub trait Manifest: Sync + Sized + 'static {
    type TreeId: Send + Sync;
    type LeafId: Send + Sync;
    fn list(&self) -> Box<dyn Iterator<Item = (MPathElement, Entry<Self::TreeId, Self::LeafId>)>>;
    /// List all subentries with a given prefix
    fn list_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> Box<dyn Iterator<Item = (MPathElement, Entry<Self::TreeId, Self::LeafId>)> + 'a> {
        Box::new(self.list().filter(|(k, _)| k.starts_with(prefix)))
    }
    fn list_prefix_after<'a>(
        &'a self,
        prefix: &'a [u8],
        after: &'a [u8],
    ) -> Box<dyn Iterator<Item = (MPathElement, Entry<Self::TreeId, Self::LeafId>)> + 'a> {
        Box::new(
            self.list()
                .filter(move |(k, _)| k.as_ref() > after && k.starts_with(prefix)),
        )
    }
    fn list_skip<'a>(
        &'a self,
        skip: usize,
    ) -> Box<dyn Iterator<Item = (MPathElement, Entry<Self::TreeId, Self::LeafId>)> + 'a> {
        Box::new(self.list().skip(skip))
    }
    fn lookup(&self, name: &MPathElement) -> Option<Entry<Self::TreeId, Self::LeafId>>;
}

#[async_trait]
impl<M: Manifest + Send, Store: Send + Sync> AsyncManifest<Store> for M {
    type TreeId = <Self as Manifest>::TreeId;
    type LeafId = <Self as Manifest>::LeafId;
    type TrieMapType = SortedVectorTrieMap<Entry<Self::TreeId, Self::LeafId>>;

    async fn list(
        &self,
        _ctx: &CoreContext,
        _blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        Ok(stream::iter(Manifest::list(self).map(anyhow::Ok).collect::<Vec<_>>()).boxed())
    }

    async fn list_prefix(
        &self,
        _ctx: &CoreContext,
        _blobstore: &Store,
        prefix: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        Ok(stream::iter(
            Manifest::list_prefix(self, prefix)
                .map(anyhow::Ok)
                .collect::<Vec<_>>(),
        )
        .boxed())
    }

    async fn list_prefix_after(
        &self,
        _ctx: &CoreContext,
        _blobstore: &Store,
        prefix: &[u8],
        after: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        Ok(stream::iter(
            Manifest::list_prefix_after(self, prefix, after)
                .map(anyhow::Ok)
                .collect::<Vec<_>>(),
        )
        .boxed())
    }

    async fn list_skip(
        &self,
        _ctx: &CoreContext,
        _blobstore: &Store,
        skip: usize,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        Ok(stream::iter(
            Manifest::list_skip(self, skip)
                .map(anyhow::Ok)
                .collect::<Vec<_>>(),
        )
        .boxed())
    }

    async fn lookup(
        &self,
        _ctx: &CoreContext,
        _blobstore: &Store,
        name: &MPathElement,
    ) -> Result<Option<Entry<Self::TreeId, Self::LeafId>>> {
        anyhow::Ok(Manifest::lookup(self, name))
    }

    async fn into_trie_map(
        self,
        _ctx: &CoreContext,
        _blobstore: &Store,
    ) -> Result<Self::TrieMapType> {
        let entries = Manifest::list(&self)
            .map(|(k, v)| (k.to_smallvec(), v))
            .collect();
        Ok(SortedVectorTrieMap::new(entries))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CombinedId<M, N>(pub M, pub N);

pub struct Combined<M, N>(pub M, pub N);

fn combine_entries<
    M: AsyncManifest<Store> + Send + Sync,
    N: AsyncManifest<Store> + Send + Sync,
    Store: Send + Sync,
>(
    (m_result, n_result): (
        Result<(MPathElement, Entry<M::TreeId, M::LeafId>)>,
        Result<(MPathElement, Entry<N::TreeId, N::LeafId>)>,
    ),
) -> Result<(
    MPathElement,
    Entry<
        <Combined<M, N> as AsyncManifest<Store>>::TreeId,
        <Combined<M, N> as AsyncManifest<Store>>::LeafId,
    >,
)> {
    let (m_elem, m_entry) = m_result?;
    let (n_elem, n_entry) = n_result?;

    match (m_elem == n_elem, m_entry, n_entry) {
        (true, Entry::Tree(m_tree), Entry::Tree(n_tree)) => {
            Ok((m_elem, Entry::Tree(CombinedId(m_tree, n_tree))))
        }
        (true, Entry::Leaf(m_leaf), Entry::Leaf(n_leaf)) => {
            Ok((m_elem, Entry::Leaf(CombinedId(m_leaf, n_leaf))))
        }
        _ => bail!(
            "Found non-matching entries while iterating over a pair of manifests: {} vs {}",
            m_elem,
            n_elem,
        ),
    }
}

#[async_trait]
impl<S, M, N> StoreLoadable<S> for CombinedId<M, N>
where
    M: StoreLoadable<S> + Send + Sync + Clone + Eq,
    M::Value: Send + Sync,
    N: StoreLoadable<S> + Send + Sync + Clone + Eq,
    N::Value: Send + Sync,
    S: Send + Sync,
{
    type Value = Combined<M::Value, N::Value>;

    async fn load<'a>(
        &'a self,
        ctx: &'a CoreContext,
        store: &'a S,
    ) -> Result<Self::Value, LoadableError> {
        let CombinedId(m_id, n_id) = self;
        let (m, n) = try_join!(m_id.load(ctx, store), n_id.load(ctx, store))?;
        Ok(Combined(m, n))
    }
}

#[async_trait]
impl<
    M: AsyncManifest<Store> + Send + Sync,
    N: AsyncManifest<Store> + Send + Sync,
    Store: Send + Sync,
> AsyncManifest<Store> for Combined<M, N>
{
    type TreeId =
        CombinedId<<M as AsyncManifest<Store>>::TreeId, <N as AsyncManifest<Store>>::TreeId>;
    type LeafId =
        CombinedId<<M as AsyncManifest<Store>>::LeafId, <N as AsyncManifest<Store>>::LeafId>;
    type TrieMapType = TrieMap<Entry<Self::TreeId, Self::LeafId>>;

    async fn list(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        let Combined(m, n) = self;
        Ok(m.list(ctx, blobstore)
            .await?
            .zip(n.list(ctx, blobstore).await?)
            .map(combine_entries::<M, N, Store>)
            .boxed())
    }

    async fn list_prefix(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        let Combined(m, n) = self;
        Ok(m.list_prefix(ctx, blobstore, prefix)
            .await?
            .zip(n.list_prefix(ctx, blobstore, prefix).await?)
            .map(combine_entries::<M, N, Store>)
            .boxed())
    }

    async fn list_prefix_after(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
        after: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        let Combined(m, n) = self;
        Ok(m.list_prefix_after(ctx, blobstore, prefix, after)
            .await?
            .zip(n.list_prefix_after(ctx, blobstore, prefix, after).await?)
            .map(combine_entries::<M, N, Store>)
            .boxed())
    }

    async fn list_skip(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        skip: usize,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        let Combined(m, n) = self;
        Ok(m.list_skip(ctx, blobstore, skip)
            .await?
            .zip(n.list_skip(ctx, blobstore, skip).await?)
            .map(combine_entries::<M, N, Store>)
            .boxed())
    }

    async fn lookup(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        name: &MPathElement,
    ) -> Result<Option<Entry<Self::TreeId, Self::LeafId>>> {
        let Combined(m, n) = self;
        match (
            m.lookup(ctx, blobstore, name).await?,
            n.lookup(ctx, blobstore, name).await?,
        ) {
            (Some(Entry::Tree(m_tree)), Some(Entry::Tree(n_tree))) => {
                Ok(Some(Entry::Tree(CombinedId(m_tree, n_tree))))
            }
            (Some(Entry::Leaf(m_leaf)), Some(Entry::Leaf(n_leaf))) => {
                Ok(Some(Entry::Leaf(CombinedId(m_leaf, n_leaf))))
            }
            (None, None) => Ok(None),
            _ => bail!("Found non-matching entry types during lookup for {}", name),
        }
    }

    async fn into_trie_map(
        self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<Self::TrieMapType> {
        self.list(ctx, blobstore).await?.try_collect().await
    }
}

#[async_trait]
impl<
    M: AsyncManifest<Store> + Send + Sync,
    N: AsyncManifest<Store> + Send + Sync,
    Store: Send + Sync,
> AsyncManifest<Store> for Either<M, N>
{
    type TreeId = Either<<M as AsyncManifest<Store>>::TreeId, <N as AsyncManifest<Store>>::TreeId>;
    type LeafId = Either<<M as AsyncManifest<Store>>::LeafId, <N as AsyncManifest<Store>>::LeafId>;
    type TrieMapType =
        Either<<M as AsyncManifest<Store>>::TrieMapType, <N as AsyncManifest<Store>>::TrieMapType>;

    async fn list(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        let stream = match self {
            Either::Left(m) => m
                .list(ctx, blobstore)
                .await?
                .map_ok(|(path, entry)| (path, entry.left_entry()))
                .boxed(),
            Either::Right(n) => n
                .list(ctx, blobstore)
                .await?
                .map_ok(|(path, entry)| (path, entry.right_entry()))
                .boxed(),
        };
        Ok(stream)
    }

    async fn list_prefix(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        let stream = match self {
            Either::Left(m) => m
                .list_prefix(ctx, blobstore, prefix)
                .await?
                .map_ok(|(path, entry)| (path, entry.left_entry()))
                .boxed(),
            Either::Right(n) => n
                .list_prefix(ctx, blobstore, prefix)
                .await?
                .map_ok(|(path, entry)| (path, entry.right_entry()))
                .boxed(),
        };
        Ok(stream)
    }

    async fn list_prefix_after(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
        after: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        let stream = match self {
            Either::Left(m) => m
                .list_prefix_after(ctx, blobstore, prefix, after)
                .await?
                .map_ok(|(path, entry)| (path, entry.left_entry()))
                .boxed(),
            Either::Right(n) => n
                .list_prefix_after(ctx, blobstore, prefix, after)
                .await?
                .map_ok(|(path, entry)| (path, entry.right_entry()))
                .boxed(),
        };
        Ok(stream)
    }

    async fn list_skip(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        skip: usize,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        let stream = match self {
            Either::Left(m) => m
                .list_skip(ctx, blobstore, skip)
                .await?
                .map_ok(|(path, entry)| (path, entry.left_entry()))
                .boxed(),
            Either::Right(n) => n
                .list_skip(ctx, blobstore, skip)
                .await?
                .map_ok(|(path, entry)| (path, entry.right_entry()))
                .boxed(),
        };
        Ok(stream)
    }

    async fn lookup(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        name: &MPathElement,
    ) -> Result<Option<Entry<Self::TreeId, Self::LeafId>>> {
        match self {
            Either::Left(m) => Ok(m.lookup(ctx, blobstore, name).await?.map(Entry::left_entry)),
            Either::Right(n) => Ok(n
                .lookup(ctx, blobstore, name)
                .await?
                .map(Entry::right_entry)),
        }
    }

    async fn into_trie_map(
        self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<Self::TrieMapType> {
        match self {
            Either::Left(m) => Ok(Either::Left(m.into_trie_map(ctx, blobstore).await?)),
            Either::Right(n) => Ok(Either::Right(n.into_trie_map(ctx, blobstore).await?)),
        }
    }
}

fn bssm_v3_to_mf_entry(entry: BssmV3Entry) -> Entry<BssmV3Directory, ()> {
    match entry {
        BssmV3Entry::Directory(dir) => Entry::Tree(dir),
        BssmV3Entry::File => Entry::Leaf(()),
    }
}

#[async_trait]
impl<Store: Blobstore> AsyncManifest<Store> for BssmV3Directory {
    type TreeId = BssmV3Directory;
    type LeafId = ();
    type TrieMapType = LoadableShardedMapV2Node<BssmV3Entry>;

    async fn list(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        anyhow::Ok(
            self.clone()
                .into_subentries(ctx, blobstore)
                .map_ok(|(path, entry)| (path, bssm_v3_to_mf_entry(entry)))
                .boxed(),
        )
    }

    async fn list_prefix(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        anyhow::Ok(
            self.clone()
                .into_prefix_subentries(ctx, blobstore, prefix)
                .map_ok(|(path, entry)| (path, bssm_v3_to_mf_entry(entry)))
                .boxed(),
        )
    }

    async fn list_prefix_after(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
        after: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        anyhow::Ok(
            self.clone()
                .into_prefix_subentries_after(ctx, blobstore, prefix, after)
                .map_ok(|(path, entry)| (path, bssm_v3_to_mf_entry(entry)))
                .boxed(),
        )
    }

    async fn list_skip(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        skip: usize,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        anyhow::Ok(
            self.clone()
                .into_subentries_skip(ctx, blobstore, skip)
                .map_ok(|(path, entry)| (path, bssm_v3_to_mf_entry(entry)))
                .boxed(),
        )
    }

    async fn lookup(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        name: &MPathElement,
    ) -> Result<Option<Entry<Self::TreeId, Self::LeafId>>> {
        Ok(self
            .lookup(ctx, blobstore, name)
            .await?
            .map(bssm_v3_to_mf_entry))
    }

    async fn into_trie_map(
        self,
        _ctx: &CoreContext,
        _blobstore: &Store,
    ) -> Result<Self::TrieMapType> {
        Ok(LoadableShardedMapV2Node::Inlined(self.subentries))
    }
}

impl Manifest for ManifestUnode {
    type TreeId = ManifestUnodeId;
    type LeafId = FileUnodeId;

    fn lookup(&self, name: &MPathElement) -> Option<Entry<Self::TreeId, Self::LeafId>> {
        self.lookup(name).map(convert_unode)
    }

    fn list(&self) -> Box<dyn Iterator<Item = (MPathElement, Entry<Self::TreeId, Self::LeafId>)>> {
        let v: Vec<_> = self
            .list()
            .map(|(basename, entry)| (basename.clone(), convert_unode(entry)))
            .collect();
        Box::new(v.into_iter())
    }
}

fn convert_unode(unode_entry: &UnodeEntry) -> Entry<ManifestUnodeId, FileUnodeId> {
    match unode_entry {
        UnodeEntry::File(file_unode_id) => Entry::Leaf(file_unode_id.clone()),
        UnodeEntry::Directory(mf_unode_id) => Entry::Tree(mf_unode_id.clone()),
    }
}

impl Manifest for Fsnode {
    type TreeId = FsnodeId;
    type LeafId = FsnodeFile;

    fn lookup(&self, name: &MPathElement) -> Option<Entry<Self::TreeId, Self::LeafId>> {
        self.lookup(name).map(convert_fsnode)
    }

    fn list(&self) -> Box<dyn Iterator<Item = (MPathElement, Entry<Self::TreeId, Self::LeafId>)>> {
        let v: Vec<_> = self
            .list()
            .map(|(basename, entry)| (basename.clone(), convert_fsnode(entry)))
            .collect();
        Box::new(v.into_iter())
    }
}

fn convert_fsnode(fsnode_entry: &FsnodeEntry) -> Entry<FsnodeId, FsnodeFile> {
    match fsnode_entry {
        FsnodeEntry::File(fsnode_file) => Entry::Leaf(*fsnode_file),
        FsnodeEntry::Directory(fsnode_directory) => Entry::Tree(fsnode_directory.id().clone()),
    }
}

impl Manifest for SkeletonManifest {
    type TreeId = SkeletonManifestId;
    type LeafId = ();

    fn lookup(&self, name: &MPathElement) -> Option<Entry<Self::TreeId, Self::LeafId>> {
        self.lookup(name).map(convert_skeleton_manifest)
    }

    fn list(&self) -> Box<dyn Iterator<Item = (MPathElement, Entry<Self::TreeId, Self::LeafId>)>> {
        let v: Vec<_> = self
            .list()
            .map(|(basename, entry)| (basename.clone(), convert_skeleton_manifest(entry)))
            .collect();
        Box::new(v.into_iter())
    }
}

fn convert_skeleton_manifest(
    skeleton_entry: &SkeletonManifestEntry,
) -> Entry<SkeletonManifestId, ()> {
    match skeleton_entry {
        SkeletonManifestEntry::File => Entry::Leaf(()),
        SkeletonManifestEntry::Directory(skeleton_directory) => {
            Entry::Tree(skeleton_directory.id().clone())
        }
    }
}

impl Manifest for TestManifest {
    type TreeId = TestManifestDirectory;
    type LeafId = ();

    fn lookup(&self, name: &MPathElement) -> Option<Entry<Self::TreeId, Self::LeafId>> {
        self.lookup(name).map(convert_test_manifest)
    }

    fn list(&self) -> Box<dyn Iterator<Item = (MPathElement, Entry<Self::TreeId, Self::LeafId>)>> {
        let v: Vec<_> = self
            .list()
            .map(|(basename, entry)| (basename.clone(), convert_test_manifest(entry)))
            .collect();
        Box::new(v.into_iter())
    }
}

fn convert_test_manifest(
    test_manifest_entry: &TestManifestEntry,
) -> Entry<TestManifestDirectory, ()> {
    match test_manifest_entry {
        TestManifestEntry::File => Entry::Leaf(()),
        TestManifestEntry::Directory(dir) => Entry::Tree(dir.clone()),
    }
}

#[async_trait]
impl<Store: Blobstore> AsyncManifest<Store> for TestShardedManifest {
    type TreeId = TestShardedManifestDirectory;
    type LeafId = ();
    type TrieMapType = LoadableShardedMapV2Node<TestShardedManifestEntry>;

    async fn list(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        anyhow::Ok(
            self.clone()
                .into_subentries(ctx, blobstore)
                .map_ok(|(path, entry)| (path, convert_test_sharded_manifest(entry)))
                .boxed(),
        )
    }

    async fn list_prefix(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        anyhow::Ok(
            self.clone()
                .into_prefix_subentries(ctx, blobstore, prefix)
                .map_ok(|(path, entry)| (path, convert_test_sharded_manifest(entry)))
                .boxed(),
        )
    }

    async fn list_prefix_after(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        prefix: &[u8],
        after: &[u8],
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        anyhow::Ok(
            self.clone()
                .into_prefix_subentries_after(ctx, blobstore, prefix, after)
                .map_ok(|(path, entry)| (path, convert_test_sharded_manifest(entry)))
                .boxed(),
        )
    }

    async fn list_skip(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        skip: usize,
    ) -> Result<BoxStream<'async_trait, Result<(MPathElement, Entry<Self::TreeId, Self::LeafId>)>>>
    {
        anyhow::Ok(
            self.clone()
                .into_subentries_skip(ctx, blobstore, skip)
                .map_ok(|(path, entry)| (path, convert_test_sharded_manifest(entry)))
                .boxed(),
        )
    }

    async fn lookup(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        name: &MPathElement,
    ) -> Result<Option<Entry<Self::TreeId, Self::LeafId>>> {
        Ok(self
            .lookup(ctx, blobstore, name)
            .await?
            .map(convert_test_sharded_manifest))
    }

    async fn into_trie_map(
        self,
        _ctx: &CoreContext,
        _blobstore: &Store,
    ) -> Result<Self::TrieMapType> {
        Ok(LoadableShardedMapV2Node::Inlined(self.subentries))
    }
}

fn convert_test_sharded_manifest(
    test_sharded_manifest_entry: TestShardedManifestEntry,
) -> Entry<TestShardedManifestDirectory, ()> {
    match test_sharded_manifest_entry {
        TestShardedManifestEntry::File(_file) => Entry::Leaf(()),
        TestShardedManifestEntry::Directory(dir) => Entry::Tree(dir),
    }
}

pub type Weight = usize;

pub trait OrderedManifest: Manifest {
    fn lookup_weighted(
        &self,
        name: &MPathElement,
    ) -> Option<Entry<(Weight, <Self as Manifest>::TreeId), <Self as Manifest>::LeafId>>;
    fn list_weighted(
        &self,
    ) -> Box<
        dyn Iterator<
            Item = (
                MPathElement,
                Entry<(Weight, <Self as Manifest>::TreeId), <Self as Manifest>::LeafId>,
            ),
        >,
    >;
}

#[async_trait]
pub trait AsyncOrderedManifest<Store: Send + Sync>: AsyncManifest<Store> {
    async fn list_weighted(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<
        BoxStream<
            'async_trait,
            Result<(MPathElement, Entry<(Weight, Self::TreeId), Self::LeafId>)>,
        >,
    >;
    async fn lookup_weighted(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        name: &MPathElement,
    ) -> Result<Option<Entry<(Weight, Self::TreeId), Self::LeafId>>>;
}

#[async_trait]
impl<M: OrderedManifest + Send, Store: Send + Sync> AsyncOrderedManifest<Store> for M {
    async fn list_weighted(
        &self,
        _ctx: &CoreContext,
        _blobstore: &Store,
    ) -> Result<
        BoxStream<
            'async_trait,
            Result<(MPathElement, Entry<(Weight, Self::TreeId), Self::LeafId>)>,
        >,
    > {
        Ok(stream::iter(
            OrderedManifest::list_weighted(self)
                .map(anyhow::Ok)
                .collect::<Vec<_>>(),
        )
        .boxed())
    }
    async fn lookup_weighted(
        &self,
        _ctx: &CoreContext,
        _blobstore: &Store,
        name: &MPathElement,
    ) -> Result<Option<Entry<(Weight, Self::TreeId), Self::LeafId>>> {
        anyhow::Ok(OrderedManifest::lookup_weighted(self, name))
    }
}

fn convert_bssm_v3_to_weighted(
    entry: Entry<BssmV3Directory, ()>,
) -> Entry<(Weight, BssmV3Directory), ()> {
    match entry {
        Entry::Tree(dir) => Entry::Tree((
            dir.rollup_count()
                .into_inner()
                .try_into()
                .unwrap_or(usize::MAX),
            dir,
        )),
        Entry::Leaf(()) => Entry::Leaf(()),
    }
}

#[async_trait]
impl<Store: Blobstore> AsyncOrderedManifest<Store> for BssmV3Directory {
    async fn list_weighted(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
    ) -> Result<
        BoxStream<
            'async_trait,
            Result<(MPathElement, Entry<(Weight, Self::TreeId), Self::LeafId>)>,
        >,
    > {
        self.list(ctx, blobstore).await.map(|stream| {
            stream
                .map_ok(|(p, entry)| (p, convert_bssm_v3_to_weighted(entry)))
                .boxed()
        })
    }

    async fn lookup_weighted(
        &self,
        ctx: &CoreContext,
        blobstore: &Store,
        name: &MPathElement,
    ) -> Result<Option<Entry<(Weight, Self::TreeId), Self::LeafId>>> {
        AsyncManifest::lookup(self, ctx, blobstore, name)
            .await
            .map(|opt| opt.map(convert_bssm_v3_to_weighted))
    }
}

impl OrderedManifest for SkeletonManifest {
    fn lookup_weighted(
        &self,
        name: &MPathElement,
    ) -> Option<Entry<(Weight, <Self as Manifest>::TreeId), <Self as Manifest>::LeafId>> {
        self.lookup(name).map(convert_skeleton_manifest_weighted)
    }

    fn list_weighted(
        &self,
    ) -> Box<
        dyn Iterator<
            Item = (
                MPathElement,
                Entry<(Weight, <Self as Manifest>::TreeId), <Self as Manifest>::LeafId>,
            ),
        >,
    > {
        let v: Vec<_> = self
            .list()
            .map(|(basename, entry)| (basename.clone(), convert_skeleton_manifest_weighted(entry)))
            .collect();
        Box::new(v.into_iter())
    }
}

fn convert_skeleton_manifest_weighted(
    skeleton_entry: &SkeletonManifestEntry,
) -> Entry<(Weight, SkeletonManifestId), ()> {
    match skeleton_entry {
        SkeletonManifestEntry::File => Entry::Leaf(()),
        SkeletonManifestEntry::Directory(skeleton_directory) => {
            let summary = skeleton_directory.summary();
            let weight = summary.descendant_files_count + summary.descendant_dirs_count;
            Entry::Tree((weight as Weight, skeleton_directory.id().clone()))
        }
    }
}

impl OrderedManifest for Fsnode {
    fn lookup_weighted(
        &self,
        name: &MPathElement,
    ) -> Option<Entry<(Weight, <Self as Manifest>::TreeId), <Self as Manifest>::LeafId>> {
        self.lookup(name).map(convert_fsnode_weighted)
    }

    fn list_weighted(
        &self,
    ) -> Box<
        dyn Iterator<
            Item = (
                MPathElement,
                Entry<(Weight, <Self as Manifest>::TreeId), <Self as Manifest>::LeafId>,
            ),
        >,
    > {
        let v: Vec<_> = self
            .list()
            .map(|(basename, entry)| (basename.clone(), convert_fsnode_weighted(entry)))
            .collect();
        Box::new(v.into_iter())
    }
}

fn convert_fsnode_weighted(fsnode_entry: &FsnodeEntry) -> Entry<(Weight, FsnodeId), FsnodeFile> {
    match fsnode_entry {
        FsnodeEntry::File(fsnode_file) => Entry::Leaf(*fsnode_file),
        FsnodeEntry::Directory(fsnode_directory) => {
            let summary = fsnode_directory.summary();
            // Fsnodes don't have a full descendant dirs count, so we use the
            // child count as a lower-bound estimate.
            let weight = summary.descendant_files_count + summary.child_dirs_count;
            Entry::Tree((weight as Weight, fsnode_directory.id().clone()))
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Entry<T, L> {
    Tree(T),
    Leaf(L),
}

impl<T, L> Entry<T, L> {
    pub fn into_tree(self) -> Option<T> {
        match self {
            Entry::Tree(tree) => Some(tree),
            _ => None,
        }
    }

    pub fn into_leaf(self) -> Option<L> {
        match self {
            Entry::Leaf(leaf) => Some(leaf),
            _ => None,
        }
    }

    pub fn map_leaf<L2>(self, m: impl FnOnce(L) -> L2) -> Entry<T, L2> {
        match self {
            Entry::Tree(tree) => Entry::Tree(tree),
            Entry::Leaf(leaf) => Entry::Leaf(m(leaf)),
        }
    }

    pub fn map_tree<T2>(self, m: impl FnOnce(T) -> T2) -> Entry<T2, L> {
        match self {
            Entry::Tree(tree) => Entry::Tree(m(tree)),
            Entry::Leaf(leaf) => Entry::Leaf(leaf),
        }
    }

    pub fn left_entry<T2, L2>(self) -> Entry<Either<T, T2>, Either<L, L2>> {
        match self {
            Entry::Tree(tree) => Entry::Tree(Either::Left(tree)),
            Entry::Leaf(leaf) => Entry::Leaf(Either::Left(leaf)),
        }
    }

    pub fn right_entry<T2, L2>(self) -> Entry<Either<T2, T>, Either<L2, L>> {
        match self {
            Entry::Tree(tree) => Entry::Tree(Either::Right(tree)),
            Entry::Leaf(leaf) => Entry::Leaf(Either::Right(leaf)),
        }
    }

    pub fn is_tree(&self) -> bool {
        match self {
            Entry::Tree(_) => true,
            _ => false,
        }
    }
}

#[async_trait]
impl<T, L> Loadable for Entry<T, L>
where
    T: Loadable + Sync,
    L: Loadable + Sync,
{
    type Value = Entry<T::Value, L::Value>;

    async fn load<'a, B: Blobstore>(
        &'a self,
        ctx: &'a CoreContext,
        blobstore: &'a B,
    ) -> Result<Self::Value, LoadableError> {
        Ok(match self {
            Entry::Tree(tree_id) => Entry::Tree(tree_id.load(ctx, blobstore).await?),
            Entry::Leaf(leaf_id) => Entry::Leaf(leaf_id.load(ctx, blobstore).await?),
        })
    }
}

#[async_trait]
impl<T, L> Storable for Entry<T, L>
where
    T: Storable + Send,
    L: Storable + Send,
{
    type Key = Entry<T::Key, L::Key>;

    async fn store<'a, B: Blobstore>(
        self,
        ctx: &'a CoreContext,
        blobstore: &'a B,
    ) -> Result<Self::Key> {
        Ok(match self {
            Entry::Tree(tree) => Entry::Tree(tree.store(ctx, blobstore).await?),
            Entry::Leaf(leaf) => Entry::Leaf(leaf.store(ctx, blobstore).await?),
        })
    }
}

/// Traced allows you to trace a given parent through manifest derivation. For example, if you
/// assign ID 1 to a tree, then perform manifest derivation, then further entries you presented to
/// you that came from this parent will have the same ID.
#[derive(Debug)]
pub struct Traced<I, E>(Option<I>, E);

impl<I, E: Hash> Hash for Traced<I, E> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.1.hash(state);
    }
}

impl<I, E: PartialEq> PartialEq for Traced<I, E> {
    fn eq(&self, other: &Self) -> bool {
        self.1 == other.1
    }
}

impl<I, E: Eq> Eq for Traced<I, E> {}

impl<I: Copy, E: Copy> Copy for Traced<I, E> {}

impl<I: Clone, E: Clone> Clone for Traced<I, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}

impl<I, E> Traced<I, E> {
    pub fn generate(e: E) -> Self {
        Self(None, e)
    }

    pub fn assign(i: I, e: E) -> Self {
        Self(Some(i), e)
    }

    pub fn id(&self) -> Option<&I> {
        self.0.as_ref()
    }

    pub fn untraced(&self) -> &E {
        &self.1
    }

    pub fn into_untraced(self) -> E {
        self.1
    }
}

impl<I: Copy, E> Traced<I, E> {
    fn inherit_into_entry<TreeId, LeafId>(
        &self,
        e: Entry<TreeId, LeafId>,
    ) -> Entry<Traced<I, TreeId>, Traced<I, LeafId>> {
        match e {
            Entry::Tree(t) => Entry::Tree(Traced(self.0, t)),
            Entry::Leaf(l) => Entry::Leaf(Traced(self.0, l)),
        }
    }
}

impl<I, TreeId, LeafId> From<Entry<Traced<I, TreeId>, Traced<I, LeafId>>>
    for Entry<TreeId, LeafId>
{
    fn from(entry: Entry<Traced<I, TreeId>, Traced<I, LeafId>>) -> Self {
        match entry {
            Entry::Tree(Traced(_, t)) => Entry::Tree(t),
            Entry::Leaf(Traced(_, l)) => Entry::Leaf(l),
        }
    }
}

impl<I: Send + Sync + Copy + 'static, M: Manifest> Manifest for Traced<I, M> {
    type TreeId = Traced<I, <M as Manifest>::TreeId>;
    type LeafId = Traced<I, <M as Manifest>::LeafId>;

    fn list(&self) -> Box<dyn Iterator<Item = (MPathElement, Entry<Self::TreeId, Self::LeafId>)>> {
        Box::new(
            self.1
                .list()
                .map(|(path, entry)| (path, self.inherit_into_entry(entry)))
                .collect::<Vec<_>>()
                .into_iter(),
        )
    }

    fn lookup(&self, name: &MPathElement) -> Option<Entry<Self::TreeId, Self::LeafId>> {
        self.1.lookup(name).map(|e| self.inherit_into_entry(e))
    }
}

#[async_trait]
impl<I: Clone + 'static + Send + Sync, M: Loadable + Send + Sync> Loadable for Traced<I, M> {
    type Value = Traced<I, <M as Loadable>::Value>;

    async fn load<'a, B: Blobstore>(
        &'a self,
        ctx: &'a CoreContext,
        blobstore: &'a B,
    ) -> Result<Self::Value, LoadableError> {
        let id = self.0.clone();
        let v = self.1.load(ctx, blobstore).await?;
        Ok(Traced(id, v))
    }
}
