/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::sync::Arc;

use anyhow::Result;
use cas_client::CasClient;
use configmodel::Config;
use configmodel::ConfigExt;
use re_client_lib::create_default_config;
use re_client_lib::ExternalCASDaemonAddress;
use re_client_lib::REClient;
use re_client_lib::REClientBuilder;
use re_client_lib::RemoteExecutionMetadata;

pub struct ThinCasClient {
    client: re_cas_common::OnceCell<REClient>,
    metadata: RemoteExecutionMetadata,
    connection_count: u32,
    port: Option<i32>,
    uds_path: Option<String>,
    verbose: bool,
}

pub fn init() {
    fn construct(config: &dyn Config) -> Result<Option<Arc<dyn CasClient>>> {
        // Kill switch in case something unexpected happens during construction of client.
        if config.get_or_default("cas", "disable")? {
            tracing::warn!(target: "cas", "disabled (cas.disable=true)");
            return Ok(None);
        }

        tracing::debug!(target: "cas", "creating thin client");
        ThinCasClient::from_config(config).map(|c| Some(Arc::new(c) as Arc<dyn CasClient>))
    }
    factory::register_constructor("thin-client", construct);
}

impl ThinCasClient {
    pub fn from_config(config: &dyn Config) -> Result<Self> {
        let use_case: String = match config.get("cas", "use-case") {
            Some(use_case) => use_case.to_string(),
            None => format!(
                "source-control-{}",
                config.must_get::<String>("remotefilelog", "reponame")?
            ),
        };

        Ok(Self {
            client: Default::default(),
            connection_count: config.get_or("cas", "connection-count", || 1)?,
            port: config.get_opt::<i32>("cas", "port")?,
            uds_path: config.get_opt("cas", "uds-path")?,
            verbose: config.get_or_default("cas", "verbose")?,
            metadata: RemoteExecutionMetadata {
                use_case_id: use_case,
                ..Default::default()
            },
        })
    }

    fn build(&self) -> Result<REClient> {
        let mut re_config = create_default_config();

        re_config.client_name = Some("sapling".to_string());
        re_config.quiet_mode = !self.verbose;
        re_config.features_config_path = "remote_execution/features/client_sapling".to_string();

        let mut builder = REClientBuilder::new(fbinit::expect_init()).with_config(re_config);

        if let Some(port) = self.port {
            builder = builder
                .with_cas_daemon(ExternalCASDaemonAddress::port(port), self.connection_count);
        } else if let Some(uds_path) = self.uds_path.clone() {
            builder = builder.with_cas_daemon(
                ExternalCASDaemonAddress::uds_path(uds_path),
                self.connection_count,
            );
        } else {
            builder = builder.with_wdb_cas_daemon(self.connection_count);
        }

        builder.build()
    }
}

re_cas_common::re_client!(ThinCasClient);
