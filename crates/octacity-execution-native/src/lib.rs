//! Native Linux execution through Bubblewrap, cgroup v2, and a workspace mount.
//!
//! Bubblewrap supplies mount, PID, IPC, UTS, cgroup, and network namespaces;
//! an OctaCity-owned seccomp profile removes privileged kernel operations;
//! cgroup v2 enforces CPU, memory, and process limits. Writable data is limited
//! to the workspace filesystem, whose capacity is the signed disk boundary.

use std::{collections::BTreeMap, path::PathBuf};

#[cfg(target_os = "linux")]
use std::path::Path;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use octacity_execution::{ExecutionBackend, ExecutionError, RunnerProgram, RunningExecution, StartExecution};

/// Stable scheduler and diagnostics identifier for Native execution.
pub const NATIVE_BACKEND_NAME: &str = "native";

#[cfg(target_os = "linux")]
use octacity_execution::{ExecutionTarget, NetworkAccess};

/// Operator-owned inputs required by the Linux Native containment boundary.
#[derive(Clone, Debug)]
pub struct LinuxNativeConfig {
  /// Dedicated delegated cgroup-v2 root.
  pub cgroup_root: PathBuf,
  /// Agent-owned quota-backed workspace root.
  pub work_root: PathBuf,
  /// Exact Bubblewrap executable.
  pub bubblewrap: PathBuf,
  /// Additional host paths exposed read-only.
  pub readonly_paths: Vec<PathBuf>,
  /// Host platform required by the installed runner.
  pub runner_platform: String,
  /// Maximum accepted workspace filesystem capacity.
  pub max_workspace_bytes: u64,
  /// Bound for resource cleanup operations.
  pub cleanup_timeout: std::time::Duration,
  /// Maximum descendant processes in one job cgroup.
  pub pids_limit: u32,
  /// Explicit clean process environment.
  pub environment: BTreeMap<String, String>,
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::NativeBackend;

#[cfg(not(target_os = "linux"))]
/// Placeholder backend that fails closed when Linux containment is unavailable.
pub struct NativeBackend;

#[cfg(not(target_os = "linux"))]
impl NativeBackend {
  /// Reports Native execution as unavailable on non-Linux platforms.
  pub fn new(_config: LinuxNativeConfig) -> Result<Self, ExecutionError> {
    Err(ExecutionError::Unavailable(
      "NativeBackend v1 requires Linux cgroup v2 and a quota-backed workspace mount".to_owned(),
    ))
  }
}

#[cfg(not(target_os = "linux"))]
#[async_trait]
impl ExecutionBackend for NativeBackend {
  async fn start(
    &self,
    _runner: &RunnerProgram,
    _request: StartExecution,
    _cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    Err(ExecutionError::Unavailable(
      "NativeBackend v1 is available only on Linux".to_owned(),
    ))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    Ok(())
  }
}

#[cfg(all(test, not(target_os = "linux")))]
mod non_linux_tests {
  use super::*;

  #[test]
  fn reports_native_execution_as_unavailable() {
    let temporary = tempfile::tempdir().unwrap();
    let error = NativeBackend::new(LinuxNativeConfig {
      cgroup_root: temporary.path().to_owned(),
      work_root: temporary.path().to_owned(),
      bubblewrap: temporary.path().to_owned(),
      readonly_paths: Vec::new(),
      runner_platform: "linux-x86_64".to_owned(),
      max_workspace_bytes: 1024,
      cleanup_timeout: std::time::Duration::from_secs(1),
      pids_limit: 4096,
      environment: BTreeMap::from([("PATH".to_owned(), "/usr/bin:/bin".to_owned())]),
    })
    .err()
    .expect("NativeBackend must be unavailable outside Linux");

    assert!(matches!(error, ExecutionError::Unavailable(_)));
  }
}
