//! Apple Silicon implementation backed by Apple's official `container` CLI.

use std::{fs, path::PathBuf, process::Stdio, time::Duration};

use async_trait::async_trait;
use octacity_execution::{
  ExecutionArchitecture, ExecutionError, ExecutionOs, ExecutionPlatform, OciIsolation, RunnerProgram, RunningExecution,
  StartExecution,
};
use octacity_execution_oci::{OciCapability, OciEngine};
use tokio::{
  process::Command,
  time::{Instant, timeout},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::AppleVfEngineConfig;

mod execution;
mod filesystem;
mod plan;

use execution::AppleVfExecution;
use filesystem::{canonical_directory, canonical_executable, validate_workspace_filesystem};
use plan::AppleVfPlan;

const GUEST_CAPABILITY: OciCapability = OciCapability {
  platform: ExecutionPlatform {
    os: ExecutionOs::Linux,
    architecture: ExecutionArchitecture::Arm64,
  },
  isolation: OciIsolation::Process,
};

/// Isolation provider using Apple's per-container Virtualization.framework VMs.
pub struct AppleVfEngine {
  executable: PathBuf,
  work_root: PathBuf,
  marker_root: PathBuf,
  owner_identity: String,
  cleanup_timeout: Duration,
  open_files_limit: u64,
}

impl AppleVfEngine {
  /// Validates operator paths and prepares the private cleanup-marker root.
  pub fn new(config: AppleVfEngineConfig) -> Result<Self, ExecutionError> {
    if config.agent_id.trim().is_empty() || config.agent_id.chars().any(char::is_control) {
      return Err(invalid("agent_id must not be empty or contain control characters"));
    }
    if config.runner_platform != "linux-aarch64" {
      return Err(invalid(format!(
        "Apple VF isolation requires runner platform 'linux-aarch64', got '{}'",
        config.runner_platform
      )));
    }
    if config.max_workspace_bytes == 0 || config.open_files_limit == 0 || config.cleanup_timeout.is_zero() {
      return Err(invalid(
        "workspace, open-file, and cleanup limits must be greater than zero",
      ));
    }
    let executable = canonical_executable(&config.executable)?;
    let work_root = canonical_directory("work_root", &config.work_root)?;
    validate_workspace_filesystem(&work_root, &work_root, config.max_workspace_bytes)?;
    let state_root = canonical_directory("state_root", &config.state_root)?;
    let owner_identity = plan::owner_identity(&config.agent_id);
    let marker_root = state_root.join("apple-vf-isolation").join(&owner_identity);
    fs::create_dir_all(&marker_root).map_err(ExecutionError::Io)?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(&marker_root, fs::Permissions::from_mode(0o700)).map_err(ExecutionError::Io)?;
    }
    let marker_root = canonical_directory("Apple VF marker root", &marker_root)?;
    Ok(Self {
      executable,
      work_root,
      marker_root,
      owner_identity,
      cleanup_timeout: config.cleanup_timeout,
      open_files_limit: config.open_files_limit,
    })
  }

  /// Confirms that both the CLI and its privileged API service are available.
  pub async fn validate_connection(&self) -> Result<(), ExecutionError> {
    let output = control_output(
      &self.executable,
      &["system", "version", "--format", "json"],
      self.cleanup_timeout,
      None,
      "query Apple container service version",
    )
    .await?;
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
      .map_err(|error| backend(format!("parse Apple container service version: {error}")))?;
    let Some(entries) = document.as_array() else {
      return Err(backend("Apple container version response is not an array"));
    };
    if entries.len() < 2 {
      return Err(ExecutionError::Unavailable(
        "Apple container API service is not running".to_owned(),
      ));
    }
    Ok(())
  }
}

#[async_trait]
impl OciEngine for AppleVfEngine {
  fn capabilities(&self) -> Vec<OciCapability> {
    vec![GUEST_CAPABILITY]
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
    validate_request(&self.work_root, &request)?;
    let deadline = Instant::now()
      .checked_add(request.max_duration)
      .ok_or_else(|| invalid("Apple VF execution timeout is too large"))?;
    let plan = AppleVfPlan::build(
      &self.owner_identity,
      &self.marker_root,
      self.open_files_limit,
      runner,
      &request,
    )?;

    control_output_owned(
      &self.executable,
      plan.pull_arguments(),
      remaining(deadline, "pull immutable Apple VF image")?,
      Some(&cancellation),
      "pull immutable Apple VF image",
    )
    .await?;
    plan.create_marker()?;
    let create = control_output_owned(
      &self.executable,
      plan.create_arguments(),
      remaining(deadline, "create Apple VF container")?,
      Some(&cancellation),
      "create Apple VF container",
    )
    .await;
    if let Err(operation) = create {
      return Err(cleanup_failed_start(&self.executable, &plan, self.cleanup_timeout, operation).await);
    }
    if cancellation.is_cancelled() {
      return Err(cleanup_failed_start(&self.executable, &plan, self.cleanup_timeout, ExecutionError::Cancelled).await);
    }

    let mut command = Command::new(&self.executable);
    command
      .args(["start", "--attach", "--interactive"])
      .arg(&plan.container_name)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);
    let child = match command.spawn().map_err(ExecutionError::Io) {
      Ok(child) => child,
      Err(operation) => {
        return Err(cleanup_failed_start(&self.executable, &plan, self.cleanup_timeout, operation).await);
      }
    };
    info!(container = %plan.container_name, image = %plan.image, "started Apple VF isolation workload");
    let execution = AppleVfExecution::new(
      self.executable.clone(),
      plan,
      child,
      request.workspace_root,
      self.cleanup_timeout,
    );
    match execution {
      Ok(execution) => Ok(Box::new(execution)),
      Err(failure) => {
        let (plan, operation) = *failure;
        Err(cleanup_failed_start(&self.executable, &plan, self.cleanup_timeout, operation).await)
      }
    }
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    let mut failures = Vec::new();
    for entry in fs::read_dir(&self.marker_root).map_err(ExecutionError::Io)? {
      let entry = entry.map_err(ExecutionError::Io)?;
      if !entry.file_type().map_err(ExecutionError::Io)?.is_file() {
        continue;
      }
      let marker = entry.path();
      let Some(container) = plan::container_from_marker(&marker, &self.owner_identity)? else {
        continue;
      };
      if let Err(error) = delete_container(&self.executable, &container, self.cleanup_timeout).await {
        warn!(%container, %error, "failed to remove orphaned Apple VF container");
        failures.push(error.to_string());
        continue;
      }
      if let Err(error) = fs::remove_file(&marker)
        && error.kind() != std::io::ErrorKind::NotFound
      {
        failures.push(format!("remove Apple VF marker '{}': {error}", marker.display()));
      }
    }
    if failures.is_empty() {
      Ok(())
    } else {
      Err(backend(failures.join("; ")))
    }
  }
}

fn validate_request(work_root: &std::path::Path, request: &StartExecution) -> Result<(), ExecutionError> {
  let requested = match &request.root {
    octacity_execution::ExecutionTarget::Oci {
      platform, isolation, ..
    } => OciCapability {
      platform: *platform,
      isolation: *isolation,
    },
    _ => return Err(invalid("Apple VF isolation requires an OCI execution target")),
  };
  if requested != GUEST_CAPABILITY {
    return Err(ExecutionError::Unavailable(format!(
      "Apple VF isolation does not provide requested capability {requested:?}"
    )));
  }
  if !matches!(request.network, octacity_execution::NetworkAccess::Disabled) {
    return Err(ExecutionError::Unavailable(
      "Apple VF isolation currently qualifies only disabled networking".to_owned(),
    ));
  }
  if request.cpu_millis < 1000 || !request.cpu_millis.is_multiple_of(1000) {
    return Err(ExecutionError::Unavailable(
      "Apple VF isolation CPU limits must use whole vCPUs".to_owned(),
    ));
  }
  validate_workspace_filesystem(work_root, &request.workspace, request.writable_disk_bytes)
}

pub(super) async fn delete_container(
  executable: &std::path::Path,
  container: &str,
  command_timeout: Duration,
) -> Result<(), ExecutionError> {
  let output = control_output(
    executable,
    &["delete", "--force", container],
    command_timeout,
    None,
    "delete Apple VF container",
  )
  .await;
  match output {
    Ok(_) => Ok(()),
    Err(ExecutionError::Backend(message)) if message.to_ascii_lowercase().contains("not found") => Ok(()),
    Err(error) => Err(error),
  }
}

pub(super) async fn control_output(
  executable: &std::path::Path,
  arguments: &[&str],
  command_timeout: Duration,
  cancellation: Option<&CancellationToken>,
  operation: &'static str,
) -> Result<std::process::Output, ExecutionError> {
  control_output_owned(
    executable,
    arguments.iter().map(|value| (*value).to_owned()).collect(),
    command_timeout,
    cancellation,
    operation,
  )
  .await
}

async fn control_output_owned(
  executable: &std::path::Path,
  arguments: Vec<String>,
  command_timeout: Duration,
  cancellation: Option<&CancellationToken>,
  operation: &'static str,
) -> Result<std::process::Output, ExecutionError> {
  let mut command = Command::new(executable);
  command.args(arguments).kill_on_drop(true);
  let future = command.output();
  let output = match cancellation {
    Some(cancellation) => {
      tokio::select! {
        () = cancellation.cancelled() => return Err(ExecutionError::Cancelled),
        result = timeout(command_timeout, future) => result,
      }
    }
    None => timeout(command_timeout, future).await,
  }
  .map_err(|_| ExecutionError::TimedOut { operation })?
  .map_err(ExecutionError::Io)?;
  if output.status.success() {
    Ok(output)
  } else {
    Err(backend(format!(
      "{operation} failed with {}: {}",
      output.status,
      String::from_utf8_lossy(&output.stderr).trim()
    )))
  }
}

async fn cleanup_failed_start(
  executable: &std::path::Path,
  plan: &AppleVfPlan,
  cleanup_timeout: Duration,
  operation: ExecutionError,
) -> ExecutionError {
  let cleanup = delete_container(executable, &plan.container_name, cleanup_timeout).await;
  let marker = plan.remove_marker();
  match (cleanup, marker) {
    (Ok(()), Ok(())) => operation,
    (Err(cleanup), Ok(())) => operation.with_cleanup(cleanup),
    (Ok(()), Err(cleanup)) => operation.with_cleanup(cleanup),
    (Err(first), Err(second)) => operation.with_cleanup(backend(format!("{first}; {second}"))),
  }
}

pub(super) fn invalid(message: impl Into<String>) -> ExecutionError {
  ExecutionError::Invalid(message.into())
}

pub(super) fn backend(message: impl Into<String>) -> ExecutionError {
  ExecutionError::Backend(message.into())
}

fn remaining(deadline: Instant, operation: &'static str) -> Result<Duration, ExecutionError> {
  deadline
    .checked_duration_since(Instant::now())
    .filter(|duration| !duration.is_zero())
    .ok_or(ExecutionError::TimedOut { operation })
}
