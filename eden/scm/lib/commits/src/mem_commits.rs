/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::sync::Arc;

use anyhow::bail;
use dag::delegate;
use dag::errors::NotFoundError;
use dag::ops::DagAddHeads;
use dag::ops::DagStrip;
use dag::MemDag;
use dag::Set;
use dag::Vertex;
use futures::stream::BoxStream;
use futures::stream::StreamExt;
use minibytes::Bytes;
use storemodel::SerializationFormat;

use crate::AppendCommits;
use crate::DescribeBackend;
use crate::GraphNode;
use crate::HgCommit;
use crate::ParentlessHgCommit;
use crate::ReadCommitText;
use crate::Result;
use crate::StreamCommitText;
use crate::StripCommits;

/// HG commits in memory.
pub struct MemCommits {
    commits: HashMap<Vertex, Bytes>,
    dag: MemDag,
    format: SerializationFormat,
}

impl MemCommits {
    pub fn new(format: SerializationFormat) -> Result<Self> {
        let result = Self {
            dag: MemDag::new(),
            commits: HashMap::new(),
            format,
        };
        Ok(result)
    }
}

#[async_trait::async_trait]
impl AppendCommits for MemCommits {
    async fn add_commits(&mut self, commits: &[HgCommit]) -> Result<()> {
        // Write commit data to zstore.
        for commit in commits {
            self.commits
                .insert(commit.vertex.clone(), commit.raw_text.clone());
        }

        // Write commit graph to DAG.
        let graph_nodes = commits
            .iter()
            .map(|c| GraphNode {
                vertex: c.vertex.clone(),
                parents: c.parents.clone(),
            })
            .collect::<Vec<_>>();
        self.add_graph_nodes(&graph_nodes).await?;

        Ok(())
    }

    async fn flush(&mut self, _master_heads: &[Vertex]) -> Result<()> {
        Ok(())
    }

    async fn flush_commit_data(&mut self) -> Result<()> {
        Ok(())
    }

    async fn add_graph_nodes(&mut self, graph_nodes: &[GraphNode]) -> Result<()> {
        // Write commit graph to DAG.
        let parents: HashMap<Vertex, Vec<Vertex>> = graph_nodes
            .iter()
            .cloned()
            .map(|c| (c.vertex, c.parents))
            .collect();
        let heads: Vec<Vertex> = {
            let mut non_heads = HashSet::new();
            for graph_node in graph_nodes {
                for parent in graph_node.parents.iter() {
                    non_heads.insert(parent);
                }
            }
            graph_nodes
                .iter()
                .map(|c| &c.vertex)
                .filter(|v| !non_heads.contains(v))
                .cloned()
                .collect()
        };
        self.dag.add_heads(&parents, &heads.into()).await?;
        Ok(())
    }

    async fn update_virtual_nodes(&mut self, _wdir_parents: Vec<Vertex>) -> Result<()> {
        bail!("virtual nodes for memory backend is not implemented")
    }
}

#[async_trait::async_trait]
impl ReadCommitText for MemCommits {
    async fn get_commit_raw_text(&self, vertex: &Vertex) -> Result<Option<Bytes>> {
        Ok(self.commits.get(vertex).cloned())
    }

    fn to_dyn_read_commit_text(&self) -> Arc<dyn ReadCommitText + Send + Sync> {
        Arc::new(ArcHashMapVertexBytes(
            Arc::new(self.commits.clone()),
            self.format,
        ))
    }

    fn format(&self) -> SerializationFormat {
        self.format
    }
}

#[derive(Clone)]
struct ArcHashMapVertexBytes(Arc<HashMap<Vertex, Bytes>>, SerializationFormat);

#[async_trait::async_trait]
impl ReadCommitText for ArcHashMapVertexBytes {
    async fn get_commit_raw_text(&self, vertex: &Vertex) -> Result<Option<Bytes>> {
        Ok(self.0.get(vertex).cloned())
    }

    fn to_dyn_read_commit_text(&self) -> Arc<dyn ReadCommitText + Send + Sync> {
        Arc::new(self.clone())
    }

    fn format(&self) -> SerializationFormat {
        self.1
    }
}

impl StreamCommitText for MemCommits {
    fn stream_commit_raw_text(
        &self,
        stream: BoxStream<'static, anyhow::Result<Vertex>>,
    ) -> Result<BoxStream<'static, anyhow::Result<ParentlessHgCommit>>> {
        let commits = self.commits.clone();
        let stream = stream.map(move |item| {
            let vertex = item?;
            match commits.get(&vertex) {
                Some(raw_text) => {
                    let raw_text = raw_text.clone();
                    Ok(ParentlessHgCommit { vertex, raw_text })
                }
                None => vertex.not_found().map_err(Into::into),
            }
        });
        Ok(Box::pin(stream))
    }
}

#[async_trait::async_trait]
impl StripCommits for MemCommits {
    async fn strip_commits(&mut self, set: Set) -> Result<()> {
        self.dag.strip(&set).await.map_err(Into::into)
    }
}

delegate!(CheckIntegrity | IdConvert | IdMapSnapshot | PrefixLookup | DagAlgorithm, MemCommits => self.dag);

impl DescribeBackend for MemCommits {
    fn algorithm_backend(&self) -> &'static str {
        "segments"
    }

    fn describe_backend(&self) -> String {
        r#"Backend (memory):
  Local:
    Memory
Feature Providers:
  Commit Graph Algorithms:
    Memory
  Commit Hash / Rev Lookup:
    Memory
  Commit Data (user, message):
    Memory
"#
        .to_string()
    }

    fn explain_internals(&self, w: &mut dyn io::Write) -> io::Result<()> {
        write!(w, "{:?}", &self.dag)
    }
}
