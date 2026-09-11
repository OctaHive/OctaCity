//! Owns the complete, backend-neutral lifecycle of one verified job.
//!
//! This crate is orchestration rather than infrastructure. It accepts an
//! already verified intent, creates one private workspace,
//! materializes the exact source revision, selects the requested execution
//! backend without fallback, drives `octa-runner`, and either removes failed
//! workspaces or transfers ownership of a successful workspace for output
//! processing and explicit cleanup. Concrete VCS, process, microVM, and
//! transport mechanics remain behind their respective component interfaces.

use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use octacity_execution::{
  ExecutionArchitecture, ExecutionBackend, ExecutionError, ExecutionOs, ExecutionPlatform, ExecutionTarget,
  NetworkAccess, OciIsolation as ExecutionOciIsolation, StartExecution,
};
use octacity_protocol::{
  JobSpecV1, NetworkPolicy, OciIsolation as ProtocolOciIsolation, PlatformArchitecture, PlatformOs, RuntimeMode,
  RuntimeTarget,
};
use octacity_runner::{
  RunnerCompletion, RunnerInstallation, RunnerInstallationError, RunnerJobRequest, RunnerStreamItem,
  RunnerSupervisionError, RunnerSupervisionPolicy, supervise,
};
use octacity_source::{MaterializedSource, SourceError, SourceMaterializationRequest, SourceMaterializer};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::{sync::mpsc, time::Instant};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Verified execution intent and agent-resolved source credentials for one job.
#[derive(Debug)]
pub struct ExecuteJobRequest {
  /// JobSpec verified against the active lease before this layer is called.
  pub spec: JobSpecV1,
  /// Agent-resolved credential files keyed by source-plugin handle.
  pub source_credentials: BTreeMap<String, PathBuf>,
}

/// Successful terminal data from source acquisition and `octa-runner`.
#[derive(Debug)]
pub struct JobCompletion {
  source: MaterializedSource,
  runner: RunnerCompletion,
  output_limits: octacity_protocol::OutputLimits,
  workspace: PathBuf,
  job_root: Option<PathBuf>,
}

impl JobCompletion {
  /// Returns verified provenance from source materialization.
  pub fn source(&self) -> &MaterializedSource {
    &self.source
  }

  /// Returns the terminal runner result and final resource usage.
  pub fn runner(&self) -> &RunnerCompletion {
    &self.runner
  }

  /// Signed bounds to apply while collecting and uploading job outputs.
  pub fn output_limits(&self) -> &octacity_protocol::OutputLimits {
    &self.output_limits
  }

  /// Host workspace retained after runner shutdown for output processing.
  pub fn workspace(&self) -> &Path {
    &self.workspace
  }

  /// Permanently removes all filesystem state retained for this job.
  pub async fn cleanup(mut self) -> Result<(), JobError> {
    let Some(job_root) = self.job_root.as_ref() else {
      return Ok(());
    };
    tokio::fs::remove_dir_all(job_root).await.map_err(JobError::Cleanup)?;
    self.job_root = None;
    Ok(())
  }
}

impl Drop for JobCompletion {
  fn drop(&mut self) {
    if let Some(job_root) = &self.job_root {
      warn!(path = %job_root.display(), "job completion dropped before workspace cleanup");
    }
  }
}

/// Filesystem and timing policy owned by the job lifecycle.
#[derive(Clone, Debug)]
pub struct JobExecutorConfig {
  /// Existing agent-owned root under which private job directories are created.
  pub work_root: PathBuf,
  /// Agent-side upper bound for a signed job's writable workspace request.
  pub max_workspace_bytes: u64,
  /// Time allowed for source plugins and the runner to stop gracefully.
  pub cancellation_grace: Duration,
  /// Runner protocol and resource-accounting timing policy.
  pub runner_supervision: RunnerSupervisionPolicy,
}

/// Construction or lifecycle failure for one job.
#[derive(Debug, Error)]
pub enum JobError {
  #[error("invalid job executor configuration: {0}")]
  /// Executor construction or request data violates a local invariant.
  Invalid(String),
  #[error("installed Octa does not satisfy the job: {0}")]
  /// The installed runner or plugin set differs from the signed requirement.
  RunnerInstallation(#[source] Box<RunnerInstallationError>),
  #[error("runtime mode '{0:?}' is not enabled")]
  /// No configured execution backend implements the requested runtime mode.
  RuntimeUnavailable(RuntimeMode),
  #[error("workload identity is not available in this agent milestone")]
  /// The job requests workload identity before the agent supports it.
  WorkloadIdentityUnavailable,
  #[error("job requests {requested} workspace bytes, exceeding the agent limit {maximum}")]
  /// The signed writable-disk request exceeds local agent policy.
  WorkspaceLimit {
    /// Bytes requested by the signed job.
    requested: u64,
    /// Maximum bytes allowed by the agent.
    maximum: u64,
  },
  #[error("job was cancelled before runner execution")]
  /// Cancellation stopped the job lifecycle.
  Cancelled,
  #[error("job execution deadline expired")]
  /// The job's absolute execution deadline expired.
  TimedOut,
  #[error("source acquisition failed: {0}")]
  /// Trusted source-plugin resolution or materialization failed.
  Source(#[source] Box<SourceError>),
  #[error("runner supervision failed: {0}")]
  /// Runner execution, protocol handling, or backend cleanup failed.
  Runner(#[source] Box<RunnerSupervisionError>),
  #[error("failed to {action} '{path}': {source}")]
  /// A workspace filesystem operation failed.
  Filesystem {
    /// Human-readable operation being attempted.
    action: &'static str,
    /// Filesystem path involved in the operation.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  #[error("workspace cleanup failed: {0}")]
  /// Explicit successful-job cleanup failed.
  Cleanup(#[source] std::io::Error),
  #[error("job failed ({operation}) and workspace cleanup also failed: {cleanup}")]
  /// Both the primary lifecycle operation and mandatory cleanup failed.
  OperationAndCleanup {
    /// Original job lifecycle failure.
    operation: Box<JobError>,
    /// Subsequent workspace cleanup failure.
    cleanup: std::io::Error,
  },
  #[error("orphan cleanup failed: {0}")]
  /// One or more backend resources or abandoned workspaces could not be removed.
  OrphanCleanup(String),
}

/// Immutable dependencies and policy limits used for every job on an agent.
pub struct JobExecutor {
  runner: RunnerInstallation,
  source: Arc<dyn SourceMaterializer>,
  backends: BTreeMap<RuntimeMode, Arc<dyn ExecutionBackend>>,
  work_root: PathBuf,
  max_workspace_bytes: u64,
  cancellation_grace: Duration,
  runner_supervision: RunnerSupervisionPolicy,
}

impl JobExecutor {
  /// Validates immutable agent dependencies and constructs a job orchestrator.
  pub fn new(
    runner: RunnerInstallation,
    source: Arc<dyn SourceMaterializer>,
    backends: BTreeMap<RuntimeMode, Arc<dyn ExecutionBackend>>,
    config: JobExecutorConfig,
  ) -> Result<Self, JobError> {
    if backends.is_empty() {
      return Err(JobError::Invalid(
        "at least one execution backend is required".to_owned(),
      ));
    }
    if config.max_workspace_bytes == 0 || config.cancellation_grace.is_zero() {
      return Err(JobError::Invalid(
        "workspace limit and cancellation grace must be greater than zero".to_owned(),
      ));
    }
    config
      .runner_supervision
      .validate()
      .map_err(|error| JobError::Invalid(error.to_string()))?;
    let work_root = config
      .work_root
      .canonicalize()
      .map_err(|source| filesystem("canonicalize work root", &config.work_root, source))?;
    if !work_root.is_dir() {
      return Err(JobError::Invalid("work_root must be a directory".to_owned()));
    }
    Ok(Self {
      runner,
      source,
      backends,
      work_root,
      max_workspace_bytes: config.max_workspace_bytes,
      cancellation_grace: config.cancellation_grace,
      runner_supervision: config.runner_supervision,
    })
  }

  /// Runs one verified job and retains its workspace for output processing.
  /// The caller must finish by invoking [`JobCompletion::cleanup`].
  pub async fn execute(
    &self,
    request: ExecuteJobRequest,
    cancellation: CancellationToken,
    events: &mpsc::Sender<RunnerStreamItem>,
  ) -> Result<JobCompletion, JobError> {
    let spec = request.spec;

    // Verify every immutable dependency before creating a workspace or
    // starting a source plugin.
    self
      .runner
      .verify(&spec.octa)
      .map_err(|error| JobError::RunnerInstallation(Box::new(error)))?;
    let backend = self
      .backends
      .get(&spec.runtime.mode())
      .ok_or(JobError::RuntimeUnavailable(spec.runtime.mode()))?;
    if spec.runtime.writable_disk_bytes > self.max_workspace_bytes {
      return Err(JobError::WorkspaceLimit {
        requested: spec.runtime.writable_disk_bytes,
        maximum: self.max_workspace_bytes,
      });
    }
    if spec.runtime.workload_identity_profile.is_some() {
      return Err(JobError::WorkloadIdentityUnavailable);
    }
    if cancellation.is_cancelled() {
      return Err(JobError::Cancelled);
    }

    let execution_id = execution_id(&spec);
    let job_root = self.work_root.join(&execution_id);
    create_private_directory(&job_root)?;
    let result = async {
      let workspace = job_root.join("workspace");
      create_private_directory(&workspace)?;
      let deadline = Instant::now() + Duration::from_secs(spec.runtime.timeout_seconds);
      info!(job_id = %spec.job_id, attempt = spec.attempt, runtime = ?spec.runtime.mode(), "starting job");

      let source = self
        .source
        .materialize(
          &spec.source,
          SourceMaterializationRequest {
            request_id: execution_id.clone(),
            destination: workspace.clone(),
            revision: spec.source.revision.clone(),
            reference: spec.source.reference.clone(),
            parameters: spec.source.parameters.clone(),
            credential_files: request.source_credentials,
            max_workspace_bytes: spec.runtime.writable_disk_bytes,
          },
          remaining(deadline)?,
          self.cancellation_grace,
          cancellation.clone(),
        )
        .await
        .map_err(|error| JobError::Source(Box::new(error)))?;

      if cancellation.is_cancelled() {
        return Err(JobError::Cancelled);
      }
      let data_dir = workspace.join(".octacity");
      create_private_directory(&data_dir)?;
      let (root, network) = execution_environment(&spec);
      let runner = supervise(
        &self.runner,
        backend.as_ref(),
        RunnerJobRequest {
          request_id: execution_id.clone(),
          octa: spec.octa.clone(),
          execution: StartExecution {
            execution_id,
            workspace_root: self.work_root.clone(),
            workspace: workspace.clone(),
            data_dir,
            cpu_millis: spec.runtime.cpu_millis,
            memory_bytes: spec.runtime.memory_bytes,
            writable_disk_bytes: spec.runtime.writable_disk_bytes,
            max_duration: remaining(deadline)?,
            root,
            network,
          },
          spec: spec.execution.clone(),
          cancellation_grace: self.cancellation_grace,
        },
        cancellation,
        events,
        &self.runner_supervision,
      )
      .await
      .map_err(map_runner_error)?;

      info!(job_id = %spec.job_id, attempt = spec.attempt, status = ?runner.status, "finished job");
      Ok((source, runner, workspace))
    }
    .await;
    match result {
      Ok((source, runner, workspace)) => Ok(JobCompletion {
        source,
        runner,
        output_limits: spec.outputs,
        workspace,
        job_root: Some(job_root),
      }),
      Err(operation) => match tokio::fs::remove_dir_all(&job_root).await {
        Ok(()) => Err(operation),
        Err(cleanup) => Err(JobError::OperationAndCleanup {
          operation: Box::new(operation),
          cleanup,
        }),
      },
    }
  }

  /// Removes backend-owned resources and abandoned job directories left by a
  /// previous agent process. Call this once before accepting leases.
  pub async fn cleanup_orphans(&self) -> Result<(), JobError> {
    let mut failures = Vec::new();
    for (kind, backend) in &self.backends {
      if let Err(error) = backend.cleanup_orphans().await {
        warn!(backend = ?kind, error = %error, "failed to clean up backend orphans");
        failures.push(format!("{kind:?} backend: {error}"));
      }
    }

    let entries =
      fs::read_dir(&self.work_root).map_err(|error| filesystem("read work root", &self.work_root, error))?;
    for entry in entries {
      let entry = entry.map_err(|error| filesystem("read work-root entry", &self.work_root, error))?;
      let name = entry.file_name();
      let Some(name) = name.to_str() else {
        warn!(path = %entry.path().display(), "leaving an unrecognized work-root entry untouched");
        continue;
      };
      if !is_execution_directory(name)
        || !entry
          .file_type()
          .map_err(|error| filesystem("inspect work-root entry", &entry.path(), error))?
          .is_dir()
      {
        warn!(path = %entry.path().display(), "leaving an unrecognized work-root entry untouched");
        continue;
      }
      if let Err(error) = fs::remove_dir_all(entry.path()) {
        failures.push(format!("workspace '{}': {error}", entry.path().display()));
      }
    }

    if failures.is_empty() {
      Ok(())
    } else {
      Err(JobError::OrphanCleanup(failures.join("; ")))
    }
  }
}

fn execution_environment(spec: &JobSpecV1) -> (ExecutionTarget, NetworkAccess) {
  let root = match &spec.runtime.target {
    RuntimeTarget::Native { platform } => ExecutionTarget::Native {
      platform: execution_platform(*platform),
    },
    RuntimeTarget::Oci {
      platform,
      isolation,
      image,
    } => ExecutionTarget::Oci {
      reference: image.clone(),
      platform: execution_platform(*platform),
      isolation: match isolation {
        ProtocolOciIsolation::Process => ExecutionOciIsolation::Process,
        ProtocolOciIsolation::Hypervisor => ExecutionOciIsolation::Hypervisor,
      },
    },
  };
  let network = match &spec.runtime.network {
    NetworkPolicy::Unrestricted => NetworkAccess::Unrestricted,
    NetworkPolicy::Disabled => NetworkAccess::Disabled,
    NetworkPolicy::Restricted { allowed_hosts } => NetworkAccess::Restricted {
      allowed_hosts: allowed_hosts.clone(),
    },
  };
  (root, network)
}

fn execution_platform(platform: octacity_protocol::PlatformSpec) -> ExecutionPlatform {
  ExecutionPlatform {
    os: match platform.os {
      PlatformOs::Linux => ExecutionOs::Linux,
      PlatformOs::Windows => ExecutionOs::Windows,
      PlatformOs::Macos => ExecutionOs::Macos,
    },
    architecture: match platform.architecture {
      PlatformArchitecture::Amd64 => ExecutionArchitecture::Amd64,
      PlatformArchitecture::Arm64 => ExecutionArchitecture::Arm64,
    },
  }
}

fn remaining(deadline: Instant) -> Result<Duration, JobError> {
  deadline
    .checked_duration_since(Instant::now())
    .ok_or(JobError::TimedOut)
}

fn execution_id(spec: &JobSpecV1) -> String {
  let mut digest = Sha256::new();
  digest.update(spec.job_id.as_bytes());
  digest.update([0]);
  digest.update(spec.attempt.to_be_bytes());
  format!("job-{:x}", digest.finalize())
}

fn is_execution_directory(name: &str) -> bool {
  name.len() == 68
    && name.starts_with("job-")
    && name[4..]
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn create_private_directory(path: &Path) -> Result<(), JobError> {
  #[cfg(unix)]
  let builder = {
    use std::os::unix::fs::DirBuilderExt as _;

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    builder
  };
  #[cfg(not(unix))]
  let builder = fs::DirBuilder::new();

  builder
    .create(path)
    .map_err(|source| filesystem("create directory", path, source))
}

fn filesystem(action: &'static str, path: &Path, source: std::io::Error) -> JobError {
  JobError::Filesystem {
    action,
    path: path.to_owned(),
    source,
  }
}

fn map_runner_error(error: RunnerSupervisionError) -> JobError {
  match error {
    RunnerSupervisionError::Execution(ExecutionError::Cancelled) => JobError::Cancelled,
    RunnerSupervisionError::Execution(ExecutionError::TimedOut { .. }) => JobError::TimedOut,
    RunnerSupervisionError::StartupTimeout => JobError::TimedOut,
    error => JobError::Runner(Box::new(error)),
  }
}

#[cfg(test)]
mod tests;
