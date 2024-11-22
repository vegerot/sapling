/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Range;

use dag::ops::DagAlgorithm;
use dag::ops::DagPersistent;
use dag::ops::IdConvert;
use dag::set::SyncSetQuery;
use dag::Dag;
use dag::Group;
use dag::Id;
use dag::IdSet;
use dag::OnDiskIdDag;
use dag::Vertex;
use dag::VertexListWithOptions;
use nonblocking::non_blocking_result;
use tempfile::TempDir;

use crate::parse_bindag;
use crate::ParentRevs;

/// Context for testing purpose.
/// Contains the parsed bindag and Dag from the dag crate.
pub struct GeneralTestContext<T> {
    /// Plain DAG parsed from bindag
    pub parents: Vec<T>,

    /// Complex DAG, with ids re-assigned
    pub dag: Dag,

    /// Simple IdMap. Dag Id -> Plain DAG id
    pub idmap: HashMap<Id, usize>,

    /// Plain DAG id -> Dag Id.
    pub rev_idmap: Vec<Id>,

    /// Temporary dir containing the Dag daga.
    pub dir: TempDir,
}

pub type TestContext = GeneralTestContext<ParentRevs>;
pub type OctopusTestContext = GeneralTestContext<Vec<usize>>;

impl TestContext {
    pub fn from_bin(bin: &[u8]) -> Self {
        Self::from_bin_sliced(bin, 0..usize::max_value())
    }

    pub fn from_bin_sliced(bin: &[u8], range: Range<usize>) -> Self {
        // Prepare the plain DAG (parents)
        let parents = parse_bindag(bin);
        let parents = crate::slice_parents(parents, range);
        Self::from_parents(parents)
    }
}

impl<T: AsRef<[usize]> + Send + Sync + Clone + 'static> GeneralTestContext<T> {
    pub fn from_parents(parents: Vec<T>) -> Self {
        // Prepare Dag
        let parents_by_name = {
            let parents = parents.clone();
            move |name: Vertex| -> dag::Result<Vec<Vertex>> {
                let i = String::from_utf8(name.as_ref().to_vec())
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                Ok(parents[i]
                    .as_ref()
                    .iter()
                    .map(|p| format!("{}", p).as_bytes().to_vec().into())
                    .collect())
            }
        };
        // Pick heads from 0..n
        let get_heads = |n: usize| -> Vec<Vertex> {
            let mut heads: HashSet<usize> = (0..n).collect();
            for ps in parents.iter().take(n) {
                for p in ps.as_ref().iter() {
                    heads.remove(p);
                }
            }
            let mut names: Vec<Vertex> = Vec::new();
            for h in heads {
                names.push(Vertex::copy_from(format!("{}", h).as_bytes()));
            }
            names
        };
        let head_names: Vec<Vertex> = get_heads(parents.len());
        let master_names: Vec<Vertex> = get_heads(parents.len() / 2);

        let dir = tempfile::tempdir().unwrap();
        let mut dag = Dag::open(dir.path()).unwrap();
        let parents_map: Box<dyn Fn(Vertex) -> dag::Result<Vec<Vertex>> + Send + Sync> =
            Box::new(parents_by_name);
        let heads = VertexListWithOptions::from(master_names)
            .with_desired_group(Group::MASTER)
            .chain(head_names);
        non_blocking_result(dag.add_heads_and_flush(&parents_map, &heads)).unwrap();

        // Prepare idmap
        let idmap: HashMap<Id, usize> = non_blocking_result(dag.all())
            .unwrap()
            .iter()
            .unwrap()
            .map(|name| {
                let name = name.unwrap();
                let dag_id: Id = non_blocking_result(dag.vertex_id(name.clone())).unwrap();
                let plain_id: usize = std::str::from_utf8(name.as_ref()).unwrap().parse().unwrap();
                (dag_id, plain_id)
            })
            .collect();
        let mut rev_idmap = vec![Id(0); idmap.len()];
        for (&k, &v) in idmap.iter() {
            rev_idmap[v] = k;
        }
        assert_eq!(rev_idmap.len(), parents.len());

        Self {
            parents,
            dag,
            idmap,
            rev_idmap,
            dir,
        }
    }
}

impl<T> GeneralTestContext<T> {
    /// Limit the size of `parents`.
    pub fn truncate(mut self, size: usize) -> Self {
        if size < self.parents.len() {
            self.parents.truncate(size);
        }
        self
    }

    /// Convert a IdSet (used by IdDag) to plain revs (used by `parents`).
    pub fn to_plain_revs(&self, set: &IdSet) -> Vec<usize> {
        set.iter_desc().map(|i| self.idmap[&i]).collect()
    }

    /// Get the IdDag reference.
    pub fn id_dag(&self) -> &OnDiskIdDag {
        self.dag.dag()
    }

    /// Make `revs` in the range of parents.
    pub fn clamp_revs(&self, revs: &[impl Into<usize> + Clone]) -> Vec<usize> {
        revs.iter()
            .cloned()
            .map(|i| Into::<usize>::into(i) % self.parents.len())
            .collect()
    }

    /// Convert `usize` plain revs to IdSet used by IdDag.
    pub fn to_dag_revs(&self, revs: &[usize]) -> IdSet {
        let iter = revs.iter().map(|&i| self.rev_idmap[i].clone());
        IdSet::from_spans(iter)
    }
}
