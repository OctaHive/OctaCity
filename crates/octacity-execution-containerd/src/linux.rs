//! Linux implementation of the containerd OCI process engine.

use std::{
  collections::HashMap,
  ffi::CString,
  fs::{self, File, OpenOptions},
  future::Future,
  os::unix::{
    ffi::OsStrExt as _,
    fs::{FileTypeExt as _, MetadataExt as _, OpenOptionsExt as _},
    io::AsRawFd as _,
  },
  path::{Path, PathBuf},
  time::Duration,
};

use async_trait::async_trait;
use containerd_client::{
  Client,
  services::v1::{
    CloseIoRequest, Container, CreateContainerRequest, CreateTaskRequest, DeleteContainerRequest, DeleteTaskRequest,
    GetImageRequest, KillRequest, ListContainersRequest, MetricsRequest, PluginsRequest, StartRequest, TransferOptions,
    TransferRequest, WaitRequest,
    container::Runtime,
    snapshots::{ListSnapshotsRequest, PrepareSnapshotRequest, RemoveSnapshotRequest},
  },
  to_any,
  tonic::{Code, Request, Status, metadata::MetadataValue, transport::Channel},
  types::{
    Platform,
    transfer::{ImageStore, OciRegistry, UnpackConfiguration},
  },
};
use octacity_execution::{
  ExecutionArchitecture, ExecutionError, ExecutionExit, ExecutionIo, ExecutionOs, ExecutionPaths, ExecutionPlatform,
  ExecutionReader, ExecutionTarget, ExecutionWriter, NetworkAccess, OciIsolation, ResourceUsage, RunnerProgram,
  RunningExecution, StartExecution,
};
use octacity_execution_oci::{OciCapability, OciEngine};
use prost::Message;
use prost_types::Any;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use tokio::time::{Instant, timeout, timeout_at};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::ContainerdEngineConfig;

#[path = "image.rs"]
mod image;

use image::{image_configuration, pull_and_unpack};

#[path = "client.rs"]
mod client;
#[path = "execution.rs"]
mod execution;
#[path = "filesystem.rs"]
mod filesystem;
#[path = "io.rs"]
mod io;
#[path = "metrics.rs"]
mod metrics;
#[path = "spec.rs"]
mod spec;

use client::{before_deadline, elapsed_millis, grpc, grpc_before, namespaced, operation_deadline};
use execution::{PreparedExecution, destroy_resources, start_prepared};
#[cfg(test)]
use filesystem::agent_resource_prefix;
use filesystem::{
  canonical_directory, create_fifo, filesystem_usage, map_path, remove_abandoned_io_directories, resource_id,
  set_private_permissions, validate_work_root, validate_workspace_filesystem,
};
#[cfg(test)]
use image::{
  ImagePlatform, chain_id, containerd_platform, execution_architecture, execution_os, is_index_media_type,
  is_manifest_media_type, process_environment, validate_digest,
};
use io::ContainerIo;
use metrics::decode_cgroup_v2_metrics;
#[cfg(test)]
use metrics::{CgroupV2Metrics, CpuStat, IoEntry, IoStat, MemoryStat};
use spec::{guest_paths, oci_spec};

const OWNER_LABEL: &str = "octacity.agent";
const EXECUTION_LABEL: &str = "octacity.execution";
const MAX_IMAGE_METADATA_BYTES: usize = 8 * 1024 * 1024;
const SECURITY_PROFILE_VERSION: &str = "containerd-process-v1";
const OCI_SPEC_TYPE_URL: &str = "types.containerd.io/opencontainers/runtime-spec/1/Spec";
const CGROUP_V2_METRICS_SUFFIX: &str = "io.containerd.cgroups.v2.Metrics";
const GUEST_WORKSPACE: &str = "/workspace";
const GUEST_RELEASE: &str = "/opt/octa";

/// OCI process engine backed by a configured containerd daemon.
pub struct ContainerdEngine {
  config: ContainerdEngineConfig,
  capability: OciCapability,
  io_root: PathBuf,
}

impl ContainerdEngine {
  /// Validates operator-owned paths and policy before any daemon resources are
  /// created. Daemon capabilities are checked separately at agent startup.
  pub fn new(mut config: ContainerdEngineConfig) -> Result<Self, ExecutionError> {
    validate_identifier("agent_id", &config.agent_id)?;
    validate_identifier("containerd namespace", &config.namespace)?;
    validate_identifier("containerd snapshotter", &config.snapshotter)?;
    validate_identifier("containerd runtime", &config.runtime)?;
    if !config.endpoint.is_absolute() {
      return Err(invalid("containerd endpoint must be an absolute path"));
    }
    if !fs::metadata(&config.endpoint)
      .map_err(|error| {
        unavailable(format!(
          "inspect containerd endpoint '{}': {error}",
          config.endpoint.display()
        ))
      })?
      .file_type()
      .is_socket()
    {
      return Err(invalid("containerd endpoint must be a Unix socket"));
    }
    if config.max_workspace_bytes == 0
      || config.cleanup_timeout.is_zero()
      || config.pids_limit == 0
      || config.open_files_limit == 0
    {
      return Err(invalid(
        "containerd workspace, cleanup, process, and open-file limits must be greater than zero",
      ));
    }
    config.state_root = canonical_directory("state_root", &config.state_root)?;
    config.work_root = canonical_directory("work_root", &config.work_root)?;
    validate_work_root(&config.work_root, config.max_workspace_bytes)?;
    if let Some(path) = &config.registry_config_dir {
      config.registry_config_dir = Some(canonical_directory("containerd registry config", path)?);
    }
    let io_root = config.state_root.join("containerd").join("io");
    fs::create_dir_all(&io_root).map_err(ExecutionError::Io)?;
    set_private_permissions(&io_root)?;
    Ok(Self {
      config,
      capability: host_capability()?,
      io_root,
    })
  }

  async fn connect(
    &self,
    deadline: Instant,
    cancellation: Option<&CancellationToken>,
  ) -> Result<Client, ExecutionError> {
    before_deadline(
      deadline,
      cancellation,
      "connect to containerd",
      Client::from_path(&self.config.endpoint),
    )
    .await?
    .map_err(|error| {
      unavailable(format!(
        "connect to containerd '{}': {error}",
        self.config.endpoint.display()
      ))
    })
  }

  /// Verifies daemon reachability and the configured snapshotter before the
  /// agent advertises this engine's capability.
  pub async fn validate_connection(&self) -> Result<(), ExecutionError> {
    let deadline = operation_deadline(self.config.cleanup_timeout)?;
    let client = self.connect(deadline, None).await?;
    grpc_before(
      deadline,
      None,
      "query containerd version",
      client.version().version(namespaced((), &self.config.namespace)?),
    )
    .await?;
    let response = grpc_before(
      deadline,
      None,
      "query containerd plugins",
      client.introspection().plugins(namespaced(
        PluginsRequest { filters: Vec::new() },
        &self.config.namespace,
      )?),
    )
    .await?
    .into_inner();
    match response
      .plugins
      .into_iter()
      .find(|plugin| plugin.r#type == "io.containerd.snapshotter.v1" && plugin.id == self.config.snapshotter)
    {
      Some(plugin) if plugin.init_err.is_none() => Ok(()),
      Some(_) => Err(unavailable(format!(
        "containerd snapshotter '{}' failed to initialize",
        self.config.snapshotter
      ))),
      None => Err(unavailable(format!(
        "containerd snapshotter '{}' is not installed",
        self.config.snapshotter
      ))),
    }
  }
}

#[async_trait]
impl OciEngine for ContainerdEngine {
  fn capabilities(&self) -> Vec<OciCapability> {
    vec![self.capability]
  }

  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    if cancellation.is_cancelled() {
      return Err(ExecutionError::Cancelled);
    }
    runner.validate()?;
    request.validate()?;
    validate_request(&self.config, self.capability, &request)?;

    // One absolute deadline covers connection, image resolution, snapshot
    // preparation, and task startup; slow setup must not steal runtime beyond
    // the duration signed into the job.
    let deadline = operation_deadline(request.max_duration)?;
    let client = self.connect(deadline, Some(&cancellation)).await?;
    let ExecutionTarget::Oci { reference, .. } = &request.root else {
      return Err(invalid("containerd requires an OCI execution target"));
    };
    pull_and_unpack(
      &client,
      &self.config,
      self.capability.platform,
      reference,
      deadline,
      &cancellation,
    )
    .await?;
    let image = image_configuration(
      &client,
      &self.config.namespace,
      reference,
      self.capability.platform,
      deadline,
      &cancellation,
    )
    .await?;

    let id = resource_id(&self.config.agent_id, &request.execution_id);
    let snapshot_key = format!("{id}-rootfs");
    // Ownership labels are the only resources cleanup_orphans may reclaim;
    // resources belonging to another agent or namespace remain untouched.
    let labels = HashMap::from([
      (OWNER_LABEL.to_owned(), self.config.agent_id.clone()),
      (EXECUTION_LABEL.to_owned(), request.execution_id.clone()),
    ]);
    let prepared = grpc_before(
      deadline,
      Some(&cancellation),
      "prepare containerd snapshot",
      client.snapshots().prepare(namespaced(
        PrepareSnapshotRequest {
          snapshotter: self.config.snapshotter.clone(),
          key: snapshot_key.clone(),
          parent: image.chain_id,
          labels: labels.clone(),
        },
        &self.config.namespace,
      )?),
    )
    .await;
    let io_directory = self.io_root.join(&id);
    let result = match prepared {
      Ok(prepared) => {
        start_prepared(PreparedExecution {
          client: &client,
          config: &self.config,
          runner,
          request: &request,
          id: &id,
          snapshot_key: &snapshot_key,
          labels,
          rootfs: prepared.into_inner().mounts,
          image_environment: &image.environment,
          io_directory: &io_directory,
          deadline,
          cancellation: &cancellation,
        })
        .await
      }
      Err(error) => Err(error),
    };
    match result {
      Ok(execution) => Ok(Box::new(execution)),
      Err(error) => {
        // Snapshot preparation is the first mutating daemon call. Every later
        // startup failure therefore rolls the complete resource set back.
        match cleanup_resources(
          &client,
          &self.config,
          &id,
          &snapshot_key,
          &io_directory,
          self.config.cleanup_timeout,
        )
        .await
        {
          Ok(()) => Err(error),
          Err(cleanup) => Err(error.with_cleanup(cleanup)),
        }
      }
    }
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    let deadline = operation_deadline(self.config.cleanup_timeout)?;
    let client = self.connect(deadline, None).await?;
    let response = grpc_before(
      deadline,
      None,
      "list containerd containers",
      client.containers().list(namespaced(
        ListContainersRequest { filters: Vec::new() },
        &self.config.namespace,
      )?),
    )
    .await?
    .into_inner();
    let mut failures = Vec::new();
    // Container deletion normally removes task state; the subsequent snapshot
    // pass also catches crashes between snapshot creation and container create.
    for container in response.containers {
      if container.labels.get(OWNER_LABEL) != Some(&self.config.agent_id) {
        continue;
      }
      let snapshot_key = container.snapshot_key.clone();
      let io_directory = self.io_root.join(&container.id);
      if let Err(error) = destroy_resources(
        &client,
        &self.config.namespace,
        &self.config.snapshotter,
        &container.id,
        &snapshot_key,
        &io_directory,
        self.config.cleanup_timeout,
      )
      .await
      {
        failures.push(error.to_string());
      }
    }
    let mut snapshots = grpc_before(
      deadline,
      None,
      "list containerd snapshots",
      client.snapshots().list(namespaced(
        ListSnapshotsRequest {
          snapshotter: self.config.snapshotter.clone(),
          filters: Vec::new(),
        },
        &self.config.namespace,
      )?),
    )
    .await?
    .into_inner();
    while let Some(batch) = grpc_before(deadline, None, "stream containerd snapshots", snapshots.message()).await? {
      for snapshot in batch.info {
        if snapshot.labels.get(OWNER_LABEL) != Some(&self.config.agent_id) {
          continue;
        }
        let removal = timeout_at(
          deadline,
          client.snapshots().remove(namespaced(
            RemoveSnapshotRequest {
              snapshotter: self.config.snapshotter.clone(),
              key: snapshot.name,
            },
            &self.config.namespace,
          )?),
        )
        .await;
        match removal {
          Ok(Ok(_)) => {}
          Ok(Err(error)) if error.code() == Code::NotFound => {}
          Ok(Err(error)) => failures.push(grpc("remove orphaned containerd snapshot", error).to_string()),
          Err(_) => failures.push("remove orphaned containerd snapshot timed out".to_owned()),
        }
      }
    }
    remove_abandoned_io_directories(&self.io_root, &self.config.agent_id)?;
    if failures.is_empty() {
      Ok(())
    } else {
      Err(ExecutionError::Backend(failures.join("; ")))
    }
  }
}

async fn cleanup_resources(
  client: &Client,
  config: &ContainerdEngineConfig,
  id: &str,
  snapshot_key: &str,
  io_directory: &Path,
  cleanup_timeout: Duration,
) -> Result<(), ExecutionError> {
  let result = destroy_resources(
    client,
    &config.namespace,
    &config.snapshotter,
    id,
    snapshot_key,
    io_directory,
    cleanup_timeout,
  )
  .await;
  if let Err(error) = &result {
    warn!(container_id = id, %error, "failed to roll back containerd execution startup");
  }
  result
}

fn validate_request(
  config: &ContainerdEngineConfig,
  capability: OciCapability,
  request: &StartExecution,
) -> Result<(), ExecutionError> {
  let ExecutionTarget::Oci {
    platform, isolation, ..
  } = request.root
  else {
    return Err(invalid("containerd requires an OCI execution target"));
  };
  if (OciCapability { platform, isolation }) != capability {
    return Err(unavailable(format!(
      "containerd engine provides {capability:?}, not {:?}",
      OciCapability { platform, isolation }
    )));
  }
  if request.network != NetworkAccess::Disabled {
    return Err(unavailable(
      "containerd process engine currently enforces only disabled job networking",
    ));
  }
  if request.cpu_millis < 10 {
    return Err(unavailable("containerd CPU allocation must be at least 10 millicpu"));
  }
  if request.memory_bytes < 8 * 1024 * 1024 {
    return Err(unavailable("containerd memory allocation must be at least 8 MiB"));
  }
  let workspace_root = request.workspace_root.canonicalize().map_err(ExecutionError::Io)?;
  if workspace_root != config.work_root {
    return Err(invalid("execution workspace_root does not match containerd work_root"));
  }
  validate_workspace_filesystem(&config.work_root, &request.workspace, request.writable_disk_bytes)?;
  Ok(())
}

fn host_capability() -> Result<OciCapability, ExecutionError> {
  let architecture = match std::env::consts::ARCH {
    "x86_64" => ExecutionArchitecture::Amd64,
    "aarch64" => ExecutionArchitecture::Arm64,
    architecture => {
      return Err(unavailable(format!(
        "containerd does not support host architecture '{architecture}'"
      )));
    }
  };
  Ok(OciCapability {
    platform: ExecutionPlatform {
      os: ExecutionOs::Linux,
      architecture,
    },
    isolation: OciIsolation::Process,
  })
}

fn validate_identifier(name: &str, value: &str) -> Result<(), ExecutionError> {
  if value.is_empty()
    || value.len() > 128
    || !value
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
  {
    return Err(invalid(format!("{name} contains unsupported characters")));
  }
  Ok(())
}

fn invalid(message: impl Into<String>) -> ExecutionError {
  ExecutionError::Invalid(message.into())
}

fn unavailable(message: impl Into<String>) -> ExecutionError {
  ExecutionError::Unavailable(message.into())
}

fn backend(message: impl Into<String>) -> ExecutionError {
  ExecutionError::Backend(message.into())
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
