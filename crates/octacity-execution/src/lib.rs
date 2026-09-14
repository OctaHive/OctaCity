//! Backend-neutral contract for starting and supervising one job execution.
//!
//! The runner supervisor depends only on these lifecycle and accounting
//! operations. Native and sandboxed backends therefore expose identical I/O,
//! cancellation, cleanup, and resource-usage semantics.

use std::{
  path::{Path, PathBuf},
  pin::Pin,
  time::Duration,
};

use async_trait::async_trait;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

/// Owned asynchronous stdout or stderr stream from an execution backend.
pub type ExecutionReader = Pin<Box<dyn AsyncRead + Send>>;
/// Owned asynchronous stdin stream into an execution backend.
pub type ExecutionWriter = Pin<Box<dyn AsyncWrite + Send>>;

/// Stable read-only path at which every backend exposes job identity.
pub const WORKLOAD_IDENTITY_PATH: &str = "/run/octa-identity";

/// Protocol streams connected to the isolated `octa-runner` process.
pub struct ExecutionIo {
  /// Control input consumed by `octa-runner`.
  pub stdin: ExecutionWriter,
  /// Protocol output produced by `octa-runner`.
  pub stdout: ExecutionReader,
  /// Bounded diagnostic output kept separate from the protocol.
  pub stderr: ExecutionReader,
}

/// Verified host paths required by a backend to start `octa-runner`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunnerProgram {
  /// Canonical root containing the complete verified Octa distribution.
  pub release_root: PathBuf,
  /// Canonical `octa-runner` executable inside `release_root`.
  pub executable: PathBuf,
  /// Canonical directory containing the locked plugin executables.
  pub plugins_dir: PathBuf,
  /// Canonical `Octa.lock` authorizing the plugin set.
  pub plugin_lock: PathBuf,
}

impl RunnerProgram {
  /// Checks that every distribution path is absolute and confined to its root.
  pub fn validate(&self) -> Result<(), ExecutionError> {
    if !self.release_root.is_absolute()
      || !self.executable.is_absolute()
      || !self.plugins_dir.is_absolute()
      || !self.plugin_lock.is_absolute()
      || !self.executable.starts_with(&self.release_root)
      || !self.plugins_dir.starts_with(&self.release_root)
      || !self.plugin_lock.starts_with(&self.release_root)
    {
      return Err(ExecutionError::Invalid(
        "runner paths must be absolute and contained by release_root".to_owned(),
      ));
    }
    Ok(())
  }
}

/// Root filesystem selected for an execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionTarget {
  /// Run directly on an agent host matching the signed platform.
  Native {
    /// Host platform that must execute the runner directly.
    platform: ExecutionPlatform,
  },
  /// Run an immutable OCI image for the exact guest platform and isolation tier.
  Oci {
    /// Immutable repository and digest selected by the signed job.
    reference: String,
    /// Required guest platform.
    platform: ExecutionPlatform,
    /// Minimum isolation boundary the engine must provide.
    isolation: OciIsolation,
  },
}

/// Operating systems understood by execution backends.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExecutionOs {
  /// Linux host or guest.
  Linux,
  /// Windows host or guest.
  Windows,
  /// macOS host.
  Macos,
}

/// CPU architectures understood by execution backends.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExecutionArchitecture {
  /// 64-bit x86 (`amd64`).
  Amd64,
  /// 64-bit ARM (`arm64`).
  Arm64,
}

/// Exact OS and architecture required by an execution.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExecutionPlatform {
  /// Required operating system.
  pub os: ExecutionOs,
  /// Required CPU architecture.
  pub architecture: ExecutionArchitecture,
}

/// Isolation tier required for an OCI execution.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OciIsolation {
  /// Linux namespaces and cgroups in the agent kernel.
  Process,
  /// A separate guest kernel managed by a hypervisor-backed engine.
  Hypervisor,
}

/// Network access that the selected backend must enforce exactly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkAccess {
  /// Preserve backend network access without filtering.
  Unrestricted,
  /// Create the execution without external network access.
  Disabled,
  /// Allow only the explicitly listed hosts.
  Restricted {
    /// Non-empty host names authorized by the signed job.
    allowed_hosts: Vec<String>,
  },
}

/// Resources and filesystem locations needed to start a job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartExecution {
  /// Unique, log-safe identity used to name owned backend resources.
  pub execution_id: String,
  /// Agent-owned filesystem root containing this execution's workspace.
  pub workspace_root: PathBuf,
  /// Existing workspace directory visible to the runner.
  pub workspace: PathBuf,
  /// Existing Octa state directory contained by `workspace`.
  pub data_dir: PathBuf,
  /// Optional job-private identity file mounted read-only at
  /// [`WORKLOAD_IDENTITY_PATH`].
  pub workload_identity: Option<PathBuf>,
  /// CPU allocation in thousandths of one logical CPU.
  pub cpu_millis: u32,
  /// Maximum addressable memory in bytes.
  pub memory_bytes: u64,
  /// Maximum writable workspace filesystem capacity in bytes.
  pub writable_disk_bytes: u64,
  /// Maximum duration for backend preparation and the running execution.
  pub max_duration: Duration,
  /// Native or OCI root selected for this execution.
  pub root: ExecutionTarget,
  /// Network policy the backend must enforce exactly.
  pub network: NetworkAccess,
}

impl StartExecution {
  /// Validates backend-independent execution invariants.
  pub fn validate(&self) -> Result<(), ExecutionError> {
    if self.execution_id.is_empty() || self.execution_id.chars().any(char::is_control) {
      return Err(ExecutionError::Invalid(
        "execution_id must not be empty or contain control characters".to_owned(),
      ));
    }
    if !self.workspace_root.is_absolute() || !self.workspace_root.is_dir() {
      return Err(ExecutionError::Invalid(
        "workspace_root must be an existing absolute directory".to_owned(),
      ));
    }
    if !self.workspace.is_absolute() || !self.workspace.is_dir() {
      return Err(ExecutionError::Invalid(
        "workspace must be an existing absolute directory inside workspace_root".to_owned(),
      ));
    }
    if !self.data_dir.is_absolute() || !self.data_dir.is_dir() {
      return Err(ExecutionError::Invalid(
        "data_dir must be an existing absolute directory inside workspace".to_owned(),
      ));
    }
    let canonical_root = canonical_directory(&self.workspace_root, "workspace_root")?;
    let canonical_workspace = canonical_directory(&self.workspace, "workspace")?;
    let canonical_data = canonical_directory(&self.data_dir, "data_dir")?;
    if !canonical_workspace.starts_with(&canonical_root) {
      return Err(ExecutionError::Invalid(
        "workspace must resolve inside workspace_root".to_owned(),
      ));
    }
    if !canonical_data.starts_with(&canonical_workspace) {
      return Err(ExecutionError::Invalid(
        "data_dir must resolve inside workspace".to_owned(),
      ));
    }
    if let Some(identity) = &self.workload_identity {
      let metadata = std::fs::symlink_metadata(identity).map_err(|error| {
        ExecutionError::Invalid(format!("workload identity must be an existing regular file: {error}"))
      })?;
      if !identity.is_absolute()
        || !metadata.file_type().is_file()
        || !is_canonical_path(identity)
        || !identity.starts_with(&canonical_root)
        || identity.starts_with(&canonical_workspace)
      {
        return Err(ExecutionError::Invalid(
          "workload identity must be a canonical absolute regular file inside workspace_root and outside workspace"
            .to_owned(),
        ));
      }
    }
    if self.cpu_millis == 0 || self.memory_bytes == 0 || self.writable_disk_bytes == 0 || self.max_duration.is_zero() {
      return Err(ExecutionError::Invalid(
        "CPU, memory, and writable disk limits must be greater than zero".to_owned(),
      ));
    }
    if let ExecutionTarget::Oci { reference, .. } = &self.root
      && !is_immutable_oci_reference(reference)
    {
      return Err(ExecutionError::Invalid(
        "OCI image must be an immutable repository@sha256:<digest> reference".to_owned(),
      ));
    }
    if matches!(&self.network, NetworkAccess::Restricted { allowed_hosts } if allowed_hosts.is_empty() || allowed_hosts.iter().any(|host| host.trim().is_empty()))
    {
      return Err(ExecutionError::Invalid(
        "restricted network access requires non-empty allowed_hosts".to_owned(),
      ));
    }
    Ok(())
  }
}

fn canonical_directory(path: &Path, name: &str) -> Result<PathBuf, ExecutionError> {
  std::fs::canonicalize(path).map_err(|error| ExecutionError::Invalid(format!("failed to resolve {name}: {error}")))
}

fn is_canonical_path(path: &Path) -> bool {
  path.is_absolute() && std::fs::canonicalize(path).is_ok_and(|canonical| canonical == path)
}

fn is_immutable_oci_reference(value: &str) -> bool {
  let Some((repository, digest)) = value.rsplit_once("@sha256:") else {
    return false;
  };
  !repository.is_empty()
    && !repository.contains('@')
    && !repository.contains("://")
    && !repository.chars().any(char::is_whitespace)
    && digest.len() == 64
    && digest
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Paths as seen by `octa-runner` inside the selected backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPaths {
  /// Workspace path as observed by the runner.
  pub workspace: PathBuf,
  /// Octa state path as observed by the runner.
  pub data_dir: PathBuf,
  /// Plugin directory as observed by the runner.
  pub plugins_dir: PathBuf,
  /// Plugin lock path as observed by the runner.
  pub plugin_lock: PathBuf,
}

/// Monotonic resource-accounting snapshot for a running job.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceUsage {
  /// Monotonic elapsed execution time in milliseconds.
  pub elapsed_ms: u64,
  /// Cumulative CPU time used by the complete execution tree.
  pub cpu_time_ms: u64,
  /// Current accounted memory in bytes.
  pub memory_current_bytes: u64,
  /// Peak accounted memory in bytes.
  pub memory_peak_bytes: u64,
  /// Current writable filesystem consumption in bytes.
  pub disk_current_bytes: u64,
  /// Peak writable filesystem consumption observed by the backend.
  pub disk_peak_bytes: u64,
  /// Cumulative bytes read from accounted block devices.
  pub io_read_bytes: u64,
  /// Cumulative bytes written to accounted block devices.
  pub io_written_bytes: u64,
  /// Received network bytes, or `None` when the backend cannot account them.
  pub network_received_bytes: Option<u64>,
  /// Transmitted network bytes, or `None` when the backend cannot account them.
  pub network_transmitted_bytes: Option<u64>,
}

/// Process exit observed by the backend after the runner has stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionExit {
  /// Portable process exit code, or `None` when the backend cannot represent it.
  pub code: Option<i32>,
}

/// Failure reported by an execution backend or its lifecycle contract.
#[derive(Debug, Error)]
pub enum ExecutionError {
  /// Caller supplied an invalid request or path boundary.
  #[error("invalid execution request: {0}")]
  Invalid(String),
  /// The configured backend cannot enforce the requested execution.
  #[error("execution backend is unavailable: {0}")]
  Unavailable(String),
  /// The backend failed while creating, driving, or removing resources.
  #[error("execution backend failed: {0}")]
  Backend(String),
  /// Local asynchronous or filesystem I/O failed.
  #[error("execution I/O failed: {0}")]
  Io(#[source] std::io::Error),
  /// Protocol streams were requested more than once.
  #[error("execution I/O has already been taken")]
  IoTaken,
  /// Process exit was requested more than once.
  #[error("execution process has already been reaped")]
  Reaped,
  /// Cancellation won before the requested operation completed.
  #[error("execution was cancelled")]
  Cancelled,
  /// The shared execution deadline expired during the named operation.
  #[error("execution deadline expired while {operation}")]
  TimedOut {
    /// Operation active when the deadline expired.
    operation: &'static str,
  },
  /// Both an execution operation and its mandatory cleanup failed.
  #[error("execution failed ({operation}) and cleanup also failed: {cleanup}")]
  OperationAndCleanup {
    /// Original lifecycle failure.
    operation: Box<ExecutionError>,
    /// Additional failure that prevented complete cleanup.
    cleanup: Box<ExecutionError>,
  },
}

impl ExecutionError {
  /// Preserves the original failure while reporting that rollback was incomplete.
  pub fn with_cleanup(self, cleanup: ExecutionError) -> Self {
    Self::OperationAndCleanup {
      operation: Box::new(self),
      cleanup: Box::new(cleanup),
    }
  }
}

/// Creates executions while hiding native or sandbox-specific mechanics.
///
/// `start` must honor `StartExecution::max_duration` and `cancellation`. If
/// startup fails, is cancelled, or reaches its deadline after creating backend
/// resources, it must roll them back before returning an error.
#[async_trait]
pub trait ExecutionBackend: Send + Sync {
  /// Starts one execution and rolls back all created resources on failure.
  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError>;
  /// Removes abandoned resources owned by this agent and backend.
  async fn cleanup_orphans(&self) -> Result<(), ExecutionError>;
}

/// Exclusive handle to one running execution and all resources it owns.
#[async_trait]
pub trait RunningExecution: Send {
  /// Returns paths translated into the execution's filesystem namespace.
  fn paths(&self) -> &ExecutionPaths;
  /// Transfers exclusive ownership of runner protocol streams to the caller.
  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError>;
  /// Samples accounting for the complete process tree or sandbox.
  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError>;
  /// Closes any backend-owned input transport after the caller drops its
  /// `ExecutionIo::stdin`. Most process transports need no extra action;
  /// runtimes such as containerd must explicitly signal EOF to their shim.
  async fn close_input(&mut self) -> Result<(), ExecutionError> {
    Ok(())
  }
  /// Waits once for the complete backend process tree to stop.
  ///
  /// A second call must return [`ExecutionError::Reaped`] rather than repeat a
  /// cached exit or wait on resources that have already been consumed.
  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError>;
  /// Forcefully terminates the complete execution process tree.
  async fn kill(&mut self) -> Result<(), ExecutionError>;
  /// Removes every backend resource owned by this execution.
  async fn destroy(self: Box<Self>) -> Result<(), ExecutionError>;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn validates_execution_boundaries() {
    let temporary = tempfile::tempdir().unwrap();
    let request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: temporary.path().canonicalize().unwrap(),
      workspace: temporary.path().canonicalize().unwrap(),
      data_dir: temporary.path().canonicalize().unwrap().join("data"),
      workload_identity: None,
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      max_duration: Duration::from_secs(600),
      root: ExecutionTarget::Native {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
      },
      network: NetworkAccess::Unrestricted,
    };
    std::fs::create_dir(&request.data_dir).unwrap();
    assert!(request.validate().is_ok());

    let mut invalid = request.clone();
    invalid.execution_id.clear();
    assert!(invalid.validate().is_err());
    invalid = request;
    invalid.memory_bytes = 0;
    assert!(invalid.validate().is_err());
  }

  #[test]
  fn rejects_mutable_execution_images_at_the_backend_boundary() {
    let temporary = tempfile::tempdir().unwrap();
    let data_dir = temporary.path().join("data");
    std::fs::create_dir(&data_dir).unwrap();
    let mut request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: temporary.path().canonicalize().unwrap(),
      workspace: temporary.path().canonicalize().unwrap(),
      data_dir: data_dir.canonicalize().unwrap(),
      workload_identity: None,
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      max_duration: Duration::from_secs(600),
      root: ExecutionTarget::Oci {
        reference: "registry.example.com/octacity/build:latest".to_owned(),
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
        isolation: OciIsolation::Process,
      },
      network: NetworkAccess::Disabled,
    };
    assert!(request.validate().is_err());

    request.root = ExecutionTarget::Oci {
      reference:
        "registry.example.com/octacity/build@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
          .to_owned(),
      platform: ExecutionPlatform {
        os: ExecutionOs::Linux,
        architecture: ExecutionArchitecture::Amd64,
      },
      isolation: OciIsolation::Hypervisor,
    };
    assert!(request.validate().is_ok());
  }

  #[test]
  fn rejects_noncanonical_identity_paths_before_a_backend_sees_them() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let workspace = root.join("workspace");
    let identity_directory = root.join("identities");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&identity_directory).unwrap();
    let data_dir = workspace.join("data");
    std::fs::create_dir(&data_dir).unwrap();
    let identity = identity_directory.join("job.identity");
    std::fs::write(&identity, b"identity").unwrap();
    let mut request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: root,
      workspace,
      data_dir,
      workload_identity: Some(identity.clone()),
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      max_duration: Duration::from_secs(600),
      root: ExecutionTarget::Native {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
      },
      network: NetworkAccess::Disabled,
    };
    assert!(request.validate().is_ok());

    request.workload_identity = Some(identity_directory.join("..").join("identities/job.identity"));
    assert!(request.validate().is_err());
  }

  #[cfg(unix)]
  #[test]
  fn rejects_identity_paths_through_an_intermediate_symlink() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let workspace = root.join("workspace");
    let identity_directory = root.join("identities");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&identity_directory).unwrap();
    let data_dir = workspace.join("data");
    std::fs::create_dir(&data_dir).unwrap();
    std::fs::write(identity_directory.join("job.identity"), b"identity").unwrap();
    let alias = root.join("identity-alias");
    symlink(&identity_directory, &alias).unwrap();
    let request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: root,
      workspace,
      data_dir,
      workload_identity: Some(alias.join("job.identity")),
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      max_duration: Duration::from_secs(600),
      root: ExecutionTarget::Native {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
      },
      network: NetworkAccess::Disabled,
    };

    assert!(request.validate().is_err());
  }
}
