/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;

use async_trait::async_trait;
use dag::delegate;
use dag::ops::DagAlgorithm;
use dag::ops::DagImportCloneData;
use dag::ops::DagImportPullData;
use dag::ops::DagPersistent;
use dag::protocol::AncestorPath;
use dag::protocol::RemoteIdConvertProtocol;
use dag::CloneData;
use dag::Location;
use dag::Set;
use dag::Vertex;
use dag::VertexListWithOptions;
use edenapi::types::CommitLocationToHashRequest;
use edenapi::SaplingRemoteApi;
use format_util::git_sha1_serialize;
use format_util::hg_sha1_deserialize;
use format_util::strip_sha1_header;
use futures::stream;
use futures::stream::BoxStream;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;
use minibytes::Bytes;
use parking_lot::RwLock;
use storemodel::SerializationFormat;
use streams::HybridResolver;
use streams::HybridStream;
use tracing::instrument;
use zstore::Id20;
use zstore::Zstore;

use crate::AppendCommits;
use crate::DescribeBackend;
use crate::HgCommit;
use crate::OnDiskCommits;
use crate::ParentlessHgCommit;
use crate::ReadCommitText;
use crate::Result;
use crate::RevlogCommits;
use crate::StreamCommitText;
use crate::StripCommits;

/// Segmented Changelog + Revlog (Optional) + Remote.
///
/// Use segmented changelog for the commit graph algorithms and IdMap.
/// Optionally writes to revlog just for fallback.
///
/// Use edenapi to resolve public commit messages and hashes.
pub struct HybridCommits {
    revlog: Option<RevlogCommits>,
    commits: OnDiskCommits,
    client: Arc<dyn SaplingRemoteApi>,
    lazy_hash_desc: String,
}

const EDENSCM_DISABLE_REMOTE_RESOLVE: &str = "EDENSCM_DISABLE_REMOTE_RESOLVE";
const EDENSCM_REMOTE_ID_THRESHOLD: &str = "EDENSCM_REMOTE_ID_THRESHOLD";
const EDENSCM_REMOTE_NAME_THRESHOLD: &str = "EDENSCM_REMOTE_NAME_THRESHOLD";

struct SaplingRemoteApiProtocol {
    client: Arc<dyn SaplingRemoteApi>,

    /// Manually disabled names defined by `EDENSCM_DISABLE_REMOTE_RESOLVE`
    /// in the form `hex1,hex2,...`.
    disabled_names: HashSet<Vertex>,

    /// Manually disabled ID resolution after `N` entries.
    /// Set by `EDENSCM_REMOTE_ID_THRESHOLD=N`.
    remote_id_threshold: Option<usize>,
    remote_id_current: AtomicUsize,

    /// Manually disabled name resolution after `N` entries.
    /// Set by `EDENSCM_REMOTE_NAME_THRESHOLD=N`.
    remote_name_threshold: Option<usize>,
    remote_name_current: AtomicUsize,
}

fn to_dag_error<E: Into<anyhow::Error>>(e: E) -> dag::Error {
    dag::errors::BackendError::Other(e.into()).into()
}

#[async_trait]
impl RemoteIdConvertProtocol for SaplingRemoteApiProtocol {
    async fn resolve_names_to_relative_paths(
        &self,
        heads: Vec<Vertex>,
        names: Vec<Vertex>,
    ) -> dag::Result<Vec<(AncestorPath, Vec<Vertex>)>> {
        let mut pairs = Vec::with_capacity(names.len());
        let response_vec = {
            if heads.is_empty() {
                // Not an error case. Just do not resolve anything.
                return Ok(Vec::new());
            }
            let mut hgids = Vec::with_capacity(names.len());
            for name in names {
                if self.disabled_names.contains(&name) {
                    let msg = format!(
                        "Resolving {:?} is disabled via {}",
                        name, EDENSCM_DISABLE_REMOTE_RESOLVE
                    );
                    return Err(dag::errors::BackendError::Generic(msg).into());
                }
                if name.as_ref() == Id20::wdir_id().as_ref()
                    || name.as_ref() == Id20::null_id().as_ref()
                {
                    // Do not borther asking server about virtual nodes.
                    // Check resolve_names_to_relative_paths API docstring.
                    continue;
                }
                if let Some(threshold) = self.remote_name_threshold {
                    let current = self.remote_name_current.fetch_add(1, SeqCst);
                    if current >= threshold {
                        let msg = format!(
                            "Resolving name {:?} exceeds threshold {} set by {}",
                            name, threshold, EDENSCM_REMOTE_NAME_THRESHOLD
                        );
                        return Err(dag::errors::BackendError::Generic(msg).into());
                    }
                }
                hgids.push(Id20::from_slice(name.as_ref()).map_err(to_dag_error)?);
            }
            let heads: Vec<_> = heads
                .iter()
                .map(|v| Id20::from_slice(v.as_ref()).map_err(to_dag_error))
                .collect::<dag::Result<Vec<_>>>()?;
            self.client
                .commit_hash_to_location(heads, hgids)
                .await
                .map_err(to_dag_error)?
        };
        for response in response_vec {
            if let Some(location) = response.result.map_err(to_dag_error)? {
                let path = AncestorPath {
                    x: Vertex::copy_from(location.descendant.as_ref()),
                    n: location.distance,
                    batch_size: 1,
                };
                let name = Vertex::copy_from(response.hgid.as_ref());
                pairs.push((path, vec![name]));
            }
        }
        Ok(pairs)
    }

    async fn resolve_relative_paths_to_names(
        &self,
        paths: Vec<AncestorPath>,
    ) -> dag::Result<Vec<(AncestorPath, Vec<Vertex>)>> {
        if let Some(threshold) = self.remote_id_threshold {
            let current = self.remote_id_current.fetch_add(1, SeqCst);
            if current >= threshold {
                let msg = format!(
                    "Resolving id exceeds threshold {} set by {}",
                    threshold, EDENSCM_REMOTE_ID_THRESHOLD
                );
                return Err(dag::errors::BackendError::Generic(msg).into());
            }
        }
        let mut pairs = Vec::with_capacity(paths.len());
        let response_vec = {
            let mut requests = Vec::with_capacity(paths.len());
            for path in paths {
                let descendant = Id20::from_slice(path.x.as_ref()).map_err(to_dag_error)?;
                requests.push(CommitLocationToHashRequest {
                    location: Location {
                        descendant,
                        distance: path.n,
                    },
                    count: path.batch_size,
                });
            }
            self.client
                .commit_location_to_hash(requests)
                .await
                .map_err(to_dag_error)?
        };
        for response in response_vec {
            let path = AncestorPath {
                x: Vertex::copy_from(response.location.descendant.as_ref()),
                n: response.location.distance,
                batch_size: response.count,
            };
            let names = response
                .hgids
                .into_iter()
                .map(|n| Vertex::copy_from(n.as_ref()))
                .collect();
            pairs.push((path, names));
        }
        Ok(pairs)
    }
}

impl HybridCommits {
    pub fn new(
        revlog_dir: Option<&Path>,
        dag_path: &Path,
        commits_path: &Path,
        client: Arc<dyn SaplingRemoteApi>,
        format: SerializationFormat,
    ) -> Result<Self> {
        let commits = OnDiskCommits::new(dag_path, commits_path, format)?;
        let revlog = match revlog_dir {
            Some(revlog_dir) => Some(RevlogCommits::new(revlog_dir, format)?),
            None => None,
        };
        Ok(Self {
            revlog,
            commits,
            client,
            lazy_hash_desc: "not lazy".to_string(),
        })
    }

    /// Enable fetching commit hashes lazily via SaplingRemoteAPI.
    pub fn enable_lazy_commit_hashes(&mut self) {
        let mut disabled_names: HashSet<Vertex> = Default::default();
        if let Ok(env) = std::env::var(EDENSCM_DISABLE_REMOTE_RESOLVE) {
            for hex in env.split(',') {
                if let Ok(name) = Vertex::from_hex(hex.as_ref()) {
                    disabled_names.insert(name);
                }
            }
        }
        let remote_id_threshold = if let Ok(env) = std::env::var(EDENSCM_REMOTE_ID_THRESHOLD) {
            env.parse::<usize>().ok()
        } else {
            None
        };
        let remote_name_threshold = if let Ok(env) = std::env::var(EDENSCM_REMOTE_NAME_THRESHOLD) {
            env.parse::<usize>().ok()
        } else {
            None
        };
        let protocol = SaplingRemoteApiProtocol {
            client: self.client.clone(),
            disabled_names,
            remote_id_threshold,
            remote_id_current: Default::default(),
            remote_name_threshold,
            remote_name_current: Default::default(),
        };
        self.commits.dag.set_remote_protocol(Arc::new(protocol));
        self.lazy_hash_desc = "lazy, using SaplingRemoteAPI".to_string();
    }

    /// Enable fetching commit hashes lazily via another "segments".
    /// directory locally. This is for testing purpose.
    pub fn enable_lazy_commit_hashes_from_local_segments(&mut self, dag_path: &Path) -> Result<()> {
        let dag = dag::Dag::open(dag_path)?;
        self.commits.dag.set_remote_protocol(Arc::new(dag));
        self.lazy_hash_desc = format!("lazy, using local segments ({})", dag_path.display());
        Ok(())
    }

    fn to_hybrid_commit_text(&self) -> HybridCommitTextReader {
        HybridCommitTextReader {
            zstore: self.commits.commit_data_store(),
            client: self.client.clone(),
            format: self.commits.format(),
        }
    }
}

#[async_trait::async_trait]
impl AppendCommits for HybridCommits {
    async fn add_commits(&mut self, commits: &[HgCommit]) -> Result<()> {
        if let Some(revlog) = self.revlog.as_mut() {
            revlog.add_commits(commits).await?;
        }
        self.commits.add_commits(commits).await?;
        Ok(())
    }

    async fn flush(&mut self, master_heads: &[Vertex]) -> Result<()> {
        if let Some(revlog) = self.revlog.as_mut() {
            revlog.flush(master_heads).await?;
        }
        self.commits.flush(master_heads).await?;
        Ok(())
    }

    async fn flush_commit_data(&mut self) -> Result<()> {
        if let Some(revlog) = self.revlog.as_mut() {
            revlog.flush_commit_data().await?;
        }
        self.commits.flush_commit_data().await?;
        self.commits.dag.flush_cached_idmap().await?;
        Ok(())
    }

    async fn add_graph_nodes(&mut self, graph_nodes: &[crate::GraphNode]) -> Result<()> {
        if self.revlog.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "add_graph_nodes is not supported for revlog backend",
            )
            .into());
        }
        self.commits.add_graph_nodes(graph_nodes).await?;
        Ok(())
    }

    async fn import_clone_data(&mut self, clone_data: CloneData<Vertex>) -> Result<()> {
        if self.revlog.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "import_clone_data is not supported for revlog backend",
            )
            .into());
        }
        if self.commits.dag.all().await?.count().await? > 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "import_clone_data can only be used in an empty repo",
            )
            .into());
        }
        if !self.commits.dag.is_vertex_lazy() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "import_clone_data can only be used in commit graph with lazy vertexes",
            )
            .into());
        }
        self.commits.dag.import_clone_data(clone_data).await?;
        Ok(())
    }

    async fn import_pull_data(
        &mut self,
        clone_data: CloneData<Vertex>,
        heads: &VertexListWithOptions,
    ) -> Result<()> {
        if self.revlog.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "import_pull_data is not supported for revlog backend",
            )
            .into());
        }
        if !self.commits.dag.is_vertex_lazy() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "import_pull_data can only be used in commit graph with lazy vertexes",
            )
            .into());
        }
        self.commits.dag.import_pull_data(clone_data, heads).await?;
        Ok(())
    }

    async fn update_virtual_nodes(&mut self, wdir_parents: Vec<Vertex>) -> Result<()> {
        self.commits.update_virtual_nodes(wdir_parents).await
    }
}

/// Subset of HybridCommits useful to read commit text.
#[derive(Clone)]
struct HybridCommitTextReader {
    zstore: Arc<RwLock<Zstore>>,
    client: Arc<dyn SaplingRemoteApi>,
    format: SerializationFormat,
}

#[async_trait::async_trait]
impl ReadCommitText for HybridCommits {
    async fn get_commit_raw_text_list(&self, vertexes: &[Vertex]) -> Result<Vec<Bytes>> {
        self.to_hybrid_commit_text()
            .get_commit_raw_text_list(vertexes)
            .await
    }

    fn to_dyn_read_commit_text(&self) -> Arc<dyn ReadCommitText + Send + Sync> {
        Arc::new(self.to_hybrid_commit_text())
    }

    fn format(&self) -> SerializationFormat {
        self.commits.format()
    }
}

#[async_trait::async_trait]
impl ReadCommitText for HybridCommitTextReader {
    async fn get_commit_raw_text_list(&self, vertexes: &[Vertex]) -> Result<Vec<Bytes>> {
        let vertexes: Vec<Vertex> = vertexes.to_vec();
        let stream =
            self.stream_commit_raw_text(Box::pin(stream::iter(vertexes.into_iter().map(Ok))))?;
        let commits: Vec<Bytes> = stream.map(|c| c.map(|c| c.raw_text)).try_collect().await?;
        Ok(commits)
    }

    fn to_dyn_read_commit_text(&self) -> Arc<dyn ReadCommitText + Send + Sync> {
        Arc::new(self.clone())
    }

    fn format(&self) -> SerializationFormat {
        self.format
    }
}

impl StreamCommitText for HybridCommits {
    fn stream_commit_raw_text(
        &self,
        input: BoxStream<'static, anyhow::Result<Vertex>>,
    ) -> Result<BoxStream<'static, anyhow::Result<ParentlessHgCommit>>> {
        self.to_hybrid_commit_text().stream_commit_raw_text(input)
    }
}

impl StreamCommitText for HybridCommitTextReader {
    fn stream_commit_raw_text(
        &self,
        input: BoxStream<'static, anyhow::Result<Vertex>>,
    ) -> Result<BoxStream<'static, anyhow::Result<ParentlessHgCommit>>> {
        let zstore = self.zstore.clone();
        let client = self.client.clone();
        let format = self.format;
        let resolver = Resolver {
            client,
            zstore,
            format,
        };
        let buffer_size = 10000;
        let retry_limit = 0;
        let stream = HybridStream::new(input, resolver, buffer_size, retry_limit);
        let stream = stream.map_ok(|(vertex, raw_text)| ParentlessHgCommit { vertex, raw_text });
        Ok(Box::pin(stream))
    }
}

#[async_trait::async_trait]
impl StripCommits for HybridCommits {
    async fn strip_commits(&mut self, set: Set) -> Result<()> {
        if let Some(revlog) = self.revlog.as_mut() {
            revlog.strip_commits(set.clone()).await?;
        }
        self.commits.strip_commits(set).await?;
        Ok(())
    }
}

struct Resolver {
    client: Arc<dyn SaplingRemoteApi>,
    zstore: Arc<RwLock<Zstore>>,
    format: SerializationFormat,
}

impl Drop for Resolver {
    fn drop(&mut self) {
        // Write commit data back to zstore, best effort.
        let _ = self.zstore.write().flush();
    }
}

#[async_trait]
impl HybridResolver<Vertex, Bytes, anyhow::Error> for Resolver {
    fn resolve_local(&mut self, vertex: &Vertex) -> anyhow::Result<Option<Bytes>> {
        let id = Id20::from_slice(vertex.as_ref())?;
        if &id == Id20::wdir_id() || &id == Id20::null_id() {
            // Do not borther asking server about virtual nodes.
            return Ok(Some(Bytes::new()));
        }
        match self.zstore.read().get(id)? {
            Some(bytes) => {
                let text = strip_sha1_header(&bytes, self.format)?;
                Ok(Some(text))
            }
            None => Ok(crate::revlog::get_hard_coded_commit_text(vertex)),
        }
    }

    #[instrument(level = "debug", skip(self))]
    async fn resolve_remote(
        &self,
        input: &[Vertex],
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<(Vertex, Bytes)>>> {
        let ids: Vec<Id20> = input
            .iter()
            .map(|i| Id20::from_slice(i.as_ref()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let client = self.client.clone();
        let response = client.commit_revlog_data(ids).await?;
        let zstore = self.zstore.clone();
        let format = self.format;
        let commits = response.entries.map(move |e| {
            let e = e?;
            let text = match format {
                // For hg, `e` includes the `p1`, `p2` prefix so its SHA1 can be verified.
                SerializationFormat::Hg => e.revlog_data.clone(),
                // For git, the `revlog_data` does not have git framing, we need to add the
                // framing to verify SHA1.
                SerializationFormat::Git => {
                    Bytes::from(git_sha1_serialize(&e.revlog_data, "commit"))
                }
            };
            let written_id = zstore.write().insert(&text, &[])?;
            if !written_id.is_null() && written_id != e.hgid {
                anyhow::bail!(
                    "server returned commit-text pair ({}, {:?}) has mismatched {:?} SHA1: {}",
                    e.hgid.to_hex(),
                    e.revlog_data,
                    format,
                    written_id.to_hex(),
                );
            }
            let commit_text = match format {
                SerializationFormat::Hg => e
                    .revlog_data
                    .slice_to_bytes(hg_sha1_deserialize(e.revlog_data.as_ref())?.0),
                SerializationFormat::Git => e.revlog_data.clone(),
            };
            let input_output = (Vertex::copy_from(e.hgid.as_ref()), commit_text);
            Ok(input_output)
        });
        Ok(Box::pin(commits) as BoxStream<'_, _>)
    }

    fn retry_error(&self, _attempt: usize, input: &[Vertex]) -> anyhow::Error {
        anyhow::format_err!("cannot resolve {:?} remotely", input)
    }
}

delegate!(CheckIntegrity | IdConvert | IdMapSnapshot | PrefixLookup | DagAlgorithm, HybridCommits => self.commits);

impl DescribeBackend for HybridCommits {
    fn algorithm_backend(&self) -> &'static str {
        "segments"
    }

    fn describe_backend(&self) -> String {
        let (backend, revlog_path, revlog_usage) = match self.revlog.as_ref() {
            Some(revlog) => {
                let path = revlog.dir.join("00changelog.{i,d,nodemap}");
                (
                    "hybrid",
                    path.display().to_string(),
                    "present, not used for reading",
                )
            }
            None => ("lazytext", "(not used)".to_string(), "(not used)"),
        };
        format!(
            r#"Backend ({}):
  Local:
    Segments + IdMap: {}
    Zstore: {}
    Revlog + Nodemap: {}
Feature Providers:
  Commit Graph Algorithms:
    Segments
  Commit Hash / Rev Lookup:
    IdMap
  Commit Data (user, message):
    Zstore (incomplete, draft)
    SaplingRemoteAPI (remaining, public)
    Revlog {}
Commit Hashes: {}
"#,
            backend,
            self.commits.dag_path.display(),
            self.commits.commits_path.display(),
            revlog_path,
            revlog_usage,
            &self.lazy_hash_desc,
        )
    }

    fn explain_internals(&self, w: &mut dyn io::Write) -> io::Result<()> {
        self.commits.explain_internals(w)
    }
}
