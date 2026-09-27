//! Runs Linux OCI workloads in per-container Apple Virtualization.framework VMs.
//!
//! The adapter drives Apple's official `container` CLI as an explicitly
//! configured runtime. Although the provider uses one lightweight VM per
//! workload internally, it implements OctaCity's provider-neutral `isolation`
//! contract: the signed job asks for observable filesystem, process, network,
//! and resource boundaries without selecting this implementation by name.

#![warn(missing_docs)]

use std::{path::PathBuf, time::Duration};

/// Stable scheduler, diagnostics, and evidence identifier for this provider.
pub const APPLE_VF_PROVIDER_NAME: &str = "apple-vf-isolation";

/// Operator-owned settings for Apple's Virtualization.framework-backed CLI.
#[derive(Clone, Debug)]
pub struct AppleVfEngineConfig {
  /// Stable Agent owner identity used to derive private container names.
  pub agent_id: String,
  /// Exact operator-installed `container` executable.
  pub executable: PathBuf,
  /// Agent-owned root used for durable cleanup markers.
  pub state_root: PathBuf,
  /// Agent-owned quota-backed workspace filesystem.
  pub work_root: PathBuf,
  /// Linux guest platform declared by the installed Octa runner.
  pub runner_platform: String,
  /// Maximum capacity accepted for the workspace filesystem.
  pub max_workspace_bytes: u64,
  /// Bound for runtime control and cleanup commands.
  pub cleanup_timeout: Duration,
  /// Per-process `RLIMIT_NOFILE` inside the workload.
  pub open_files_limit: u64,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod macos;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use macos::AppleVfEngine;

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
mod unsupported {
  use async_trait::async_trait;
  use octacity_execution::{ExecutionError, RunnerProgram, RunningExecution, StartExecution};
  use octacity_execution_oci::{OciCapability, OciEngine};
  use tokio_util::sync::CancellationToken;

  use crate::AppleVfEngineConfig;

  /// Placeholder that rejects Apple VF isolation on unsupported hosts.
  pub struct AppleVfEngine;

  impl AppleVfEngine {
    /// Reports that the provider requires Apple Silicon macOS.
    pub fn new(_config: AppleVfEngineConfig) -> Result<Self, ExecutionError> {
      Err(ExecutionError::Unavailable(
        "Apple VF isolation requires an Apple Silicon macOS agent".to_owned(),
      ))
    }

    /// Reports that the provider cannot be validated on this host.
    pub async fn validate_connection(&self) -> Result<(), ExecutionError> {
      Err(ExecutionError::Unavailable(
        "Apple VF isolation requires an Apple Silicon macOS agent".to_owned(),
      ))
    }
  }

  #[async_trait]
  impl OciEngine for AppleVfEngine {
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
        "Apple VF isolation requires an Apple Silicon macOS agent".to_owned(),
      ))
    }

    async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
      Ok(())
    }
  }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub use unsupported::AppleVfEngine;
