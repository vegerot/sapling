/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use dag::iddagstore::IdDagStore;
use dag::idmap::IdMap;
use dag::idmap::IdMapAssignHead;
use dag::namedag::NameDag;
use dag::ops::DagAlgorithm;
use dag::ops::DagPersistent;
use dag::ops::Persist;
use dag::Group;
use dag::Id;
use dag::IdDag;
use dag::IdSet;
use dag::Set;
use dag::VertexListWithOptions;
use dag::VertexName;
use minibench::bench;
use minibench::elapsed;
use nonblocking::non_blocking_result as nbr;
use tempfile::tempdir;

type ParentsFunc<'a> = Box<dyn Fn(VertexName) -> dag::Result<Vec<VertexName>> + Send + Sync + 'a>;

pub fn main() {
    let dag_dir = tempdir().unwrap();

    bench_with_iddag(|| IdDag::open(dag_dir.path()).unwrap());
    bench_with_iddag(IdDag::new_in_process);

    bench_many_heads_namedag();
}

fn bench_with_iddag<S: IdDagStore + Persist>(get_empty_iddag: impl Fn() -> IdDag<S>) {
    println!("benchmarking {}", std::any::type_name::<S>());
    let parents = bindag::parse_bindag(bindag::MOZILLA);

    let head_name = VertexName::copy_from(format!("{}", parents.len() - 1).as_bytes());
    let parents_by_name: ParentsFunc =
        Box::new(|name: VertexName| -> dag::Result<Vec<VertexName>> {
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

    bench("building segments", || {
        let mut dag = get_empty_iddag();
        elapsed(|| {
            dag.build_segments_from_prepared_flat_segments(&outcome)
                .unwrap();
        })
    });

    // Write segments to filesystem.
    let mut dag = get_empty_iddag();
    {
        dag.build_segments_from_prepared_flat_segments(&outcome)
            .unwrap();
        let locked = dag.lock().unwrap();
        dag.persist(&locked).unwrap();
    }

    let sample_two_ids: Vec<IdSet> = (0..parents.len() as u64)
        .step_by(10079)
        .flat_map(|i| {
            (1..parents.len() as u64)
                .step_by(7919)
                .map(move |j| (Id(i), Id(j)).into())
        })
        .collect(); // 2679 samples
    let sample_one_ids: Vec<IdSet> = (0..parents.len() as u64)
        .step_by(153)
        .map(|i| IdSet::from(Id(i)))
        .collect();
    let sample_sets: Vec<IdSet> = (0..parents.len() as u64)
        .step_by(10079)
        .flat_map(|i| {
            ((i + 7919)..parents.len() as u64)
                .step_by(7919)
                .map(move |j| (Id(i)..=Id(j)).into())
        })
        .collect(); // 1471 samples

    bench("ancestors", || {
        elapsed(|| {
            for set in &sample_sets {
                dag.ancestors(set.clone()).unwrap();
            }
        })
    });

    bench("children (spans)", || {
        elapsed(|| {
            for set in &sample_sets {
                dag.children(set.clone()).unwrap();
            }
        })
    });

    bench("children (1 id)", || {
        elapsed(|| {
            for set in &sample_one_ids {
                dag.children(set.clone()).unwrap();
            }
        })
    });

    bench("common_ancestors (spans)", || {
        elapsed(|| {
            for set in &sample_sets {
                dag.common_ancestors(set.clone()).unwrap();
            }
        })
    });

    bench("descendants (small subset)", || {
        elapsed(|| {
            // "descendants" is extremely slow. Therefore only test a very
            // small subset.
            for set in sample_sets.iter().skip(500).take(2) {
                dag.descendants(set.clone()).unwrap();
            }
        })
    });

    bench("gca_one (2 ids)", || {
        elapsed(|| {
            for set in &sample_two_ids {
                dag.gca_one(set.clone()).unwrap();
            }
        })
    });

    bench("gca_one (spans)", || {
        elapsed(|| {
            for set in &sample_sets {
                dag.gca_one(set.clone()).unwrap();
            }
        })
    });

    bench("gca_all (2 ids)", || {
        elapsed(|| {
            for set in &sample_two_ids {
                dag.gca_all(set.clone()).unwrap();
            }
        })
    });

    bench("gca_all (spans)", || {
        elapsed(|| {
            for set in &sample_sets {
                dag.gca_all(set.clone()).unwrap();
            }
        })
    });

    bench("heads", || {
        elapsed(|| {
            for set in &sample_sets {
                dag.heads(set.clone()).unwrap();
            }
        })
    });

    bench("heads_ancestors", || {
        elapsed(|| {
            for set in &sample_sets {
                dag.heads_ancestors(set.clone()).unwrap();
            }
        })
    });

    bench("is_ancestor", || {
        elapsed(|| {
            for set in &sample_two_ids {
                let ids: Vec<_> = set.iter_desc().collect();
                dag.is_ancestor(ids[0], ids[1]).unwrap();
            }
        })
    });

    bench("parents", || {
        elapsed(|| {
            for set in &sample_sets {
                dag.parents(set.clone()).unwrap();
            }
        })
    });

    bench("parent_ids", || {
        elapsed(|| {
            for set in &sample_two_ids {
                for id in set.iter_desc() {
                    dag.parent_ids(id).unwrap();
                }
            }
        })
    });

    bench("range (2 ids)", || {
        elapsed(|| {
            for set in &sample_two_ids {
                let ids: Vec<_> = set.iter_desc().collect();
                dag.range(ids[0].into(), ids[1].into()).unwrap();
            }
        })
    });

    bench("range (spans)", || {
        elapsed(|| {
            let mut iter = sample_sets.iter();
            if let (Some(set1), Some(set2)) = (iter.next(), iter.next_back()) {
                dag.range(set1.clone(), set2.clone()).unwrap();
            }
        })
    });

    bench("roots", || {
        elapsed(|| {
            for set in &sample_sets {
                dag.roots(set.clone()).unwrap();
            }
        })
    });
}

fn bench_many_heads_namedag() {
    println!("benchmarking NameDag with many heads");
    // Create a graph with M linear vertexes in the master branch, and M
    // child for every vertex in the master branch.
    //
    // VertexName are just strings of Ids (0, 1, ..., N0, N1, ...).
    const M: usize = 8192;
    let parent_func: ParentsFunc = Box::new(|v: VertexName| -> dag::Result<Vec<VertexName>> {
        let is_non_master = v.as_ref().starts_with(b"N");
        let idx: usize = if is_non_master {
            std::str::from_utf8(&v.as_ref()[1..])
                .unwrap()
                .parse()
                .unwrap()
        } else {
            std::str::from_utf8(v.as_ref()).unwrap().parse().unwrap()
        };
        let parents = if is_non_master {
            vec![VertexName::copy_from(&v.as_ref()[1..])]
        } else if idx > 0 {
            vec![VertexName::copy_from(format!("{}", idx - 1).as_bytes())]
        } else {
            vec![]
        };
        Ok(parents)
    });
    let non_master_heads: Vec<VertexName> = (0..M)
        .map(|i| VertexName::copy_from(format!("N{}", i).as_bytes()))
        .collect::<Vec<_>>();
    let master_heads: Vec<VertexName> =
        vec![VertexName::copy_from(format!("{}", M - 1).as_bytes())];
    let heads = VertexListWithOptions::from(master_heads)
        .with_highest_group(Group::MASTER)
        .chain(non_master_heads);
    let dag_dir = tempdir().unwrap();
    let mut dag = NameDag::open(dag_dir.path()).unwrap();
    nbr(dag.add_heads_and_flush(&parent_func, &heads)).unwrap();

    let to_set = |v: &str| -> Set {
        nbr(dag.sort(&Set::from_static_names(vec![VertexName::copy_from(
            v.as_bytes(),
        )])))
        .unwrap()
    };
    let head_root_pairs: Vec<(Set, Set)> = (0..M)
        .map(|i| {
            let head = to_set(&format!("N{}", i));
            let root = to_set(&format!("{}", i));
            (head, root)
        })
        .collect();
    bench("range (master::draft)", || {
        elapsed(|| {
            for (head, root) in &head_root_pairs {
                nbr(dag.range(root.clone(), head.clone())).unwrap();
            }
        })
    });

    let heads = nbr(dag.heads(nbr(dag.all()).unwrap())).unwrap();
    let root_list: Vec<Set> = ((M - 64)..M).map(|i| to_set(&format!("N{}", i))).collect();
    bench("range (recent_draft::drafts)", || {
        elapsed(|| {
            for root in &root_list {
                nbr(dag.range(root.clone(), heads.clone())).unwrap();
            }
        })
    });
}
