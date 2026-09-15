//! Owns the complete, backend-neutral lifecycle of one verified job.
//!
//! This crate is orchestration rather than infrastructure. It accepts an
//! already verified intent, creates one private workspace,
//! materializes the exact source revision, selects the requested execution
//! backend without fallback, drives `octa-runner`, and transfers every created
//! workspace to its caller for explicitly ordered cleanup. Concrete VCS,
//! process, microVM, and
//! transport mechanics remain behind their respective component interfaces.

use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use octacity_cache_session::{CacheSessionError, CacheSessionLifetime, CacheSessionManager, PreparedCacheSession};
use octacity_execution::{
  ExecutionArchitecture, ExecutionBackend, ExecutionError, ExecutionOs, ExecutionPlatform, ExecutionTarget,
  NetworkAccess, OciIsolation as ExecutionOciIsolation, StartExecution,
};
use octacity_identity::{WorkloadIdentityError, WorkloadIdentityLease, WorkloadIdentityProvider};
use octacity_protocol::{
  BeginCacheSessionResponse, JobSpecV1, NetworkPolicy, OciIsolation as ProtocolOciIsolation, OutputLimits,
  PlatformArchitecture, PlatformOs, RuntimeMode, RuntimeTarget,
};
use octacity_runner::{
  RunnerCompletion, RunnerInstallation, RunnerInstallationError, RunnerJobRequest, RunnerRedactions, RunnerStreamItem,
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
  /// Fenced cache authority obtained out of band from the signed JobSpec.
  pub cache_grant: Option<BeginCacheSessionResponse>,
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

/// Failed execution plus any workspace that must be cleaned by the caller.
///
/// Keeping the retained path with the failure lets the outer durable lifecycle
/// record `Cleaning` before it invokes the destructive operation. A failure
/// raised before workspace creation carries no cleanup responsibility.
#[derive(Debug)]
pub struct JobFailure {
  error: JobError,
  job_root: Option<PathBuf>,
}

impl JobFailure {
  fn without_workspace(error: JobError) -> Self {
    Self { error, job_root: None }
  }

  fn with_workspace(error: JobError, job_root: PathBuf) -> Self {
    Self {
      error,
      job_root: Some(job_root),
    }
  }

  /// Returns the operation failure independently of cleanup state.
  pub fn error(&self) -> &JobError {
    &self.error
  }

  /// Removes retained filesystem state after the caller durably records its
  /// cleaning phase. Absence is valid for preflight failures.
  pub async fn cleanup(mut self) -> Result<(), JobError> {
    let Some(job_root) = self.job_root.as_ref() else {
      return Ok(());
    };
    match tokio::fs::remove_dir_all(job_root).await {
      Ok(()) => {
        self.job_root = None;
        Ok(())
      }
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
        self.job_root = None;
        Ok(())
      }
      Err(error) => Err(JobError::Cleanup(error)),
    }
  }
}

impl Drop for JobFailure {
  fn drop(&mut self) {
    if let Some(job_root) = &self.job_root {
      warn!(path = %job_root.display(), "job failure dropped before workspace cleanup");
    }
  }
}

impl std::fmt::Display for JobFailure {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    self.error.fmt(formatter)
  }
}

impl std::error::Error for JobFailure {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    self.error.source()
  }
}

impl From<JobError> for JobFailure {
  fn from(error: JobError) -> Self {
    Self::without_workspace(error)
  }
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
  /// Allows a signed job to select unrestricted network access.
  pub allow_unrestricted_network: bool,
  /// Complete local allowlist from which restricted jobs may select hosts.
  pub allowed_network_hosts: Vec<String>,
  /// Agent-owned maxima applied to signed artifact and report quotas.
  pub max_output_limits: OutputLimits,
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
  #[error("workload identity failed: {0}")]
  /// Local workload identity selection, provisioning, or revocation failed.
  WorkloadIdentity(#[source] Box<WorkloadIdentityError>),
  /// Cache grant validation, private bearer provisioning, or revocation failed.
  #[error("cache session failed: {0}")]
  CacheSession(#[source] Box<CacheSessionError>),
  /// Execution and cache credential revocation both failed.
  #[error("job failed ({operation}) and cache session revocation also failed: {revocation}")]
  OperationAndCacheRevocation {
    /// Original job lifecycle failure.
    operation: Box<JobError>,
    /// Additional private cache cleanup failure.
    revocation: Box<CacheSessionError>,
  },
  #[error("job failed ({operation}) and workload identity revocation also failed: {revocation}")]
  /// Both execution and mandatory identity revocation failed.
  OperationAndIdentityRevocation {
    /// Original job lifecycle failure.
    operation: Box<JobError>,
    /// Additional identity revocation failure.
    revocation: Box<WorkloadIdentityError>,
  },
  #[error("job requests {requested} workspace bytes, exceeding the agent limit {maximum}")]
  /// The signed writable-disk request exceeds local agent policy.
  WorkspaceLimit {
    /// Bytes requested by the signed job.
    requested: u64,
    /// Maximum bytes allowed by the agent.
    maximum: u64,
  },
  #[error("job network policy exceeds the agent's local policy: {0}")]
  /// The signed network request is broader than local operator policy.
  NetworkPolicy(String),
  #[error("job output limits exceed the agent's local maxima")]
  /// At least one signed artifact or report quota exceeds local policy.
  OutputLimit,
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
  #[error("orphan cleanup failed: {0}")]
  /// One or more backend resources or abandoned workspaces could not be removed.
  OrphanCleanup(String),
}

/// Immutable dependencies and policy limits used for every job on an agent.
pub struct JobExecutor {
  runner: RunnerInstallation,
  source: Arc<dyn SourceMaterializer>,
  identity: Arc<dyn WorkloadIdentityProvider>,
  cache: Option<Arc<CacheSessionManager>>,
  backends: BTreeMap<RuntimeMode, Arc<dyn ExecutionBackend>>,
  work_root: PathBuf,
  max_workspace_bytes: u64,
  allow_unrestricted_network: bool,
  allowed_network_hosts: BTreeSet<String>,
  max_output_limits: OutputLimits,
  cancellation_grace: Duration,
  runner_supervision: RunnerSupervisionPolicy,
}

impl JobExecutor {
  /// Validates immutable agent dependencies and constructs a job orchestrator.
  pub fn new(
    runner: RunnerInstallation,
    source: Arc<dyn SourceMaterializer>,
    identity: Arc<dyn WorkloadIdentityProvider>,
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
    config.max_output_limits.validate().map_err(JobError::Invalid)?;
    let configured_network_host_count = config.allowed_network_hosts.len();
    let allowed_network_hosts = config.allowed_network_hosts.into_iter().collect::<BTreeSet<_>>();
    if allowed_network_hosts.len() != configured_network_host_count {
      return Err(JobError::Invalid(
        "allowed network hosts must not contain duplicates".to_owned(),
      ));
    }
    if allowed_network_hosts
      .iter()
      .any(|host| host.trim() != host || host.is_empty() || host.chars().any(char::is_control))
    {
      return Err(JobError::Invalid(
        "allowed network hosts must be non-empty trimmed values without control characters".to_owned(),
      ));
    }
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
      identity,
      cache: None,
      backends,
      work_root,
      max_workspace_bytes: config.max_workspace_bytes,
      allow_unrestricted_network: config.allow_unrestricted_network,
      allowed_network_hosts,
      max_output_limits: config.max_output_limits,
      cancellation_grace: config.cancellation_grace,
      runner_supervision: config.runner_supervision,
    })
  }

  /// Enables job-scoped cache grants for an installation that advertises them.
  pub fn with_cache(mut self, cache: Arc<CacheSessionManager>) -> Self {
    self.cache = Some(cache);
    self
  }

  /// Runs one verified job and retains its workspace for output processing.
  /// The caller must finish by invoking [`JobCompletion::cleanup`].
  pub async fn execute(
    &self,
    request: ExecuteJobRequest,
    cancellation: CancellationToken,
    events: &mpsc::Sender<RunnerStreamItem>,
  ) -> Result<JobCompletion, JobFailure> {
    let spec = request.spec;
    if spec.cache.is_some() != request.cache_grant.is_some() {
      return Err(
        JobError::Invalid("signed cache policy and fenced cache grant must be present together".to_owned()).into(),
      );
    }

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
      return Err(
        JobError::WorkspaceLimit {
          requested: spec.runtime.writable_disk_bytes,
          maximum: self.max_workspace_bytes,
        }
        .into(),
      );
    }
    if !spec.outputs.is_within(&self.max_output_limits) {
      return Err(JobError::OutputLimit.into());
    }
    let network = self.authorize_network(&spec.runtime.network)?;
    if let Some(profile) = &spec.runtime.workload_identity_profile
      && !self.identity.supports(profile)
    {
      return Err(JobError::WorkloadIdentity(Box::new(WorkloadIdentityError::UnknownProfile(profile.clone()))).into());
    }
    if cancellation.is_cancelled() {
      return Err(JobError::Cancelled.into());
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
      let identity = match &spec.runtime.workload_identity_profile {
        Some(profile) => Some(
          self
            .identity
            .provision(profile, &job_root, &cancellation)
            .await
            .map_err(|error| JobError::WorkloadIdentity(Box::new(error)))?,
        ),
        None => None,
      };
      let cache = match (&spec.cache, request.cache_grant) {
        (Some(policy), Some(grant)) => {
          let preparation = self
            .cache
            .as_ref()
            .ok_or_else(|| JobError::Invalid("cache support is not configured".to_owned()))?
            .prepare(
              policy,
              grant,
              &spec.runtime.target,
              &spec.runtime.network,
              &job_root,
              CacheSessionLifetime {
                now: unix_now()?,
                remaining_job: remaining(deadline)?,
              },
            )
            .await
            .map_err(|error| JobError::CacheSession(Box::new(error)));
          match preparation {
            Ok(cache) => Some(cache),
            Err(error) => return finish_identity(Err(error), identity).await,
          }
        }
        (None, None) => None,
        _ => {
          return finish_identity(
            Err(JobError::Invalid(
              "signed cache policy and fenced cache grant must be present together".to_owned(),
            )),
            identity,
          )
          .await;
        }
      };
      let root = execution_target(&spec);
      let operation = async {
        supervise(
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
              workload_identity: identity.as_ref().map(|lease| lease.path().to_owned()),
              cache: cache.as_ref().map(|session| session.execution().clone()),
              cpu_millis: spec.runtime.cpu_millis,
              memory_bytes: spec.runtime.memory_bytes,
              writable_disk_bytes: spec.runtime.writable_disk_bytes,
              max_duration: remaining(deadline)?,
              root,
              network,
            },
            spec: spec.execution.clone(),
            cache: cache.as_ref().map(|session| session.runner().clone()),
            redactions: RunnerRedactions::new(
              identity
                .iter()
                .map(|lease| lease.sensitive_value())
                .chain(cache.iter().filter_map(|session| session.sensitive_value())),
            ),
            cancellation_grace: self.cancellation_grace,
          },
          cancellation,
          events,
          &self.runner_supervision,
        )
        .await
        .map_err(map_runner_error)
      }
      .await;
      let operation = finish_identity(operation, identity).await;
      let runner = finish_cache_session(operation, cache).await?;

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
      Err(operation) => Err(JobFailure::with_workspace(operation, job_root)),
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

  /// Narrows a signed request through the operator-owned network policy.
  fn authorize_network(&self, requested: &NetworkPolicy) -> Result<NetworkAccess, JobError> {
    match requested {
      NetworkPolicy::Disabled => Ok(NetworkAccess::Disabled),
      NetworkPolicy::Unrestricted if self.allow_unrestricted_network => Ok(NetworkAccess::Unrestricted),
      NetworkPolicy::Unrestricted => Err(JobError::NetworkPolicy("unrestricted access is disabled".to_owned())),
      NetworkPolicy::Restricted { allowed_hosts } => {
        if let Some(host) = allowed_hosts
          .iter()
          .find(|host| !self.allowed_network_hosts.contains(host.as_str()))
        {
          return Err(JobError::NetworkPolicy(format!(
            "host '{host}' is absent from the local allowlist"
          )));
        }
        Ok(NetworkAccess::Restricted {
          allowed_hosts: allowed_hosts.clone(),
        })
      }
    }
  }
}

/// Makes cache-token removal authoritative without hiding execution failure.
async fn finish_cache_session<T>(
  operation: Result<T, JobError>,
  cache: Option<PreparedCacheSession>,
) -> Result<T, JobError> {
  let revocation = match cache {
    Some(cache) => cache.revoke().await,
    None => Ok(()),
  };
  match (operation, revocation) {
    (Ok(value), Ok(())) => Ok(value),
    (Err(operation), Ok(())) => Err(operation),
    (Ok(_), Err(revocation)) => Err(JobError::CacheSession(Box::new(revocation))),
    (Err(operation), Err(revocation)) => Err(JobError::OperationAndCacheRevocation {
      operation: Box::new(operation),
      revocation: Box::new(revocation),
    }),
  }
}

fn unix_now() -> Result<u64, JobError> {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|duration| duration.as_secs())
    .map_err(|_| JobError::Invalid("system clock is before the Unix epoch".to_owned()))
}

/// Makes identity revocation authoritative without hiding an execution error.
async fn finish_identity<T>(
  operation: Result<T, JobError>,
  identity: Option<WorkloadIdentityLease>,
) -> Result<T, JobError> {
  let revocation = match identity {
    Some(identity) => identity.revoke().await,
    None => Ok(()),
  };
  match (operation, revocation) {
    (Ok(value), Ok(())) => Ok(value),
    (Err(operation), Ok(())) => Err(operation),
    (Ok(_), Err(revocation)) => Err(JobError::WorkloadIdentity(Box::new(revocation))),
    (Err(operation), Err(revocation)) => Err(JobError::OperationAndIdentityRevocation {
      operation: Box::new(operation),
      revocation: Box::new(revocation),
    }),
  }
}

fn execution_target(spec: &JobSpecV1) -> ExecutionTarget {
  match &spec.runtime.target {
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
  }
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
  octacity_private_fs::create_private_directory(path).map_err(|source| filesystem("create directory", path, source))
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
