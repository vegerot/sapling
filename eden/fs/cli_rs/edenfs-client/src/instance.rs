/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! EdenFsInstance - manages EdenFS resources besides Thrift connection (managed by
//! [`EdenFsClient`]).

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::Context;
use edenfs_config::EdenFsConfig;
use edenfs_error::EdenFsError;
use edenfs_error::Result;
use edenfs_error::ResultExt;
use edenfs_utils::get_executable;
#[cfg(windows)]
use edenfs_utils::strip_unc_prefix;
#[cfg(fbcode_build)]
use fbinit::expect_init;
use fbthrift_socket::SocketTransport;
#[cfg(fbcode_build)]
use thrift_streaming_clients::errors::StreamStartStatusError;
#[cfg(fbcode_build)]
use thrift_streaming_thriftclients::build_StreamingEdenService_client;
use thrift_types::edenfs::DaemonInfo;
use thrift_types::edenfs_clients::EdenService;
use thrift_types::fb303_core::fb303_status;
use thrift_types::fbthrift::binary_protocol::BinaryProtocol;
use thrift_types::fbthrift::ApplicationExceptionErrorCode;
#[cfg(fbcode_build)]
use thriftclient::ThriftChannelBuilder;
#[cfg(fbcode_build)]
use thriftclient::TransportType;
use tokio_uds_compat::UnixStream;
use tracing::event;
use tracing::Level;

use crate::EdenFsClient;
#[cfg(fbcode_build)]
use crate::StartStatusStream;
#[cfg(fbcode_build)]
use crate::StreamingEdenFsClient;

// We should create a single EdenFsInstance when parsing EdenFs commands and utilize
// EdenFsInstance::global() whenever we need to access it. This way we can avoid passing an
// EdenFsInstance through every subcommand
static INSTANCE: OnceLock<EdenFsInstance> = OnceLock::new();

/// These paths are relative to the user's client directory.
const CLIENTS_DIR: &str = "clients";
const CONFIG_JSON: &str = "config.json";

#[derive(Debug)]
pub struct EdenFsInstance {
    config_dir: PathBuf,
    etc_eden_dir: PathBuf,
    home_dir: Option<PathBuf>,
}

impl EdenFsInstance {
    pub fn global() -> &'static EdenFsInstance {
        INSTANCE.get().expect("EdenFsInstance is not initialized")
    }

    pub fn init(config_dir: PathBuf, etc_eden_dir: PathBuf, home_dir: Option<PathBuf>) {
        event!(
            Level::TRACE,
            ?config_dir,
            ?etc_eden_dir,
            ?home_dir,
            "Creating EdenFsInstance"
        );
        INSTANCE
            .set(Self {
                config_dir,
                etc_eden_dir,
                home_dir,
            })
            .expect("should be able to initialize EdenfsInstance")
    }

    pub fn get_config(&self) -> Result<EdenFsConfig> {
        edenfs_config::load_config(
            &self.etc_eden_dir,
            self.home_dir.as_ref().map(|x| x.as_ref()),
        )
    }

    pub fn get_user_home_dir(&self) -> Option<&PathBuf> {
        self.home_dir.as_ref()
    }

    async fn _connect(&self, socket_path: &PathBuf) -> Result<EdenFsClient> {
        let stream = UnixStream::connect(&socket_path)
            .await
            .map_err(EdenFsError::ThriftIoError)?;
        let transport = SocketTransport::new(stream);
        let client = <dyn EdenService>::new(BinaryProtocol, transport);

        Ok(client)
    }

    pub async fn connect(&self, timeout: Option<Duration>) -> Result<EdenFsClient> {
        let socket_path = self.config_dir.join("socket");

        let connect = self._connect(&socket_path);
        let res = if let Some(timeout) = timeout {
            tokio::time::timeout(timeout, connect)
                .await
                .map_err(|_| EdenFsError::ThriftConnectionTimeout(socket_path))?
        } else {
            connect.await
        };

        res
    }

    #[cfg(fbcode_build)]
    pub async fn _connect_streaming(&self, socket_path: &PathBuf) -> Result<StreamingEdenFsClient> {
        let client = build_StreamingEdenService_client(
            ThriftChannelBuilder::from_path(expect_init(), socket_path)?
                .with_transport_type(TransportType::Rocket)
                .with_secure(false),
        )?;
        Ok(client)
    }

    #[cfg(fbcode_build)]
    pub async fn connect_streaming(
        &self,
        timeout: Option<Duration>,
    ) -> Result<StreamingEdenFsClient> {
        let socket_path = self.config_dir.join("socket");
        let client = self._connect_streaming(&socket_path);

        if let Some(timeout) = timeout {
            tokio::time::timeout(timeout, client)
                .await
                .map_err(|_| EdenFsError::ThriftConnectionTimeout(socket_path))?
        } else {
            client.await
        }
    }

    #[cfg(windows)]
    fn pidfile(&self) -> PathBuf {
        self.config_dir.join("pid")
    }

    #[cfg(unix)]
    fn pidfile(&self) -> PathBuf {
        self.config_dir.join("lock")
    }

    /// Read the pid from the EdenFS lockfile
    fn pid(&self) -> Result<sysinfo::Pid, anyhow::Error> {
        let pidfile = self.pidfile();
        let pid_bytes = std::fs::read(&pidfile)
            .with_context(|| format!("Unable to read from pid file '{}'", pidfile.display()))?;
        let pid_str =
            std::str::from_utf8(&pid_bytes).context("Unable to parse pid file as UTF-8 string")?;

        pid_str
            .trim()
            .parse()
            .with_context(|| format!("Unable to parse pid file content: '{}'", pid_str))
    }

    /// Retrieving running EdenFS process status based on lock file
    pub fn status_from_lock(&self) -> Result<i32, anyhow::Error> {
        let pid = self.pid()?;

        let exe = match get_executable(pid) {
            Some(exe) => exe,
            None => {
                tracing::debug!("PID {} is not running", pid);
                return Err(anyhow!("EdenFS is not running"));
            }
        };
        let name = match exe.file_name() {
            Some(name) => name.to_string_lossy(),
            None => {
                tracing::debug!("Unable to retrieve information about PID {}", pid);
                return Err(anyhow!("EdenFS is not running"));
            }
        };

        tracing::trace!(?name, "executable name");

        if name == "edenfs"
            || name == "fake_edenfs"
            || (cfg!(windows) && name.ends_with("edenfs.exe"))
        {
            Err(anyhow!(
                "EdenFS's Thrift server does not appear to be running, \
                but the process is still alive (PID={})",
                pid
            ))
        } else {
            Err(anyhow!("EdenFS is not running"))
        }
    }

    pub async fn get_health(&self, timeout: Option<Duration>) -> Result<DaemonInfo> {
        let client = self
            .connect(timeout.or_else(|| Some(Duration::from_secs(3))))
            .await
            .context("Unable to connect to EdenFS daemon")?;
        event!(Level::DEBUG, "connected to EdenFS daemon");
        client.getDaemonInfo().await.from_err()
    }

    #[cfg(fbcode_build)]
    pub async fn get_health_with_startup_updates_included(
        &self,
        timeout: Duration,
    ) -> Result<(DaemonInfo, StartStatusStream)> {
        let client = self
            .connect_streaming(Some(timeout))
            .await
            .context("Unable to connect to EdenFS daemon")?;
        let result = client.streamStartStatus().await;
        match result {
            Err(StreamStartStatusError::ApplicationException(e))
                if e.type_ == ApplicationExceptionErrorCode::UnknownMethod =>
            {
                Err(EdenFsError::UnknownMethod(e.message))
            }
            r => r.from_err(),
        }
    }

    /// Returns a map of mount paths to mount names
    /// as defined in EdenFS's config.json.
    pub fn get_configured_mounts_map(&self) -> Result<BTreeMap<PathBuf, String>, anyhow::Error> {
        let directory_map = self.config_dir.join(CONFIG_JSON);
        match std::fs::read_to_string(&directory_map) {
            Ok(buff) => {
                let string_map = serde_json::from_str::<BTreeMap<String, String>>(&buff)
                    .with_context(|| format!("Failed to parse directory map: {:?}", &buff))?;
                Ok(string_map
                    .into_iter()
                    .map(|(key, val)| (key.into(), val))
                    .collect())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e)
                .with_context(|| format!("Failed to read directory map from {:?}", directory_map)),
        }
    }

    pub fn clients_dir(&self) -> PathBuf {
        self.config_dir.join(CLIENTS_DIR)
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.config_dir.join("logs")
    }

    pub fn storage_dir(&self) -> PathBuf {
        self.config_dir.join("storage")
    }

    pub fn client_name(&self, path: &Path) -> Result<String> {
        // Resolve symlinks and get absolute path
        let path = path.canonicalize().from_err()?;
        #[cfg(windows)]
        let path = strip_unc_prefix(path);

        // Find `checkout_path` that `path` is a sub path of
        let all_checkouts = self.get_configured_mounts_map()?;
        if let Some(item) = all_checkouts
            .iter()
            .find(|&(checkout_path, _)| path.starts_with(checkout_path))
        {
            let (_, checkout_name) = item;
            Ok(checkout_name.clone())
        } else {
            Err(EdenFsError::Other(anyhow!(
                "Checkout path {} is not handled by EdenFS",
                path.display()
            )))
        }
    }

    pub fn config_directory(&self, client_name: &str) -> PathBuf {
        self.clients_dir().join(client_name)
    }
}

pub trait DaemonHealthy {
    fn is_healthy(&self) -> bool;
}

impl DaemonHealthy for DaemonInfo {
    fn is_healthy(&self) -> bool {
        self.status
            .map_or_else(|| false, |val| val == fb303_status::ALIVE)
    }
}
