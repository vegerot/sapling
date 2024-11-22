/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dag::idmap::IdMap;
use dag::idmap::IdMapAssignHead;
use dag::Group;
use dag::IdDag;
use dag::IdSet;
use dag::MemIdDag;
use dag::Vertex;
use minibench::bench;
use minibench::elapsed;
use nonblocking::non_blocking_result as nbr;
use tempfile::tempdir;

type ParentsFunc<'a> = Box<dyn Fn(Vertex) -> dag::Result<Vec<Vertex>> + Send + Sync + 'a>;

pub fn main() {
    println!("benchmarking {} serde", std::any::type_name::<MemIdDag>());
    let parents = bindag::parse_bindag(bindag::MOZILLA);

    let head_name = Vertex::copy_from(format!("{}", parents.len() - 1).as_bytes());
    let parents_by_name: ParentsFunc = Box::new(|name: Vertex| -> dag::Result<Vec<Vertex>> {
        let i = String::from_utf8(name.as_ref().to_vec())
            .unwrap()
            .parse::<usize>()
            .unwrap();
        Ok(parents[i]
            .iter()
            .map(|p| format!("{}", p).as_bytes().to_vec().into())
            .collect())
    });

    let id_map_dir = tempdir().unwrap();
    let mut id_map = IdMap::open(id_map_dir.path()).unwrap();
    let mut covered_ids = IdSet::empty();
    let reserved_ids = IdSet::empty();
    let outcome = nbr(id_map.assign_head(
        head_name,
        &parents_by_name,
        Group::MASTER,
        &mut covered_ids,
        &reserved_ids,
    ))
    .unwrap();
    let mut iddag = IdDag::new_in_memory();
    iddag
        .build_segments_from_prepared_flat_segments(&outcome)
        .unwrap();

    let mut blob = Vec::new();
    bench("serializing inprocess iddag with mincode", || {
        elapsed(|| {
            blob = mincode::serialize(&iddag).unwrap();
        })
    });

    println!("mincode serialized blob has {} bytes", blob.len());

    bench("deserializing inprocess iddag with mincode", || {
        elapsed(|| {
            let _new_iddag: MemIdDag = mincode::deserialize(&blob).unwrap();
        })
    });
}
