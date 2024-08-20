/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! DAG and Id operations (mostly traits)

use std::sync::Arc;

use futures::StreamExt;
use futures::TryStreamExt;

use crate::clone::CloneData;
use crate::dag::MemDag;
use crate::default_impl;
use crate::errors::NotFoundError;
use crate::id::Group;
use crate::id::Id;
use crate::id::Vertex;
pub use crate::iddag::IdDagAlgorithm;
use crate::set::id_lazy::IdLazySet;
use crate::set::id_static::IdStaticSet;
use crate::set::Set;
use crate::IdList;
use crate::IdSet;
use crate::Result;
use crate::VerLink;
use crate::VertexListWithOptions;

/// DAG related read-only algorithms.
#[async_trait::async_trait]
pub trait DagAlgorithm: Send + Sync {
    /// Sort a `Set` topologically in descending order.
    ///
    /// The returned set should have `dag` and `id_map` hints set to associated
    /// with this dag or its previous compatible version. For example, if a
    /// `set` is sorted on another dag but not in this dag, it should be resorted
    /// using this dag.  If a `set` is empty and not associated to the current
    /// `dag` in its hints, the return value should be a different empty `set`
    /// that has the `dag` and `id_map` hints set to this dag.
    async fn sort(&self, set: &Set) -> Result<Set>;

    /// Re-create the graph so it looks better when rendered.
    async fn beautify(&self, main_branch: Option<Set>) -> Result<MemDag> {
        default_impl::beautify(self, main_branch).await
    }

    /// Extract a sub graph containing only specified vertexes.
    async fn subdag(&self, set: Set) -> Result<MemDag> {
        default_impl::subdag(self, set).await
    }

    /// Get ordered parent vertexes.
    async fn parent_names(&self, name: Vertex) -> Result<Vec<Vertex>>;

    /// Returns a set that covers all vertexes tracked by this DAG.
    ///
    /// Does not include VIRTUAL vertexes.
    async fn all(&self) -> Result<Set>;

    /// Returns a set that covers all vertexes in the master group.
    async fn master_group(&self) -> Result<Set>;

    /// Returns a set that covers all vertexes in the virtual group.
    async fn virtual_group(&self) -> Result<Set>;

    /// Calculates all ancestors reachable from any name from the given set.
    async fn ancestors(&self, set: Set) -> Result<Set>;

    /// Calculates parents of the given set.
    ///
    /// Note: Parent order is not preserved. Use [`Dag::parent_names`]
    /// to preserve order.
    async fn parents(&self, set: Set) -> Result<Set> {
        default_impl::parents(self, set).await
    }

    /// Calculates the n-th first ancestor.
    async fn first_ancestor_nth(&self, name: Vertex, n: u64) -> Result<Option<Vertex>> {
        default_impl::first_ancestor_nth(self, name, n).await
    }

    /// Calculates ancestors but only follows the first parent.
    async fn first_ancestors(&self, set: Set) -> Result<Set> {
        default_impl::first_ancestors(self, set).await
    }

    /// Calculates heads of the given set.
    async fn heads(&self, set: Set) -> Result<Set> {
        default_impl::heads(self, set).await
    }

    /// Calculates children of the given set.
    async fn children(&self, set: Set) -> Result<Set>;

    /// Calculates roots of the given set.
    async fn roots(&self, set: Set) -> Result<Set> {
        default_impl::roots(self, set).await
    }

    /// Calculates merges of the selected set (vertexes with >=2 parents).
    async fn merges(&self, set: Set) -> Result<Set> {
        default_impl::merges(self, set).await
    }

    /// Calculates one "greatest common ancestor" of the given set.
    ///
    /// If there are no common ancestors, return None.
    /// If there are multiple greatest common ancestors, pick one arbitrarily.
    /// Use `gca_all` to get all of them.
    async fn gca_one(&self, set: Set) -> Result<Option<Vertex>> {
        default_impl::gca_one(self, set).await
    }

    /// Calculates all "greatest common ancestor"s of the given set.
    /// `gca_one` is faster if an arbitrary answer is ok.
    async fn gca_all(&self, set: Set) -> Result<Set> {
        default_impl::gca_all(self, set).await
    }

    /// Calculates all common ancestors of the given set.
    async fn common_ancestors(&self, set: Set) -> Result<Set> {
        default_impl::common_ancestors(self, set).await
    }

    /// Tests if `ancestor` is an ancestor of `descendant`.
    async fn is_ancestor(&self, ancestor: Vertex, descendant: Vertex) -> Result<bool> {
        default_impl::is_ancestor(self, ancestor, descendant).await
    }

    /// Calculates "heads" of the ancestors of the given set. That is,
    /// Find Y, which is the smallest subset of set X, where `ancestors(Y)` is
    /// `ancestors(X)`.
    ///
    /// This is faster than calculating `heads(ancestors(set))` in certain
    /// implementations like segmented changelog.
    ///
    /// This is different from `heads`. In case set contains X and Y, and Y is
    /// an ancestor of X, but not the immediate ancestor, `heads` will include
    /// Y while this function won't.
    async fn heads_ancestors(&self, set: Set) -> Result<Set> {
        default_impl::heads_ancestors(self, set).await
    }

    /// Calculates the "dag range" - vertexes reachable from both sides.
    async fn range(&self, roots: Set, heads: Set) -> Result<Set>;

    /// Calculates `ancestors(reachable) - ancestors(unreachable)`.
    async fn only(&self, reachable: Set, unreachable: Set) -> Result<Set> {
        default_impl::only(self, reachable, unreachable).await
    }

    /// Calculates `ancestors(reachable) - ancestors(unreachable)`, and
    /// `ancestors(unreachable)`.
    /// This might be faster in some implementations than calculating `only` and
    /// `ancestors` separately.
    async fn only_both(&self, reachable: Set, unreachable: Set) -> Result<(Set, Set)> {
        default_impl::only_both(self, reachable, unreachable).await
    }

    /// Calculates the descendants of the given set.
    async fn descendants(&self, set: Set) -> Result<Set>;

    /// Calculates `roots` that are reachable from `heads` without going
    /// through other `roots`. For example, given the following graph:
    ///
    /// ```plain,ignore
    ///   F
    ///   |\
    ///   C E
    ///   | |
    ///   B D
    ///   |/
    ///   A
    /// ```
    ///
    /// `reachable_roots(roots=[A, B, C], heads=[F])` returns `[A, C]`.
    /// `B` is not included because it cannot be reached without going
    /// through another root `C` from `F`. `A` is included because it
    /// can be reached via `F -> E -> D -> A` that does not go through
    /// other roots.
    ///
    /// The can be calculated as
    /// `roots & (heads | parents(only(heads, roots & ancestors(heads))))`.
    /// Actual implementation might have faster paths.
    ///
    /// The `roots & ancestors(heads)` portion filters out bogus roots for
    /// compatibility, if the callsite does not provide bogus roots, it
    /// could be simplified to just `roots`.
    async fn reachable_roots(&self, roots: Set, heads: Set) -> Result<Set> {
        default_impl::reachable_roots(self, roots, heads).await
    }

    /// Suggest the next place to test during a bisect.
    ///
    /// - `(roots, heads)` are either `(good, bad)` or `(bad, good)`.
    /// - `skip` should be non-lazy.
    ///
    /// Return `(vertex_to_bisect_next, untested_set, roots(high::))`.
    ///
    /// If `vertex_to_bisect_next` is `None`, the bisect is completed. At this
    /// time, `roots(heads::)` is the "first good/bad" set. `untested_set`
    /// is usually empty, or a subset of `skip`.
    async fn suggest_bisect(
        &self,
        roots: Set,
        heads: Set,
        skip: Set,
    ) -> Result<(Option<Vertex>, Set, Set)>;

    /// Vertexes buffered in memory, not yet written to disk.
    ///
    /// Does not include VIRTUAL vertexes.
    async fn dirty(&self) -> Result<Set>;

    /// Returns true if the vertex names might need to be resolved remotely.
    fn is_vertex_lazy(&self) -> bool;

    /// Get a snapshot of the current graph that can operate separately.
    ///
    /// This makes it easier to fight with borrowck.
    fn dag_snapshot(&self) -> Result<Arc<dyn DagAlgorithm + Send + Sync>>;

    /// Get a snapshot of the `IdDag` that can operate separately.
    ///
    /// This is for advanced use-cases. For example, if callsite wants to
    /// do some graph calculation without network, and control how to
    /// batch the vertex name lookups precisely.
    fn id_dag_snapshot(&self) -> Result<Arc<dyn IdDagAlgorithm + Send + Sync>> {
        Err(crate::errors::BackendError::Generic(format!(
            "id_dag_snapshot() is not supported for {}",
            std::any::type_name::<Self>()
        ))
        .into())
    }

    /// Identity of the dag.
    fn dag_id(&self) -> &str;

    /// Version of the dag. Useful to figure out compatibility between two dags.
    ///
    /// For performance, this does not include changes to the VIRTUAL group.
    fn dag_version(&self) -> &VerLink;
}

#[async_trait::async_trait]
pub trait Parents: Send + Sync {
    async fn parent_names(&self, name: Vertex) -> Result<Vec<Vertex>>;

    /// A hint of a sub-graph for inserting `heads`.
    ///
    /// This is used to reduce remote fetches in a lazy graph. The function
    /// should ideally return a subset of pending vertexes that are confirmed to
    /// not overlap in the existing (potentially lazy) graph.
    ///
    /// The pending roots will be checked first, if a root is unknown locally
    /// then all its descendants will be considered unknown locally.
    ///
    /// The returned graph is only used to optimize network fetches in
    /// `assign_head`. It is not used to be actually inserted to the graph. So
    /// returning an empty or "incorrect" graph does not hurt correctness. But
    /// might hurt performance. Returning a set that contains vertexes that do
    /// overlap in the existing graph is incorrect.
    async fn hint_subdag_for_insertion(&self, _heads: &[Vertex]) -> Result<MemDag>;
}

#[async_trait::async_trait]
impl Parents for Arc<dyn DagAlgorithm + Send + Sync> {
    async fn parent_names(&self, name: Vertex) -> Result<Vec<Vertex>> {
        DagAlgorithm::parent_names(self, name).await
    }

    async fn hint_subdag_for_insertion(&self, heads: &[Vertex]) -> Result<MemDag> {
        let scope = self.dirty().await?;
        default_impl::hint_subdag_for_insertion(self, &scope, heads).await
    }
}

#[async_trait::async_trait]
impl Parents for &(dyn DagAlgorithm + Send + Sync) {
    async fn parent_names(&self, name: Vertex) -> Result<Vec<Vertex>> {
        DagAlgorithm::parent_names(*self, name).await
    }

    async fn hint_subdag_for_insertion(&self, heads: &[Vertex]) -> Result<MemDag> {
        let scope = self.dirty().await?;
        default_impl::hint_subdag_for_insertion(self, &scope, heads).await
    }
}

#[async_trait::async_trait]
impl<'a> Parents for Box<dyn Fn(Vertex) -> Result<Vec<Vertex>> + Send + Sync + 'a> {
    async fn parent_names(&self, name: Vertex) -> Result<Vec<Vertex>> {
        (self)(name)
    }

    async fn hint_subdag_for_insertion(&self, _heads: &[Vertex]) -> Result<MemDag> {
        // No clear way to detect the "dirty" scope.
        Ok(MemDag::new())
    }
}

#[async_trait::async_trait]
impl Parents for std::collections::HashMap<Vertex, Vec<Vertex>> {
    async fn parent_names(&self, name: Vertex) -> Result<Vec<Vertex>> {
        match self.get(&name) {
            Some(v) => Ok(v.clone()),
            None => name.not_found(),
        }
    }

    async fn hint_subdag_for_insertion(&self, heads: &[Vertex]) -> Result<MemDag> {
        let mut keys: Vec<Vertex> = self.keys().cloned().collect();
        keys.sort_unstable();
        let scope = Set::from_static_names(keys);
        default_impl::hint_subdag_for_insertion(self, &scope, heads).await
    }
}

/// Add vertexes recursively to the DAG.
#[async_trait::async_trait]
pub trait DagAddHeads {
    /// Add non-lazy vertexes and their ancestors in memory.
    ///
    /// Does not persist changes to disk. Use `add_heads_and_flush` to persist.
    /// Use `import_pull_data` to add lazy segments to the DAG.
    ///
    /// `heads` must use non-MASTER (NON_MASTER, VIRTUAL) groups as
    /// `desired_group`. `heads` are imported in the given order.
    ///
    /// | Method              | Allowed groups          | Persist | Lazy |
    /// |---------------------|-------------------------|---------|------|
    /// | add_heads           | NON_MASTER, VIRTUAL [1] | No      | No   |
    /// | add_heads_and_flush | MASTER                  | Yes     | No   |
    /// | import_pull_data    | MASTER                  | Yes     | Yes  |
    ///
    /// [1]: Changes to the VIRTUAL group may not survive reloading. Use
    /// `set_managed_virtual_group` to "pin" content in VIRTUAL that survives
    /// reloads.
    async fn add_heads(
        &mut self,
        parents: &dyn Parents,
        heads: &VertexListWithOptions,
    ) -> Result<bool>;
}

/// Remove vertexes and their descendants from the DAG.
#[async_trait::async_trait]
pub trait DagStrip {
    /// Remove the given `set` and their descendants on disk.
    ///
    /// Reload and persist changes to disk (with lock) immediately.
    /// Errors out if pending changes in NON_MASTER were added by `add_heads`.
    ///
    /// After strip, the `self` graph might contain new vertexes because of
    /// the reload.
    async fn strip(&mut self, set: &Set) -> Result<()>;
}

/// Import a generated `CloneData` object into an empty DAG.
#[async_trait::async_trait]
pub trait DagImportCloneData {
    /// Updates the DAG using a `CloneData` object.
    ///
    /// This predates `import_pull_data`. New logic should use the general
    /// purpose `import_pull_data` instead. Clone is just a special case of
    /// pull.
    async fn import_clone_data(&mut self, clone_data: CloneData<Vertex>) -> Result<()>;
}

/// Import a generated incremental `CloneData` object into an existing DAG.
/// Ids in the passed CloneData might not match ids in existing DAG.
#[async_trait::async_trait]
pub trait DagImportPullData {
    /// Imports lazy segments ("name" partially known, "shape" known) on disk.
    ///
    /// Reload and persist changes to disk (with lock) immediately.
    /// Errors out if pending changes in NON_MASTER were added by `add_heads`.
    /// Errors out if `clone_data` overlaps with the existing graph.
    ///
    /// `heads` must use MASTER as `desired_group`. `heads` are imported
    /// in the given order (useful to distinguish between primary and secondary
    /// branches, and specify their gaps in the id ranges, to reduce
    /// fragmentation).
    ///
    /// `heads` with `reserve_size > 0` must be passed in even if they
    /// already exist and are not being added, for the id reservation to work
    /// correctly.
    ///
    /// If `clone_data` includes parts not covered by `heads` and their
    /// ancestors, those parts will be ignored.
    async fn import_pull_data(
        &mut self,
        clone_data: CloneData<Vertex>,
        heads: &VertexListWithOptions,
    ) -> Result<()>;
}

#[async_trait::async_trait]
pub trait DagExportCloneData {
    /// Export `CloneData` for vertexes in the master group.
    async fn export_clone_data(&self) -> Result<CloneData<Vertex>>;
}

#[async_trait::async_trait]
pub trait DagExportPullData {
    /// Export `CloneData` for vertexes in the given set.
    /// The set is typically calculated by `only(heads, common)`.
    async fn export_pull_data(&self, set: &Set) -> Result<CloneData<Vertex>>;
}

/// Persistent the DAG on disk.
#[async_trait::async_trait]
pub trait DagPersistent {
    /// Write in-memory DAG to disk. This might also pick up changes to
    /// the DAG by other processes.
    ///
    /// Calling `add_heads` followed by `flush` is like calling
    /// `add_heads_and_flush` with the `master_heads` passed to `flush` concated
    /// with `heads` from `add_heads`. `add_heads` followed by `flush` is more
    /// flexible but less performant than `add_heads_and_flush`.
    async fn flush(&mut self, master_heads: &VertexListWithOptions) -> Result<()>;

    /// Write in-memory IdMap that caches Id <-> Vertex translation from
    /// remote service to disk.
    async fn flush_cached_idmap(&self) -> Result<()>;

    /// Add non-lazy vertexes, their ancestors, and vertexes added previously by
    /// `add_heads` on disk.
    ///
    /// Reload and persist changes to disk (with lock) immediately.
    /// Does not error out if pending changes were added by `add_heads`.
    ///
    /// `heads` should not use `VIRTUAL` as `desired_group`. `heads` are
    /// imported in the given order, followed by `heads` previously added
    /// by `add_heads`.
    ///
    /// `heads` with `reserve_size > 0` must be passed in even if they
    /// already exist and are not being added, for the id reservation to work
    /// correctly.
    ///
    /// `add_heads_and_flush` is faster than `add_heads`. But `add_heads` can
    /// be useful for the VIRTUAL group, and when the final group is not yet
    /// decided (ex. the MASTER group is decided by remotenames info but
    /// remotenames is not yet known at `add_heads` time).
    async fn add_heads_and_flush(
        &mut self,
        parent_names_func: &dyn Parents,
        heads: &VertexListWithOptions,
    ) -> Result<()>;

    /// Import from another (potentially large) DAG. Write to disk immediately.
    async fn import_and_flush(&mut self, dag: &dyn DagAlgorithm, master_heads: Set) -> Result<()> {
        let heads = dag.heads(dag.all().await?).await?;
        let non_master_heads = heads - master_heads.clone();
        let master_heads: Vec<Vertex> = master_heads.iter().await?.try_collect::<Vec<_>>().await?;
        let non_master_heads: Vec<Vertex> = non_master_heads
            .iter()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        let heads = VertexListWithOptions::from(master_heads)
            .with_desired_group(Group::MASTER)
            .chain(non_master_heads);
        self.add_heads_and_flush(&dag.dag_snapshot()?, &heads).await
    }
}

/// Import ASCII graph to DAG.
pub trait ImportAscii {
    /// Import vertexes described in an ASCII graph.
    fn import_ascii(&mut self, text: &str) -> Result<()> {
        self.import_ascii_with_heads_and_vertex_fn(text, <Option<&[&str]>>::None, None)
    }

    /// Import vertexes described in an ASCII graph.
    /// `heads` optionally specifies the order of heads to insert.
    /// Useful for testing. Panic if the input is invalid.
    fn import_ascii_with_heads(
        &mut self,
        text: &str,
        heads: Option<&[impl AsRef<str>]>,
    ) -> Result<()> {
        self.import_ascii_with_heads_and_vertex_fn(text, heads, None)
    }

    /// Import vertexes described in an ASCII graph.
    /// `vertex_fn` specifies how to generate Vertex from a str from the ASCII graph.
    /// Useful for testing when we need to generate `HgId` (fixed length) from vertex.
    fn import_ascii_with_vertex_fn(
        &mut self,
        text: &str,
        vertex_fn: fn(&str) -> Vertex,
    ) -> Result<()> {
        self.import_ascii_with_heads_and_vertex_fn(text, <Option<&[&str]>>::None, Some(vertex_fn))
    }

    /// Import vertexes described in an ASCII graph.
    /// `heads` optionally specifies the order of heads to insert.
    /// `vertex_fn` specifies how to generate Vertex from a str from the ASCII graph.
    ///
    /// This method is a helper function of other APIs, choose other APIs if possible.
    fn import_ascii_with_heads_and_vertex_fn(
        &mut self,
        text: &str,
        heads: Option<&[impl AsRef<str>]>,
        vertex_fn: Option<fn(&str) -> Vertex>,
    ) -> Result<()>;
}

/// Lookup vertexes by prefixes.
#[async_trait::async_trait]
pub trait PrefixLookup {
    /// Lookup vertexes by hex prefix.
    async fn vertexes_by_hex_prefix(&self, hex_prefix: &[u8], limit: usize) -> Result<Vec<Vertex>>;
}

/// Convert between `Vertex` and `Id`.
#[async_trait::async_trait]
pub trait IdConvert: PrefixLookup + Sync {
    async fn vertex_id(&self, name: Vertex) -> Result<Id>;
    async fn vertex_id_with_max_group(&self, name: &Vertex, max_group: Group)
    -> Result<Option<Id>>;
    async fn vertex_name(&self, id: Id) -> Result<Vertex>;
    async fn contains_vertex_name(&self, name: &Vertex) -> Result<bool>;

    /// Test if an `id` is present locally. Do not trigger remote fetching.
    async fn contains_vertex_id_locally(&self, id: &[Id]) -> Result<Vec<bool>>;

    /// Test if an `name` is present locally. Do not trigger remote fetching.
    async fn contains_vertex_name_locally(&self, name: &[Vertex]) -> Result<Vec<bool>>;

    async fn vertex_id_optional(&self, name: &Vertex) -> Result<Option<Id>> {
        self.vertex_id_with_max_group(name, Group::MAX).await
    }

    /// Convert [`Id`]s to [`Vertex`]s in batch.
    async fn vertex_name_batch(&self, ids: &[Id]) -> Result<Vec<Result<Vertex>>> {
        // This is not an efficient implementation in an async context.
        let mut names = Vec::with_capacity(ids.len());
        for &id in ids {
            names.push(self.vertex_name(id).await);
        }
        Ok(names)
    }

    /// Convert [`Vertex`]s to [`Id`]s in batch.
    async fn vertex_id_batch(&self, names: &[Vertex]) -> Result<Vec<Result<Id>>> {
        // This is not an efficient implementation in an async context.
        let mut ids = Vec::with_capacity(names.len());
        for name in names {
            ids.push(self.vertex_id(name.clone()).await);
        }
        Ok(ids)
    }

    /// Identity of the map.
    fn map_id(&self) -> &str;

    /// Version of the map. Useful to figure out compatibility between two maps.
    ///
    /// For performance, this does not include changes to the VIRTUAL group.
    fn map_version(&self) -> &VerLink;
}

/// Integrity check functions.
#[async_trait::async_trait]
pub trait CheckIntegrity {
    /// Verify that universally known `Id`s (parents of merges) are actually
    /// known locally.
    ///
    /// Returns set of `Id`s that should be universally known but missing.
    /// An empty set means all universally known `Id`s are known locally.
    ///
    /// Check `FirstAncestorConstraint::KnownUniversally` for concepts of
    /// "universally known".
    async fn check_universal_ids(&self) -> Result<Vec<Id>>;

    /// Check segment properties: no cycles, no overlaps, no gaps etc.
    /// This is only about the `Id`s, not about the vertex names.
    ///
    /// Returns human readable messages about problems.
    /// No messages indicates there are no problems detected.
    async fn check_segments(&self) -> Result<Vec<String>>;

    /// Check that the subset of the current graph (ancestors of `heads`)
    /// is isomorphic with the subset in the `other` graph.
    ///
    /// Returns messages about where two graphs are different.
    /// No messages indicates two graphs are likely isomorphic.
    ///
    /// Note: For performance, this function only checks the "shape"
    /// of the graph, without checking the (potentially lazy) vertex
    /// names.
    async fn check_isomorphic_graph(
        &self,
        other: &dyn DagAlgorithm,
        heads: Set,
    ) -> Result<Vec<String>>;
}

impl<T> ImportAscii for T
where
    T: DagAddHeads,
{
    fn import_ascii_with_heads_and_vertex_fn(
        &mut self,
        text: &str,
        heads: Option<&[impl AsRef<str>]>,
        vertex_fn: Option<fn(&str) -> Vertex>,
    ) -> Result<()> {
        let vertex_fn = match vertex_fn {
            Some(vertex_fn) => vertex_fn,
            None => |s: &str| Vertex::copy_from(s.as_bytes()),
        };
        let parents = drawdag::parse(text);
        let heads: Vec<_> = match heads {
            Some(heads) => heads.iter().map(|s| vertex_fn(s.as_ref())).collect(),
            None => {
                let mut heads: Vec<_> = parents.keys().map(|s| vertex_fn(s)).collect();
                heads.sort();
                heads
            }
        };

        let parents: std::collections::HashMap<Vertex, Vec<Vertex>> = parents
            .iter()
            .map(|(k, vs)| (vertex_fn(k), vs.iter().map(|v| vertex_fn(v)).collect()))
            .collect();
        nonblocking::non_blocking_result(self.add_heads(&parents, &heads[..].into()))?;
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait ToIdSet {
    /// Converts [`Set`] to [`IdSet`].
    async fn to_id_set(&self, set: &Set) -> Result<IdSet>;
}

pub trait ToSet {
    /// Converts [`IdSet`] to [`Set`].
    fn to_set(&self, set: &IdSet) -> Result<Set>;

    /// Converts [`IdList`] to [`Set`].
    fn id_list_to_set(&self, list: &IdList) -> Result<Set>;
}

pub trait IdMapSnapshot {
    /// Get a snapshot of IdMap.
    fn id_map_snapshot(&self) -> Result<Arc<dyn IdConvert + Send + Sync>>;
}

/// Describes how to persist state to disk.
pub trait Persist {
    /// Return type of `lock()`.
    type Lock: Send + Sync;

    /// Obtain an exclusive lock for writing.
    /// This should prevent other writers.
    fn lock(&mut self) -> Result<Self::Lock>;

    /// Reload from the source of truth. Drop pending changes.
    ///
    /// This requires a lock and is usually called before `persist()`.
    fn reload(&mut self, _lock: &Self::Lock) -> Result<()>;

    /// Write pending changes to the source of truth.
    ///
    /// This requires a lock.
    fn persist(&mut self, _lock: &Self::Lock) -> Result<()>;
}

/// Address that can be used to open things.
///
/// The address type decides the return type of `open`.
pub trait Open: Clone {
    type OpenTarget;

    fn open(&self) -> Result<Self::OpenTarget>;
}

/// Has a tuple version that can be used to test if the data was changed.
pub trait StorageVersion {
    /// Version tracked by the underlying low-level storage (ex. indexedlog).
    /// `(epoch, length)`.
    /// - If `epoch` is changed, then a non-append-only change has happened,
    ///   all caches should be invalidated.
    /// - If `length` is increased but `epoch` has changed, then the storage
    ///   layer got an append-only change. Note: the append-only change at
    ///   the storage layer does *not* mean append-only at the commit graph
    ///   layer, since the strip operation that removes commits could be
    ///   implemented by appending special data to the storage layer.
    fn storage_version(&self) -> (u64, u64);
}

/// Fallible clone.
pub trait TryClone {
    fn try_clone(&self) -> Result<Self>
    where
        Self: Sized;
}

impl<T: Clone> TryClone for T {
    fn try_clone(&self) -> Result<Self> {
        Ok(self.clone())
    }
}

#[async_trait::async_trait]
impl<T: IdConvert + IdMapSnapshot> ToIdSet for T {
    /// Converts [`Set`] to [`IdSet`], which no longer preserves iteration order.
    async fn to_id_set(&self, set: &Set) -> Result<IdSet> {
        let version = set.hints().id_map_version();

        // Fast path: extract IdSet from IdStaticSet.
        if let Some(set) = set.as_any().downcast_ref::<IdStaticSet>() {
            if None < version && version <= Some(self.map_version()) {
                tracing::debug!(target: "dag::algo::to_id_set", "{:6?} (fast path)", set);
                return Ok(set.id_set_losing_order().clone());
            }
        }

        // Fast path: flatten to IdStaticSet. This works for UnionSet(...) cases.
        if let Some(set) = set.specialized_flatten_id() {
            tracing::debug!(target: "dag::algo::to_id_set", "{:6?} (fast path 2)", set);
            return Ok(set.id_set_losing_order().clone());
        }

        // Convert IdLazySet to IdStaticSet. Bypass hash lookups.
        if let Some(set) = set.as_any().downcast_ref::<IdLazySet>() {
            if None < version && version <= Some(self.map_version()) {
                tracing::warn!(target: "dag::algo::to_id_set", "{:6?} (slow path 1)", set);
                let set: IdStaticSet = set.to_static()?;
                return Ok(set.id_set_losing_order().clone());
            }
        }

        // Slow path: iterate through the set and convert it to a non-lazy
        // IdSet. Does not bypass hash lookups.
        let mut spans = IdSet::empty();
        let mut iter = set.iter().await?.chunks(1 << 17);
        tracing::warn!(target: "dag::algo::to_id_set", "{:6?} (slow path 2)", set);
        while let Some(names) = iter.next().await {
            let names = names.into_iter().collect::<Result<Vec<_>>>()?;
            let ids = self.vertex_id_batch(&names).await?;
            for id in ids {
                spans.push(id?);
            }
        }
        Ok(spans)
    }
}

impl IdMapSnapshot for Arc<dyn IdConvert + Send + Sync> {
    fn id_map_snapshot(&self) -> Result<Arc<dyn IdConvert + Send + Sync>> {
        Ok(self.clone())
    }
}

impl<T: IdMapSnapshot + DagAlgorithm> ToSet for T {
    /// Converts [`IdSet`] to [`Set`].
    fn to_set(&self, set: &IdSet) -> Result<Set> {
        Set::from_id_set_dag(set.clone(), self)
    }

    fn id_list_to_set(&self, list: &IdList) -> Result<Set> {
        Set::from_id_list_dag(list.clone(), self)
    }
}
