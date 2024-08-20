/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#![feature(never_type)]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use cache_warmup::cache_warmup;
use cache_warmup::CacheWarmupKind;
use clap::Parser;
use cloned::cloned;
use cmdlib_logging::ScribeLoggingArgs;
use environment::BookmarkCacheDerivedData;
use environment::BookmarkCacheKind;
use environment::BookmarkCacheOptions;
use executor_lib::args::ShardedExecutorArgs;
use executor_lib::RepoShardedProcess;
use executor_lib::RepoShardedProcessExecutor;
use fbinit::FacebookInit;
use futures::channel::oneshot;
use futures::stream;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;
use metaconfig_types::RepoConfigRef;
use metaconfig_types::ShardedService;
use mononoke_api::CoreContext;
use mononoke_api::Repo;
use mononoke_app::args::HooksAppExtension;
use mononoke_app::args::McrouterAppExtension;
use mononoke_app::args::ReadonlyArgs;
use mononoke_app::args::RepoFilterAppExtension;
use mononoke_app::args::ShutdownTimeoutArgs;
use mononoke_app::args::TLSArgs;
use mononoke_app::args::WarmBookmarksCacheExtension;
use mononoke_app::fb303::Fb303AppExtension;
use mononoke_app::fb303::ReadyFlagService;
use mononoke_app::MononokeApp;
use mononoke_app::MononokeAppBuilder;
use mononoke_app::MononokeReposManager;
use openssl::ssl::AlpnError;
use repo_identity::RepoIdentityRef;
use scuba_ext::MononokeScubaSampleBuilder;
use sharding_ext::RepoShard;
use slog::error;
use slog::info;
use slog::o;
use slog::warn;
use slog::Logger;

const SM_CLEANUP_TIMEOUT_SECS: u64 = 120;

// We will select the first protocol supported by the server which is also supported by the client.
// Order of preferences: hgcli, h2, http/1.1.
pub const ALPN_MONONOKE_PROTOS_OFFERS: &[u8] = b"\x05hgcli\x02h2\x08http/1.1";

/// Mononoke Server
#[derive(Parser)]
struct MononokeServerArgs {
    #[clap(flatten)]
    shutdown_timeout_args: ShutdownTimeoutArgs,
    #[clap(flatten)]
    scribe_logging_args: ScribeLoggingArgs,
    /// TCP address to listen to in format `host:port
    #[clap(long)]
    listening_host_port: String,
    /// Path for file in which to write the bound tcp address in rust std::net::SocketAddr format
    #[clap(long)]
    bound_address_file: Option<PathBuf>,
    /// If provided the thrift server will start on this port
    #[clap(long, short = 'p')]
    thrift_port: Option<String>,
    /// TLS parameters for this service
    #[clap(flatten)]
    tls_args: TLSArgs,
    /// Top level Mononoke tier where CSLB publishes routing table
    #[clap(long)]
    cslb_config: Option<String>,
    #[clap(flatten)]
    readonly: ReadonlyArgs,
    #[clap(flatten)]
    sharded_executor_args: ShardedExecutorArgs,
    /// Path to a file with land service client certificate
    #[clap(long)]
    land_service_client_cert: Option<String>,
    /// Path to a file with land service client private key
    #[clap(long, requires = "land_service_client_cert")]
    land_service_client_private_key: Option<String>,
}

/// Struct representing the Mononoke server process when sharding by repo.
pub struct MononokeServerProcess {
    fb: FacebookInit,
    scuba: MononokeScubaSampleBuilder,
    repos_mgr: Arc<MononokeReposManager<Repo>>,
}

impl MononokeServerProcess {
    fn new(
        fb: FacebookInit,
        repos_mgr: MononokeReposManager<Repo>,
        scuba: MononokeScubaSampleBuilder,
    ) -> Self {
        let repos_mgr = Arc::new(repos_mgr);
        Self {
            fb,
            repos_mgr,
            scuba,
        }
    }

    async fn add_repo(
        &self,
        repo_name: &str,
        logger: &Logger,
        scuba: &MononokeScubaSampleBuilder,
    ) -> Result<()> {
        // Check if the input repo is already initialized. This can happen if the repo is a
        // shallow-sharded repo, in which case it would already be initialized during service startup.
        if self.repos_mgr.repos().get_by_name(repo_name).is_none() {
            // The input repo is a deep-sharded repo, so it needs to be added now.
            let repo = self.repos_mgr.add_repo(repo_name).await?;
            let cache_warmup_params = repo.repo_config().cache_warmup.clone();
            let ctx =
                CoreContext::new_with_logger_and_scuba(self.fb, logger.clone(), scuba.clone());
            cache_warmup(
                &ctx,
                &repo,
                cache_warmup_params,
                CacheWarmupKind::MononokeServer,
            )
            .await
            .with_context(|| format!("Error while warming up cache for repo {}", repo_name))?;
            info!(
                &logger,
                "Completed repo {} setup in Mononoke service", repo_name
            );
        } else {
            info!(
                &logger,
                "Repo {} is already setup in Mononoke service", repo_name
            );
        }
        Ok(())
    }
}

#[async_trait]
impl RepoShardedProcess for MononokeServerProcess {
    async fn setup(&self, repo: &RepoShard) -> anyhow::Result<Arc<dyn RepoShardedProcessExecutor>> {
        let repo_name = repo.repo_name.as_str();
        let logger = self.repos_mgr.repo_logger(repo_name);
        info!(&logger, "Setting up repo {} in Mononoke service", repo_name);
        self.add_repo(repo_name, &logger, &self.scuba)
            .await
            .with_context(|| {
                format!(
                    "Failure in setting up repo {} in Mononoke service",
                    repo_name
                )
            })?;
        Ok(Arc::new(MononokeServerProcessExecutor {
            repo_name: repo_name.to_string(),
            repos_mgr: self.repos_mgr.clone(),
        }))
    }
}

/// Struct representing the execution of the Mononoke server for a particular
/// repo when sharding by repo.
pub struct MononokeServerProcessExecutor {
    repo_name: String,
    repos_mgr: Arc<MononokeReposManager<Repo>>,
}

impl MononokeServerProcessExecutor {
    fn remove_repo(&self, repo_name: &str) -> Result<()> {
        let config = self.repos_mgr.repo_config(repo_name).with_context(|| {
            format!(
                "Failure in remove repo {}. The config for repo doesn't exist",
                repo_name
            )
        })?;
        // Check if the current repo is a deep-sharded or shallow-sharded repo. If the
        // repo is deep-sharded, then remove it since SM wants some other host to serve it.
        // If repo is shallow-sharded, then keep it since regardless of SM sharding, shallow
        // sharded repos need to be present on each host.
        let is_deep_sharded = config
            .deep_sharding_config
            .and_then(|c| c.status.get(&ShardedService::SaplingRemoteApi).copied())
            .unwrap_or(false);
        if is_deep_sharded {
            self.repos_mgr.remove_repo(repo_name);
            info!(
                self.repos_mgr.logger(),
                "No longer serving repo {} in Mononoke service.", repo_name,
            );
        } else {
            info!(
                self.repos_mgr.logger(),
                "Continuing serving repo {} in Mononoke service because it's shallow-sharded.",
                repo_name,
            );
        }
        Ok(())
    }
}

#[async_trait]
impl RepoShardedProcessExecutor for MononokeServerProcessExecutor {
    async fn execute(&self) -> anyhow::Result<()> {
        info!(
            self.repos_mgr.logger(),
            "Serving repo {} in Mononoke service", &self.repo_name,
        );
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.remove_repo(&self.repo_name)
            .with_context(|| format!("Failure in stopping repo {}", &self.repo_name))
    }
}

#[fbinit::main]
fn main(fb: FacebookInit) -> Result<()> {
    let app = MononokeAppBuilder::new(fb)
        .with_default_scuba_dataset("mononoke_test_perf")
        .with_bookmarks_cache(BookmarkCacheOptions {
            cache_kind: BookmarkCacheKind::Local,
            derived_data: BookmarkCacheDerivedData::HgOnly,
        })
        .with_app_extension(WarmBookmarksCacheExtension {})
        .with_app_extension(McrouterAppExtension {})
        .with_app_extension(Fb303AppExtension {})
        .with_app_extension(HooksAppExtension {})
        .with_app_extension(RepoFilterAppExtension {})
        .build::<MononokeServerArgs>()?;
    let args: MononokeServerArgs = app.args()?;

    let root_log = app.logger().clone();
    let runtime = app.runtime().clone();

    let cslb_config = args.cslb_config.clone();
    info!(root_log, "Starting up");

    #[cfg(fbcode_build)]
    if let (Some(land_service_cert_path), Some(land_service_key_path)) = (
        &args.land_service_client_cert,
        &args.land_service_client_private_key,
    ) {
        pushrebase_client::land_service_override_certificate_paths(
            land_service_cert_path,
            land_service_key_path,
            &args.tls_args.tls_ca,
        );
    }

    let configs = app.repo_configs();

    let acceptor = {
        let mut builder = secure_utils::SslConfig::new(
            &args.tls_args.tls_ca,
            &args.tls_args.tls_certificate,
            &args.tls_args.tls_private_key,
            args.tls_args.tls_ticket_seeds,
        )
        .tls_acceptor_builder(root_log.clone())
        .context("Failed to instantiate TLS Acceptor builder")?;

        builder.set_alpn_protos(ALPN_MONONOKE_PROTOS_OFFERS)?;
        builder.set_alpn_select_callback(|_, list| {
            openssl::ssl::select_next_proto(ALPN_MONONOKE_PROTOS_OFFERS, list)
                .ok_or(AlpnError::NOACK)
        });

        if args.tls_args.disable_mtls {
            warn!(root_log, "MTLS has been disabled!");
            builder.set_verify(openssl::ssl::SslVerifyMode::NONE)
        }

        builder.build()
    };

    info!(root_log, "Creating repo listeners");

    let scribe = args.scribe_logging_args.get_scribe(fb)?;
    let host_port = args.listening_host_port;
    let bound_addr_file = args.bound_address_file;

    let service = ReadyFlagService::new();
    let (terminate_sender, terminate_receiver) = oneshot::channel::<()>();
    let will_exit = Arc::new(AtomicBool::new(false));

    let env = app.environment();
    let scuba = env.scuba_sample_builder.clone();
    // Service name is used for shallow or deep sharding. If sharding itself is disabled, provide
    // service name as None while opening repos.
    let service_name = args
        .sharded_executor_args
        .sharded_service_name
        .as_ref()
        .map(|_| ShardedService::SaplingRemoteApi);
    app.start_monitoring("mononoke_server", service.clone())?;
    app.start_stats_aggregation()?;

    let repo_listeners = {
        cloned!(root_log, will_exit, env, runtime, service_name);
        move |app: MononokeApp| async move {
            let common = configs.common.clone();
            let repos_mgr = app.open_managed_repos::<Repo>(service_name).await?;
            let mononoke = Arc::new(repos_mgr.make_mononoke_api()?);
            info!(&root_log, "Built Mononoke");

            info!(&root_log, "Warming up cache");
            stream::iter(mononoke.repos())
                .map(|repo| {
                    let repo_name = repo.repo_identity().name().to_string();
                    let root_log = root_log.clone();
                    let cache_warmup_params = repo.repo_config().cache_warmup.clone();
                    cloned!(scuba);
                    async move {
                        let logger = root_log.new(o!("repo" => repo_name.clone()));
                        let ctx = CoreContext::new_with_logger_and_scuba(fb, logger, scuba);
                        cache_warmup(
                            &ctx,
                            &repo,
                            cache_warmup_params,
                            CacheWarmupKind::MononokeServer,
                        )
                        .await
                        .with_context(|| {
                            format!("Error while warming up cache for repo {}", repo_name)
                        })
                    }
                })
                // Repo cache warmup can be quite expensive, let's limit to 40
                // at a time.
                .buffer_unordered(40)
                .try_collect()
                .await?;
            info!(&root_log, "Cache warmup completed");
            if let Some(mut executor) = args.sharded_executor_args.build_executor(
                app.fb,
                runtime.clone(),
                app.logger(),
                || Arc::new(MononokeServerProcess::new(app.fb, repos_mgr, scuba.clone())),
                false, // disable shard (repo) level healing
                SM_CLEANUP_TIMEOUT_SECS,
            )? {
                // The Sharded Process Executor needs to branch off and execute
                // on its own dedicated task spawned off the common tokio runtime.
                runtime.spawn({
                    let logger = app.logger().clone();
                    {
                        cloned!(will_exit);
                        async move { executor.block_and_execute(&logger, will_exit).await }
                    }
                });
            }
            repo_listener::create_repo_listeners(
                fb,
                app.configs(),
                common,
                mononoke.clone(),
                root_log,
                host_port,
                acceptor,
                service,
                terminate_receiver,
                &env.config_store,
                scribe,
                &scuba,
                will_exit,
                cslb_config,
                bound_addr_file,
                env.acl_provider.as_ref(),
                args.readonly.readonly,
                args.tls_args.disable_mtls,
            )
            .await
        }
    };

    app.run_until_terminated(
        repo_listeners,
        move || will_exit.store(true, Ordering::Relaxed),
        args.shutdown_timeout_args.shutdown_grace_period,
        async {
            if let Err(err) = terminate_sender.send(()) {
                error!(root_log, "could not send termination signal: {:?}", err);
            }
            repo_listener::wait_for_connections_closed(&root_log).await;
        },
        args.shutdown_timeout_args.shutdown_timeout,
    )
}
