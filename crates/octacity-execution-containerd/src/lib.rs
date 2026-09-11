//! Runs Linux OCI images through containerd with process isolation.
//!
//! The engine talks to containerd's stable gRPC API directly. It neither
//! invokes `ctr` nor discovers an ambient daemon. Every request uses the
//! configured namespace, snapshotter, and runtime, and every container,
//! snapshot, FIFO, and cgroup name is derived from an OctaCity-owned execution
//! identifier so startup cleanup can remain narrow and deterministic.

use std::{path::PathBuf, time::Duration};

/// Explicit operator-owned containerd settings.
#[derive(Clone, Debug)]
pub struct ContainerdEngineConfig {
  /// Stable agent owner identifier used in labels and resource names.
  pub agent_id: String,
  /// Absolute containerd Unix socket.
  pub endpoint: PathBuf,
  /// Dedicated containerd namespace.
  pub namespace: String,
  /// Snapshotter used for job root filesystems.
  pub snapshotter: String,
  /// OCI runtime-v2 implementation name.
  pub runtime: String,
  /// Optional registry-host configuration root.
  pub registry_config_dir: Option<PathBuf>,
  /// Agent-owned persistent backend state root.
  pub state_root: PathBuf,
  /// Agent-owned quota-backed workspace root.
  pub work_root: PathBuf,
  /// Maximum accepted workspace filesystem capacity.
  pub max_workspace_bytes: u64,
  /// Bound for resource cleanup operations.
  pub cleanup_timeout: Duration,
  /// Maximum descendant processes in one container.
  pub pids_limit: u32,
  /// Per-process `RLIMIT_NOFILE` inside the container.
  pub open_files_limit: u64,
}

#[cfg(any(target_os = "linux", all(test, target_os = "macos")))]
mod linux;

#[cfg(any(target_os = "linux", all(test, target_os = "macos")))]
pub use linux::ContainerdEngine;

#[cfg(not(any(target_os = "linux", all(test, target_os = "macos"))))]
mod unsupported {
  use async_trait::async_trait;
  use octacity_execution::{ExecutionError, RunnerProgram, RunningExecution, StartExecution};
  use octacity_execution_oci::{OciCapability, OciEngine};
  use tokio_util::sync::CancellationToken;

  use crate::ContainerdEngineConfig;

  /// Linux process-isolation engine placeholder on unsupported hosts.
  pub struct ContainerdEngine;

  impl ContainerdEngine {
    /// Reports containerd process isolation as unavailable on this host OS.
    pub fn new(_config: ContainerdEngineConfig) -> Result<Self, ExecutionError> {
      Err(ExecutionError::Unavailable(
        "containerd process isolation is currently implemented only for Linux agents".to_owned(),
      ))
    }

    /// Reports that a containerd connection cannot be validated on this OS.
    pub async fn validate_connection(&self) -> Result<(), ExecutionError> {
      Err(ExecutionError::Unavailable(
        "containerd process isolation is currently implemented only for Linux agents".to_owned(),
      ))
    }
  }

  #[async_trait]
  impl OciEngine for ContainerdEngine {
    fn capabilities(&self) -> Vec<OciCapability> {
      Vec::new()
    }

    async fn start(
      &self,
      _runner: &RunnerProgram,
      _request: StartExecution,
      _cancellation: CancellationToken,
    ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
      Err(ExecutionError::Unavailable(
        "containerd process isolation is currently implemented only for Linux agents".to_owned(),
      ))
    }

    async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
      Ok(())
    }
  }
}

#[cfg(not(any(target_os = "linux", all(test, target_os = "macos"))))]
pub use unsupported::ContainerdEngine;
