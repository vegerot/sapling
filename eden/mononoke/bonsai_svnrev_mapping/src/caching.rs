/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use abomonation_derive::Abomonation;
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Error;
use anyhow::Result;
use async_trait::async_trait;
use bonsai_svnrev_mapping_thrift as thrift;
use bytes::Bytes;
use caching_ext::get_or_fill;
use caching_ext::CacheDisposition;
use caching_ext::CacheHandlerFactory;
use caching_ext::CacheTtl;
use caching_ext::CachelibHandler;
use caching_ext::EntityStore;
use caching_ext::KeyedEntityStore;
use caching_ext::McErrorKind;
use caching_ext::McResult;
use caching_ext::MemcacheEntity;
use caching_ext::MemcacheHandler;
use context::CoreContext;
use fbthrift::compact_protocol;
use memcache::KeyGen;
use mononoke_types::ChangesetId;
use mononoke_types::RepositoryId;
use mononoke_types::Svnrev;

use super::BonsaiSvnrevMapping;
use super::BonsaiSvnrevMappingEntry;
use super::BonsaisOrSvnrevs;

#[derive(Abomonation, Clone, Debug, Eq, Hash, PartialEq)]
pub struct BonsaiSvnrevMappingCacheEntry {
    pub repo_id: RepositoryId,
    pub bcs_id: ChangesetId,
    pub svnrev: Svnrev,
}

impl BonsaiSvnrevMappingCacheEntry {
    pub fn new(repo_id: RepositoryId, bcs_id: ChangesetId, svnrev: Svnrev) -> Self {
        BonsaiSvnrevMappingCacheEntry {
            repo_id,
            bcs_id,
            svnrev,
        }
    }

    fn into_entry(self, repo_id: RepositoryId) -> Result<BonsaiSvnrevMappingEntry> {
        if self.repo_id == repo_id {
            Ok(BonsaiSvnrevMappingEntry {
                bcs_id: self.bcs_id,
                svnrev: self.svnrev,
            })
        } else {
            Err(anyhow!(
                "Cache returned invalid entry: repo {} returned for query to repo {}",
                self.repo_id,
                repo_id
            ))
        }
    }

    fn from_entry(entry: BonsaiSvnrevMappingEntry, repo_id: RepositoryId) -> Self {
        BonsaiSvnrevMappingCacheEntry {
            repo_id,
            bcs_id: entry.bcs_id,
            svnrev: entry.svnrev,
        }
    }
}

pub struct CachingBonsaiSvnrevMapping {
    cachelib: CachelibHandler<BonsaiSvnrevMappingCacheEntry>,
    memcache: MemcacheHandler,
    keygen: KeyGen,
    inner: Arc<dyn BonsaiSvnrevMapping>,
}

impl CachingBonsaiSvnrevMapping {
    pub fn new(
        inner: Arc<dyn BonsaiSvnrevMapping>,
        cache_handler_factory: CacheHandlerFactory,
    ) -> Self {
        Self {
            inner,
            cachelib: cache_handler_factory.cachelib(),
            memcache: cache_handler_factory.memcache(),
            keygen: Self::create_key_gen(),
        }
    }

    pub fn new_test(inner: Arc<dyn BonsaiSvnrevMapping>) -> Self {
        Self::new(inner, CacheHandlerFactory::Mocked)
    }

    fn create_key_gen() -> KeyGen {
        let key_prefix = "scm.mononoke.bonsai_svnrev_mapping";

        KeyGen::new(
            key_prefix,
            thrift::MC_CODEVER as u32,
            thrift::MC_SITEVER as u32,
        )
    }

    pub fn cachelib(&self) -> &CachelibHandler<BonsaiSvnrevMappingCacheEntry> {
        &self.cachelib
    }
}

#[async_trait]
impl BonsaiSvnrevMapping for CachingBonsaiSvnrevMapping {
    fn repo_id(&self) -> RepositoryId {
        self.inner.as_ref().repo_id()
    }

    async fn bulk_import(
        &self,
        ctx: &CoreContext,
        entries: &[BonsaiSvnrevMappingEntry],
    ) -> Result<(), Error> {
        self.inner.as_ref().bulk_import(ctx, entries).await
    }

    async fn get(
        &self,
        ctx: &CoreContext,
        objects: BonsaisOrSvnrevs,
    ) -> Result<Vec<BonsaiSvnrevMappingEntry>, Error> {
        let cache_request = (ctx, self);
        let repo_id = self.repo_id();

        let res = match objects {
            BonsaisOrSvnrevs::Bonsai(cs_ids) => {
                get_or_fill(&cache_request, cs_ids.into_iter().collect())
                    .await
                    .with_context(|| "Error fetching svnrevs via cache")?
                    .into_values()
                    .map(|val| val.into_entry(repo_id))
                    .collect::<Result<_>>()?
            }
            BonsaisOrSvnrevs::Svnrev(svnrevs) => {
                get_or_fill(&cache_request, svnrevs.into_iter().collect())
                    .await
                    .with_context(|| "Error fetching bonsais via cache")?
                    .into_values()
                    .map(|val| val.into_entry(repo_id))
                    .collect::<Result<_>>()?
            }
        };

        Ok(res)
    }
}

impl MemcacheEntity for BonsaiSvnrevMappingCacheEntry {
    fn serialize(&self) -> Bytes {
        let entry = thrift::BonsaiSvnrevMappingEntry {
            repo_id: self.repo_id.id(),
            bcs_id: self.bcs_id.into_thrift(),
            svnrev: self
                .svnrev
                .id()
                .try_into()
                .expect("Svnrevs must fit within a i64"),
        };
        compact_protocol::serialize(&entry)
    }

    fn deserialize(bytes: Bytes) -> McResult<Self> {
        let thrift::BonsaiSvnrevMappingEntry {
            repo_id,
            bcs_id,
            svnrev,
        } = compact_protocol::deserialize(bytes).map_err(|_| McErrorKind::Deserialization)?;

        let repo_id = RepositoryId::new(repo_id);
        let bcs_id = ChangesetId::from_thrift(bcs_id).map_err(|_| McErrorKind::Deserialization)?;
        let svnrev = Svnrev::new(
            svnrev
                .try_into()
                .map_err(|_| McErrorKind::Deserialization)?,
        );

        Ok(BonsaiSvnrevMappingCacheEntry {
            repo_id,
            bcs_id,
            svnrev,
        })
    }
}

type CacheRequest<'a> = (&'a CoreContext, &'a CachingBonsaiSvnrevMapping);

impl EntityStore<BonsaiSvnrevMappingCacheEntry> for CacheRequest<'_> {
    fn cachelib(&self) -> &CachelibHandler<BonsaiSvnrevMappingCacheEntry> {
        let (_, mapping) = self;
        &mapping.cachelib
    }

    fn keygen(&self) -> &KeyGen {
        let (_, mapping) = self;
        &mapping.keygen
    }

    fn memcache(&self) -> &MemcacheHandler {
        let (_, mapping) = self;
        &mapping.memcache
    }

    fn cache_determinator(&self, _: &BonsaiSvnrevMappingCacheEntry) -> CacheDisposition {
        CacheDisposition::Cache(CacheTtl::NoTtl)
    }

    caching_ext::impl_singleton_stats!("bonsai_svnrev_mapping");
}

#[async_trait]
impl KeyedEntityStore<ChangesetId, BonsaiSvnrevMappingCacheEntry> for CacheRequest<'_> {
    fn get_cache_key(&self, key: &ChangesetId) -> String {
        let (_, mapping) = self;
        format!("{}.bonsai.{}", mapping.repo_id(), key)
    }

    async fn get_from_db(
        &self,
        keys: HashSet<ChangesetId>,
    ) -> Result<HashMap<ChangesetId, BonsaiSvnrevMappingCacheEntry>, Error> {
        let (ctx, mapping) = self;
        let repo_id = mapping.repo_id();

        let res = mapping
            .inner
            .as_ref()
            .get(ctx, BonsaisOrSvnrevs::Bonsai(keys.into_iter().collect()))
            .await
            .with_context(|| "Error fetching svnrevs from bonsais from SQL")?;

        Result::<_, Error>::Ok(
            res.into_iter()
                .map(|e| {
                    (
                        e.bcs_id,
                        BonsaiSvnrevMappingCacheEntry::from_entry(e, repo_id),
                    )
                })
                .collect(),
        )
    }
}

#[async_trait]
impl KeyedEntityStore<Svnrev, BonsaiSvnrevMappingCacheEntry> for CacheRequest<'_> {
    fn get_cache_key(&self, key: &Svnrev) -> String {
        let (_, mapping) = self;
        format!("{}.svnrev.{}", mapping.repo_id(), key.id())
    }

    async fn get_from_db(
        &self,
        keys: HashSet<Svnrev>,
    ) -> Result<HashMap<Svnrev, BonsaiSvnrevMappingCacheEntry>, Error> {
        let (ctx, mapping) = self;
        let repo_id = mapping.repo_id();

        let res = mapping
            .inner
            .as_ref()
            .get(ctx, BonsaisOrSvnrevs::Svnrev(keys.into_iter().collect()))
            .await
            .with_context(|| "Error fetching bonsais from svnrevs from SQL")?;

        Result::<_, Error>::Ok(
            res.into_iter()
                .map(|e| {
                    (
                        e.svnrev,
                        BonsaiSvnrevMappingCacheEntry::from_entry(e, repo_id),
                    )
                })
                .collect(),
        )
    }
}
