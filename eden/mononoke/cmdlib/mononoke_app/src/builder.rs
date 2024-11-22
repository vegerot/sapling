/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use arg_extensions::ArgDefaults;
use blobstore_factory::BlobstoreArgs;
use blobstore_factory::BlobstoreOptions;
use blobstore_factory::CachelibBlobstoreOptions;
use blobstore_factory::ChaosOptions;
use blobstore_factory::DelayOptions;
#[cfg(fbcode_build)]
use blobstore_factory::ManifoldArgs;
use blobstore_factory::PackOptions;
use blobstore_factory::ReadOnlyStorage;
use blobstore_factory::ReadOnlyStorageArgs;
use blobstore_factory::ThrottleOptions;
use cached_config::ConfigStore;
use clap::Args;
use clap::Command;
use clap::CommandFactory;
use clap::FromArgMatches;
use clientinfo::ClientEntryPoint;
use cmdlib_caching::init_cachelib;
use cmdlib_caching::CachelibArgs;
use cmdlib_caching::CachelibSettings;
use cmdlib_logging::LoggingArgs;
use cmdlib_logging::ScubaLoggingArgs;
use commit_graph_types::environment::CommitGraphArgs;
use derived_data_remote::RemoteDerivationArgs;
use environment::BookmarkCacheOptions;
use environment::MononokeEnvironment;
use fbinit::FacebookInit;
use megarepo_config::MegarepoConfigsArgs;
use megarepo_config::MononokeMegarepoConfigsOptions;
use observability::DynamicLevelDrain;
use permission_checker::AclProvider;
use permission_checker::DefaultAclProvider;
use permission_checker::InternalAclProvider;
use rendezvous::RendezVousArgs;
use slog::o;
use slog::Logger;
use slog::Never;
use slog::SendSyncRefUnwindSafeDrain;
use sql_ext::facebook::MysqlOptions;
use sql_ext::facebook::PoolConfig;
use sql_ext::facebook::ReadConnectionType;
use sql_ext::facebook::SharedConnectionPool;
use tokio::runtime::Handle;
use tokio::runtime::Runtime;

use crate::app::MononokeApp;
use crate::args::parse_config_spec_to_path;
use crate::args::AclArgs;
use crate::args::ConfigArgs;
use crate::args::GFlagsArgs;
use crate::args::JustKnobsArgs;
use crate::args::MysqlArgs;
use crate::args::RuntimeArgs;
use crate::extension::AppExtension;
use crate::extension::AppExtensionBox;
use crate::extension::BoxedAppExtension;
use crate::extension::BoxedAppExtensionArgs;

pub struct MononokeAppBuilder {
    fb: FacebookInit,
    extensions: Vec<(TypeId, Box<dyn BoxedAppExtension>)>,
    arg_defaults: Vec<Box<dyn ArgDefaults>>,
    cachelib_settings: CachelibSettings,
    default_scuba_dataset: Option<String>,
    defaults: HashMap<&'static str, String>,
    bookmark_cache_options: BookmarkCacheOptions,
    client_entry_point_for_service: ClientEntryPoint,
}

#[derive(Args, Debug)]
pub struct EnvironmentArgs {
    #[clap(flatten, next_help_heading = "CONFIG OPTIONS")]
    config_args: ConfigArgs,

    #[clap(flatten, next_help_heading = "LOGGING OPTIONS")]
    logging_args: LoggingArgs,

    #[clap(flatten, next_help_heading = "SCUBA LOGGING OPTIONS")]
    scuba_logging_args: ScubaLoggingArgs,

    #[clap(flatten, next_help_heading = "CACHELIB OPTIONS")]
    cachelib_args: CachelibArgs,

    #[clap(flatten, next_help_heading = "MYSQL OPTIONS")]
    mysql_args: MysqlArgs,

    #[clap(flatten, next_help_heading = "JUST KNOBS OPTIONS")]
    just_knobs_args: JustKnobsArgs,

    #[clap(flatten, next_help_heading = "BLOBSTORE OPTIONS")]
    blobstore_args: BlobstoreArgs,

    #[cfg(fbcode_build)]
    #[clap(flatten, next_help_heading = "MANIFOLD OPTIONS")]
    manifold_args: ManifoldArgs,

    #[clap(flatten, next_help_heading = "ACL OPTIONS")]
    acl_args: AclArgs,

    #[clap(flatten, next_help_heading = "REMOTE DERIVATION OPTIONS")]
    remote_derivation_args: RemoteDerivationArgs,

    #[clap(flatten, next_help_heading = "STORAGE OPTIONS")]
    readonly_storage_args: ReadOnlyStorageArgs,

    #[clap(flatten, next_help_heading = "RENDEZ-VOUS OPTIONS")]
    rendezvous_args: RendezVousArgs,

    #[clap(flatten, next_help_heading = "MEGAREPO OPTIONS")]
    megarepo_configs_args: MegarepoConfigsArgs,

    #[clap(flatten, next_help_heading = "GFLAGS")]
    gflags_args: GFlagsArgs,

    #[clap(flatten, next_help_heading = "COMMIT GRAPH OPTIONS")]
    commit_graph_args: CommitGraphArgs,
}

impl MononokeAppBuilder {
    pub fn new(fb: FacebookInit) -> Self {
        MononokeAppBuilder {
            fb,
            extensions: Vec::new(),
            arg_defaults: Vec::new(),
            cachelib_settings: CachelibSettings::default(),
            default_scuba_dataset: None,
            defaults: HashMap::new(),
            bookmark_cache_options: Default::default(),
            client_entry_point_for_service: Default::default(),
        }
    }

    pub fn with_arg_defaults(mut self, arg_defaults: impl ArgDefaults + 'static) -> Self {
        self.arg_defaults.push(Box::new(arg_defaults));
        self
    }

    pub fn with_default_scuba_dataset(mut self, default: impl Into<String>) -> Self {
        self.default_scuba_dataset = Some(default.into());
        self
    }

    pub fn with_bookmarks_cache(mut self, bookmark_cache_options: BookmarkCacheOptions) -> Self {
        self.bookmark_cache_options = bookmark_cache_options;
        self
    }

    /// Method for setting the service level client entry point that is not specific to any
    /// particular request
    pub fn with_entry_point(mut self, entry_point: ClientEntryPoint) -> Self {
        self.client_entry_point_for_service = entry_point;
        self
    }

    pub fn with_cachelib_settings(mut self, cachelib_settings: CachelibSettings) -> Self {
        self.cachelib_settings = cachelib_settings;
        self
    }

    pub fn with_app_extension<Ext>(mut self, ext: Ext) -> Self
    where
        Ext: AppExtension + 'static,
    {
        self.extensions
            .push((TypeId::of::<Ext>(), AppExtensionBox::new(ext)));
        self
    }

    pub fn build<AppArgs>(&mut self) -> Result<MononokeApp>
    where
        AppArgs: CommandFactory,
    {
        self.build_with_subcommands::<AppArgs>(Vec::new())
    }

    pub fn build_with_subcommands<'a, AppArgs>(
        &'a mut self,
        subcommands: Vec<Command>,
    ) -> Result<MononokeApp>
    where
        AppArgs: CommandFactory,
    {
        for (arg, default) in self.cachelib_settings.arg_defaults() {
            self.defaults.insert(arg, default);
        }

        for defaults in self.arg_defaults.iter() {
            for (arg, default) in defaults.arg_defaults() {
                self.defaults.insert(arg, default);
            }
        }

        for (_type_id, ext) in self.extensions.iter() {
            for (arg, default) in ext.arg_defaults() {
                self.defaults.insert(arg, default);
            }
        }

        let mut app = AppArgs::command();

        {
            // Save app-generated about so we can restore it.
            let about = app.get_about().cloned();
            let long_about = app.get_long_about().cloned();

            app = RuntimeArgs::augment_args_for_update(app);
            app = EnvironmentArgs::augment_args_for_update(app);
            for (_type_id, ext) in self.extensions.iter() {
                app = ext.augment_args(app);
            }

            // Adding the additional args overrode the about messages.
            // Restore them.
            if let Some(about) = about {
                app = app.about(about);
            }
            if let Some(long_about) = long_about {
                app = app.long_about(long_about);
            }
        }

        if !subcommands.is_empty() {
            app = app
                .subcommands(subcommands)
                .subcommand_required(true)
                .arg_required_else_help(true);
        }

        for (name, default) in self.defaults.iter() {
            app = app.mut_arg(name, |arg| arg.default_value(default));
        }

        let args = app.get_matches();

        let extension_args = self
            .extensions
            .iter()
            .map(|(type_id, ext)| Ok((*type_id, ext.parse_args(&args)?)))
            .collect::<Result<Vec<_>>>()?;

        let runtime_args = RuntimeArgs::from_arg_matches(&args)?;
        let runtime = create_runtime(&runtime_args)?;

        let env_args = EnvironmentArgs::from_arg_matches(&args)?;
        let config_mode = env_args.config_args.mode();
        let mut env = self.build_environment(
            &runtime,
            env_args,
            extension_args.iter().map(|(_type_id, ext)| ext.as_ref()),
        )?;

        for (_type_id, ext) in extension_args.iter() {
            ext.environment_hook(&mut env)?;
        }

        MononokeApp::new(
            self.fb,
            config_mode,
            args,
            runtime,
            env,
            extension_args.into_iter().collect(),
        )
    }

    fn build_environment<'a>(
        &self,
        runtime: &Runtime,
        env_args: EnvironmentArgs,
        extension_args: impl IntoIterator<Item = &'a dyn BoxedAppExtensionArgs> + Clone,
    ) -> Result<MononokeEnvironment> {
        let EnvironmentArgs {
            blobstore_args,
            config_args,
            logging_args,
            scuba_logging_args,
            cachelib_args,
            #[cfg(fbcode_build)]
            manifold_args,
            megarepo_configs_args,
            mysql_args,
            readonly_storage_args,
            acl_args,
            remote_derivation_args,
            rendezvous_args,
            just_knobs_args,
            gflags_args,
            commit_graph_args,
        } = env_args;

        gflags_args.propagate(self.fb)?;

        let log_level = logging_args.create_log_level();
        #[cfg(fbcode_build)]
        cmdlib_logging::glog::set_glog_log_level(self.fb, log_level)?;
        let root_log_drain = logging_args
            .create_root_log_drain(self.fb, log_level)
            .context("Failed to create root log drain")?;

        let config_store = config_args
            .create_config_store(self.fb, Logger::root(root_log_drain.clone(), o![]))
            .context("Failed to create config store")?;

        let observability_context = logging_args
            .create_observability_context(&config_store, log_level)
            .context("Failed to initialize observability context")?;

        let mut root_log_drain: Arc<dyn SendSyncRefUnwindSafeDrain<Ok = (), Err = Never>> =
            Arc::new(DynamicLevelDrain::new(
                root_log_drain,
                observability_context.clone(),
            ));
        for ext in extension_args {
            root_log_drain = ext.log_drain_hook(root_log_drain)?;
        }

        let logger = logging_args.create_logger(root_log_drain)?;

        let scuba_sample_builder = scuba_logging_args
            .create_scuba_sample_builder(
                self.fb,
                &observability_context,
                &self.default_scuba_dataset,
            )
            .context("Failed to create scuba sample builder")?;
        let warm_bookmarks_cache_scuba_sample_builder = scuba_logging_args
            .create_warm_bookmark_cache_scuba_sample_builder(self.fb)
            .context("Failed to create warm bookmark cache scuba sample builder")?;

        let caching = init_cachelib(self.fb, &self.cachelib_settings, &cachelib_args);

        let mysql_options =
            create_mysql_options(&mysql_args, create_mysql_pool_config(&mysql_args));

        let blobstore_options = create_blobstore_options(
            &blobstore_args,
            &mysql_args,
            #[cfg(fbcode_build)]
            manifold_args,
        )
        .context("Failed to parse blobstore options")?;

        let readonly_storage = ReadOnlyStorage::from_args(&readonly_storage_args);

        let rendezvous_options = rendezvous_args.into();

        let megarepo_configs_options = MononokeMegarepoConfigsOptions::from_args(
            config_args.local_configerator_path.as_deref(),
            &megarepo_configs_args,
        );

        let remote_derivation_options = remote_derivation_args.into();

        let acl_provider =
            create_acl_provider(self.fb, &acl_args).context("Failed to create ACL provider")?;

        let commit_graph_options = commit_graph_args.into();

        init_just_knobs_worker(
            &just_knobs_args,
            &config_store,
            logger.clone(),
            runtime.handle().clone(),
        )?;

        Ok(MononokeEnvironment {
            fb: self.fb,
            logger,
            scuba_sample_builder,
            warm_bookmarks_cache_scuba_sample_builder,
            config_store,
            caching,
            observability_context,
            runtime: runtime.handle().clone(),
            mysql_options,
            blobstore_options,
            readonly_storage,
            acl_provider,
            rendezvous_options,
            megarepo_configs_options,
            remote_derivation_options,
            disabled_hooks: HashMap::new(),
            bookmark_cache_options: self.bookmark_cache_options.clone(),
            filter_repos: None,
            commit_graph_options,
            client_entry_point_for_service: self.client_entry_point_for_service,
        })
    }
}

fn create_runtime(runtime_args: &RuntimeArgs) -> Result<Runtime> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    builder.thread_name("tk");
    if let Some(threads) = runtime_args.runtime_threads {
        builder.worker_threads(threads);
    }
    let runtime = builder.build()?;
    Ok(runtime)
}

fn create_mysql_options(mysql_args: &MysqlArgs, pool_config: PoolConfig) -> MysqlOptions {
    let pool = SharedConnectionPool::new();
    let read_connection_type = if mysql_args.mysql_master_only {
        ReadConnectionType::Master
    } else {
        ReadConnectionType::ReplicaOnly
    };
    MysqlOptions {
        pool,
        pool_config,
        read_connection_type,
    }
}

fn create_mysql_pool_config(mysql_args: &MysqlArgs) -> PoolConfig {
    PoolConfig::new(
        mysql_args.mysql_pool_limit,
        mysql_args.mysql_pool_threads_num,
        mysql_args.mysql_pool_per_key_limit,
        mysql_args.mysql_pool_age_timeout,
        mysql_args.mysql_pool_idle_timeout,
        mysql_args.mysql_conn_open_timeout,
        Duration::from_millis(mysql_args.mysql_query_time_limit),
    )
}

fn create_mysql_sqlblob_pool_config(mysql_args: &MysqlArgs) -> PoolConfig {
    PoolConfig::new(
        mysql_args.mysql_sqlblob_pool_limit,
        mysql_args.mysql_sqlblob_pool_threads_num,
        mysql_args.mysql_sqlblob_pool_per_key_limit,
        mysql_args.mysql_sqlblob_pool_age_timeout,
        mysql_args.mysql_sqlblob_pool_idle_timeout,
        mysql_args.mysql_conn_open_timeout,
        Duration::from_millis(mysql_args.mysql_query_time_limit),
    )
}

fn create_blobstore_options(
    blobstore_args: &BlobstoreArgs,
    mysql_args: &MysqlArgs,
    #[cfg(fbcode_build)] manifold_args: ManifoldArgs,
) -> Result<BlobstoreOptions> {
    let chaos_options = ChaosOptions::new(
        blobstore_args.blobstore_read_chaos_rate,
        blobstore_args.blobstore_write_chaos_rate,
    );

    let delay_options = DelayOptions {
        get_dist: blobstore_args.get_delay_distribution()?,
        put_dist: blobstore_args.put_delay_distribution()?,
    };

    let throttle_options = ThrottleOptions {
        read_qps: blobstore_args.blobstore_read_qps,
        write_qps: blobstore_args.blobstore_write_qps,
        read_bytes: blobstore_args.blobstore_read_bytes_s,
        write_bytes: blobstore_args.blobstore_write_bytes_s,
        read_burst_bytes: blobstore_args.blobstore_read_burst_bytes_s,
        write_burst_bytes: blobstore_args.blobstore_write_burst_bytes_s,
        bytes_min_count: blobstore_args.blobstore_bytes_min_throttle,
    };

    let pack_options = PackOptions::new(blobstore_args.put_format_override()?);

    let cachelib_blobstore_options =
        CachelibBlobstoreOptions::new_lazy(Some(blobstore_args.blobstore_cachelib_attempt_zstd));

    let blobstore_put_behaviour = blobstore_args.blobstore_put_behaviour;

    let mysql_sqlblob_options =
        create_mysql_options(mysql_args, create_mysql_sqlblob_pool_config(mysql_args));

    let blobstore_options = BlobstoreOptions::new(
        chaos_options,
        delay_options,
        throttle_options,
        #[cfg(fbcode_build)]
        manifold_args.into(),
        pack_options,
        cachelib_blobstore_options,
        blobstore_put_behaviour,
        mysql_sqlblob_options,
    );

    Ok(blobstore_options)
}

fn init_just_knobs_worker(
    just_knobs_args: &JustKnobsArgs,
    config_store: &ConfigStore,
    logger: Logger,
    handle: Handle,
) -> Result<()> {
    if let Some(just_knobs_config_path) = &just_knobs_args.just_knobs_config_path {
        let config_handle =
            config_store.get_config_handle(parse_config_spec_to_path(just_knobs_config_path)?)?;
        justknobs::cached_config::init_just_knobs_worker(logger, config_handle, handle)
    } else {
        Ok(())
    }
}

fn create_acl_provider(fb: FacebookInit, acl_args: &AclArgs) -> Result<Arc<dyn AclProvider>> {
    let acl_provider = match &acl_args.acl_file {
        Some(acl_file) => InternalAclProvider::from_file(acl_file).with_context(|| {
            format!("Failed to load ACLs from '{}'", acl_file.to_string_lossy())
        })?,
        None => DefaultAclProvider::new(fb)?,
    };
    Ok(acl_provider)
}
