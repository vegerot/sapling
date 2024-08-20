/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;

use futures::StreamExt;
use futures::TryStreamExt;
use nonblocking::non_blocking;
use nonblocking::non_blocking_result;
use tracing::debug;

use crate::ops::CheckIntegrity;
use crate::ops::DagAddHeads;
use crate::ops::DagAlgorithm;
use crate::ops::DagExportCloneData;
use crate::ops::DagExportPullData;
use crate::ops::DagImportCloneData;
use crate::ops::DagImportPullData;
use crate::ops::DagPersistent;
use crate::ops::DagStrip;
use crate::ops::IdConvert;
use crate::protocol;
use crate::protocol::RemoteIdConvertProtocol;
#[cfg(feature = "render")]
use crate::render::render_dag;
use crate::tests::DrawDag;
use crate::CloneData;
use crate::Dag;
use crate::Group;
use crate::Level;
use crate::Result;
use crate::Set;
use crate::Vertex;
use crate::VertexListWithOptions;

/// Dag structure for testing purpose.
pub struct TestDag {
    pub dag: Dag,
    pub seg_size: usize,
    pub dir: tempfile::TempDir,
    pub output: Arc<Mutex<Vec<String>>>,
}

impl TestDag {
    /// Creates a `TestDag` for testing.
    /// Side effect of the `TestDag` will be removed on drop.
    pub fn new() -> Self {
        Self::new_with_segment_size(3)
    }

    /// Crates a `TestDag` using the given ASCII.
    ///
    /// This is just `new`, followed by `drawdag`, with an extra rule that
    /// comments like "# master: M" at the end can be used to specify master
    /// heads .
    pub fn draw(text: &str) -> Self {
        let mut dag = Self::new();
        let mut split = text.split("# master:");
        let text = split.next().unwrap_or("");
        let master = match split.next() {
            Some(t) => t.split_whitespace().collect::<Vec<_>>(),
            None => Vec::new(),
        };
        dag.drawdag(text, &master);
        dag
    }

    /// Similar to `draw` but creates a lazy client so all vertexes
    /// in the master group are lazy.
    pub async fn draw_client(text: &str) -> Self {
        let server = Self::draw(text);
        // clone data won't include non-master group.
        let mut client = server.client_cloned_data().await;
        tracing::debug!("CLIENT");
        #[cfg(test)]
        tracing::debug!("CLIENT: {}", client.dump_state().await);
        let non_master_heads = {
            let all = server.dag.all().await.unwrap();
            let non_master = all.difference(&server.dag.master_group().await.unwrap());
            let heads = server.dag.heads(non_master).await.unwrap();
            let iter = heads.iter().await.unwrap();
            iter.try_collect::<Vec<_>>().await.unwrap()
        };
        let heads =
            VertexListWithOptions::from(non_master_heads).with_desired_group(Group::NON_MASTER);
        client
            .dag
            .add_heads_and_flush(&server.dag.dag_snapshot().unwrap(), &heads)
            .await
            .unwrap();
        client
    }

    /// Creates a `TestDag` with a specific segment size.
    pub fn new_with_segment_size(seg_size: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let dag = Dag::open(dir.path().join("n")).unwrap();
        Self {
            dir,
            dag,
            seg_size,
            output: Default::default(),
        }
    }

    /// Reopen the dag. Drop in-memory state including caches.
    pub fn reopen(&mut self) {
        let mut dag = Dag::open(self.dir.path().join("n")).unwrap();
        dag.set_remote_protocol(self.dag.get_remote_protocol());
        self.dag = dag;
    }

    /// Add vertexes to the graph. Does not resolve vertexes remotely.
    pub fn drawdag(&mut self, text: &str, master_heads: &[&str]) {
        self.drawdag_with_limited_heads(text, master_heads, None);
    }

    /// Add vertexes to the graph. Async version that might resolve vertexes
    /// remotely on demand.
    pub async fn drawdag_async(&mut self, text: &str, master_heads: &[&str]) {
        // Do not call self.validate to avoid fetching vertexes remotely.
        self.drawdag_with_limited_heads_async(text, master_heads, None, false)
            .await
    }

    /// Add vertexes to the graph.
    ///
    /// If `heads` is set, ignore part of the graph. Only consider specified
    /// heads.
    pub fn drawdag_with_limited_heads(
        &mut self,
        text: &str,
        master_heads: &[&str],
        heads: Option<&[&str]>,
    ) {
        non_blocking(self.drawdag_with_limited_heads_async(text, master_heads, heads, true))
            .unwrap()
    }

    pub async fn drawdag_with_limited_heads_async(
        &mut self,
        text: &str,
        master_heads: &[&str],
        heads: Option<&[&str]>,
        validate: bool,
    ) {
        let (all_heads, parent_func) = get_heads_and_parents_func_from_ascii(text);
        let heads = match heads {
            Some(heads) => heads
                .iter()
                .map(|s| Vertex::copy_from(s.as_bytes()))
                .collect(),
            None => all_heads,
        };
        self.dag.dag.set_new_segment_size(self.seg_size);
        self.dag
            .add_heads(&parent_func, &heads.into())
            .await
            .unwrap();
        if validate {
            self.validate().await;
        }
        let problems = self.dag.check_segments().await.unwrap();
        assert!(
            problems.is_empty(),
            "problems after drawdag: {:?}",
            problems
        );
        let master_heads = master_heads
            .iter()
            .map(|s| Vertex::copy_from(s.as_bytes()))
            .collect::<Vec<_>>();
        let need_flush = !master_heads.is_empty();
        if need_flush {
            let heads = VertexListWithOptions::from(master_heads).with_desired_group(Group::MASTER);
            self.dag.flush(&heads).await.unwrap();
        }
        if validate {
            self.validate().await;
        }
        assert_eq!(self.dag.check_segments().await.unwrap(), [] as [String; 0]);
    }

    /// Add one vertex to the non-master group. `parents` is split by whitespaces.
    pub async fn add_one_vertex(&mut self, name: &str, parents: &str) {
        let name = Vertex::copy_from(name.as_bytes());
        let parents: Vec<Vertex> = parents
            .split_whitespace()
            .map(|s| Vertex::copy_from(s.as_bytes()))
            .collect();
        let heads =
            VertexListWithOptions::from(&[name.clone()][..]).with_desired_group(Group::NON_MASTER);
        self.dag
            .add_heads(
                &std::iter::once((name, parents)).collect::<HashMap<Vertex, Vec<Vertex>>>(),
                &heads,
            )
            .await
            .unwrap();
    }

    /// Flush space-separated master heads.
    pub async fn flush(&mut self, master_heads: &str) {
        let heads: Vec<Vertex> = master_heads
            .split_whitespace()
            .map(|v| Vertex::copy_from(v.as_bytes()))
            .collect();
        let heads = VertexListWithOptions::from(heads).with_desired_group(Group::MASTER);
        self.dag.flush(&heads).await.unwrap();
    }

    /// Replace ASCII with Ids in the graph.
    pub fn annotate_ascii(&self, text: &str) -> String {
        self.dag.map.replace(text)
    }

    /// Render the segments.
    pub fn render_segments(&self) -> String {
        format!("{:?}", &self.dag.dag)
    }

    #[cfg(feature = "render")]
    /// Render the graph.
    pub fn render_graph(&self) -> String {
        render_dag(&self.dag, |v| {
            Some(
                non_blocking_result(self.dag.vertex_id(v.clone()))
                    .unwrap()
                    .to_string(),
            )
        })
        .unwrap()
    }

    /// Use this DAG as the "server", return the "client" Dag that has lazy Vertexes.
    pub async fn client(&self) -> TestDag {
        let mut client = TestDag::new();
        client.set_remote(self);
        client
    }

    /// Update remote protocol to use the (updated) server graph.
    pub fn set_remote(&mut self, server_dag: &Self) {
        let remote = server_dag.remote_protocol(self.output.clone());
        self.dag.set_remote_protocol(remote);
    }

    /// Alternative syntax of `set_remote`.
    pub fn with_remote(mut self, server_dag: &Self) -> Self {
        self.set_remote(server_dag);
        self
    }

    /// Similar to `client`, but also clone the Dag from the server.
    pub async fn client_cloned_data(&self) -> TestDag {
        let mut client = self.client().await;
        let data = self.dag.export_clone_data().await.unwrap();
        tracing::debug!("clone data: {:?}", &data);
        client.dag.import_clone_data(data).await.unwrap();
        client
    }

    /// Pull from the server Dag using the master fast forward fast path.
    pub async fn pull_ff_master(
        &mut self,
        server: &Self,
        old_master: impl Into<Set>,
        new_master: impl Into<Set>,
    ) -> Result<()> {
        self.set_remote(server);
        let old_master = old_master.into();
        let new_master = new_master.into();
        let pull_data = server
            .export_pull_data(old_master.clone(), new_master.clone())
            .await?;
        let head_opts = to_head_opts(new_master);
        self.import_pull_data(pull_data, head_opts).await?;
        Ok(())
    }

    /// Generate the "pull data". This is intended to be called from a "server".
    pub async fn export_pull_data(
        &self,
        common: impl Into<Set>,
        heads: impl Into<Set>,
    ) -> Result<CloneData<Vertex>> {
        let missing = self.dag.only(heads.into(), common.into()).await?;
        let data = self.dag.export_pull_data(&missing).await?;
        debug!("export_pull_data: {:?}", &data);
        Ok(data)
    }

    /// Imports the "pull data". This is intended to be called from a "client".
    pub async fn import_pull_data(
        &mut self,
        pull_data: CloneData<Vertex>,
        head_opts: impl Into<VertexListWithOptions>,
    ) -> Result<()> {
        let head_opts = head_opts.into();
        self.dag.import_pull_data(pull_data, &head_opts).await?;
        Ok(())
    }

    /// Strip space-separated vertexes.
    pub async fn strip(&mut self, names: &'static str) {
        let set = Set::from_static_names(names.split(' ').map(|s| s.into()));
        self.dag.strip(&set).await.unwrap();
        let problems = self.dag.check_segments().await.unwrap();
        assert!(problems.is_empty(), "problems after strip: {:?}", problems);
    }

    /// Remote protocol used to resolve Id <-> Vertex remotely using the test dag
    /// as the "server".
    ///
    /// Logs of the remote access will be written to `output`.
    pub fn remote_protocol(
        &self,
        output: Arc<Mutex<Vec<String>>>,
    ) -> Arc<dyn RemoteIdConvertProtocol> {
        let remote = ProtocolMonitor {
            inner: Box::new(self.dag.try_snapshot().unwrap()),
            output,
        };
        Arc::new(remote)
    }

    /// Describe segments at the given level and group as a string.
    pub fn debug_segments(&self, level: Level, group: Group) -> String {
        let lines =
            crate::dag::debug_segments_by_level_group(&self.dag.dag, &self.dag.map, level, group);
        lines
            .iter()
            .map(|l| format!("\n        {}", l))
            .collect::<Vec<String>>()
            .concat()
    }

    /// Output of remote protocols since the last call.
    pub fn output(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut output = self.output.lock().unwrap();
        std::mem::swap(&mut result, &mut *output);
        result
    }

    /// Check that a vertex exists locally.
    pub fn contains_vertex_locally(&self, name: impl Into<Vertex>) -> bool {
        non_blocking_result(self.dag.contains_vertex_name_locally(&[name.into()])).unwrap()[0]
    }

    #[cfg(test)]
    /// Dump Dag state as a string.
    pub async fn dump_state(&self) -> String {
        use crate::iddagstore::tests::dump_store_state;
        use crate::Id;
        let iddag = &self.dag.dag;
        let all = iddag.all().unwrap();
        let iddag_state = dump_store_state(&iddag.store, &all);
        let all_str = format!("{:?}", &self.dag.all().await.unwrap());
        let idmap_state: String = {
            let all: Vec<Id> = all.iter_asc().collect();
            let contains = self.dag.contains_vertex_id_locally(&all).await.unwrap();
            let local_ids: Vec<Id> = all
                .into_iter()
                .zip(contains)
                .filter(|(_, c)| *c)
                .map(|(i, _)| i)
                .collect();
            let local_vertexes = self
                .dag
                .vertex_name_batch(&local_ids)
                .await
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>>>()
                .unwrap();
            local_ids
                .into_iter()
                .zip(local_vertexes)
                .map(|(i, v)| format!("{:?}->{:?}", i, v))
                .collect::<Vec<_>>()
                .join(" ")
        };

        format!("{}{}\n{}", all_str, iddag_state, idmap_state)
    }

    #[cfg(test)]
    /// Dump Dag segments as ASCII string.
    pub fn dump_segments_ascii(&self) -> String {
        use std::collections::HashSet;

        use crate::Id;
        use crate::IdSet;
        use crate::IdSpan;

        let span_iter = |span: IdSpan| IdSet::from_spans(vec![span]).into_iter().rev();
        let iddag = &self.dag.dag;
        let all_ids = iddag.all_ids_in_groups(&Group::ALL).unwrap();
        let max_level = iddag.max_level().unwrap();
        let mut output = String::new();
        for level in 0..=max_level {
            output = format!("{}\n        Lv{}:", output.trim_end(), level);
            for span in all_ids.iter_span_asc() {
                output += " |";
                let segments = iddag.segments_in_span_ascending(*span, level).unwrap();
                let segment_ids: HashSet<Id> = segments
                    .iter()
                    .flat_map(|s| span_iter(s.span().unwrap()))
                    .collect();
                let segment_highs: HashSet<Id> =
                    segments.iter().map(|s| s.high().unwrap()).collect();
                for id in span_iter(*span) {
                    let id_str = format!("{:?}", id);
                    if segment_ids.contains(&id) {
                        output += &id_str
                    } else {
                        let space = " ".repeat(id_str.len());
                        output += &space;
                    };
                    output.push(
                        if segment_highs.contains(&id)
                            || (segment_ids.contains(&(id + 1)) && !segment_ids.contains(&id))
                        {
                            '|'
                        } else {
                            ' '
                        },
                    );
                }
            }
        }
        output.trim_end().to_string()
    }

    async fn validate(&self) {
        // All vertexes should be accessible, and round-trip through IdMap.
        let mut iter = self.dag.all().await.unwrap().iter().await.unwrap();
        while let Some(v) = iter.next().await {
            let v = v.unwrap();
            let id = self.dag.vertex_id(v.clone()).await.unwrap();
            let v2 = self.dag.vertex_name(id).await.unwrap();
            assert_eq!(v, v2);
        }
    }
}

impl Deref for TestDag {
    type Target = Dag;

    fn deref(&self) -> &Self::Target {
        &self.dag
    }
}

impl DerefMut for TestDag {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.dag
    }
}

pub(crate) struct ProtocolMonitor {
    pub(crate) inner: Box<dyn RemoteIdConvertProtocol>,
    pub(crate) output: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl RemoteIdConvertProtocol for ProtocolMonitor {
    async fn resolve_names_to_relative_paths(
        &self,
        heads: Vec<Vertex>,
        names: Vec<Vertex>,
    ) -> Result<Vec<(protocol::AncestorPath, Vec<Vertex>)>> {
        let msg = format!("resolve names: {:?}, heads: {:?}", &names, &heads);
        self.output.lock().unwrap().push(msg);
        self.inner
            .resolve_names_to_relative_paths(heads, names)
            .await
    }

    async fn resolve_relative_paths_to_names(
        &self,
        paths: Vec<protocol::AncestorPath>,
    ) -> Result<Vec<(protocol::AncestorPath, Vec<Vertex>)>> {
        let msg = format!("resolve paths: {:?}", &paths);
        self.output.lock().unwrap().push(msg);
        self.inner.resolve_relative_paths_to_names(paths).await
    }
}

fn get_heads_and_parents_func_from_ascii(text: &str) -> (Vec<Vertex>, DrawDag) {
    let dag = DrawDag::from(text);
    let heads = dag.heads();
    (heads, dag)
}

#[cfg(test)]
impl From<&'static str> for VertexListWithOptions {
    fn from(names: &str) -> Self {
        let set = Set::from(names);
        set.into()
    }
}

#[cfg(test)]
impl From<Set> for VertexListWithOptions {
    fn from(names: Set) -> Self {
        to_head_opts(names)
    }
}

fn to_head_opts(set: Set) -> VertexListWithOptions {
    use crate::set::SyncSetQuery;
    let heads_vec = set.iter().unwrap().collect::<Result<Vec<_>>>().unwrap();
    VertexListWithOptions::from(heads_vec).with_desired_group(Group::MASTER)
}
