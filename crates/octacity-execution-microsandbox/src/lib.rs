//! Runs `octa-runner` inside an attached Microsandbox microVM.
//!
//! The adapter maps the backend-neutral execution contract to one ephemeral,
//! digest-pinned sandbox. The workspace is the only disk-backed writable mount
//! and has the job's quota; the OCI root overlay is RAM-backed and therefore
//! charged to the VM memory limit. The verified Octa release is mounted
//! read-only. Runner stdin/stdout/stderr remain byte streams, so the
//! higher-level supervisor uses exactly the same protocol in Native and
//! isolated execution. A task-scoped SDK backend keeps Microsandbox databases,
//! images, logs, and VM state under the agent's configured state root instead
//! of ambient user directories.

use std::{
  fs,
  path::{Path, PathBuf},
  time::Duration,
};

use async_trait::async_trait;
use microsandbox::{Backend, ExecControl, ExecEvent, LocalBackend, NetworkPolicy, Sandbox, with_backend};
use octacity_execution::{
  ExecutionArchitecture, ExecutionError, ExecutionExit, ExecutionIo, ExecutionOs, ExecutionPaths, ExecutionPlatform,
  ExecutionReader, ExecutionTarget, ExecutionWriter, NetworkAccess, OciIsolation, ResourceUsage, RunnerProgram,
  RunningExecution, StartExecution,
};
use octacity_execution_oci::{OciCapability, OciEngine};
use sha2::{Digest as _, Sha256};
use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  sync::oneshot,
  task::JoinHandle,
  time::{Instant, timeout, timeout_at},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Stable scheduler and diagnostics identifier for this execution engine.
pub const MICROSANDBOX_ENGINE_NAME: &str = "microsandbox";

mod execution;
mod plan;

use execution::{MicrosandboxExecution, pump_events, pump_stdin};
use plan::{SandboxPlan, canonical_runtime_file, directory_size_async};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const MEBIBYTE: u64 = 1024 * 1024;
const OWNER_LABEL: &str = "octacity.agent";
const GUEST_PLATFORM: Option<&str> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
  Some("linux-x86_64")
} else if cfg!(any(
  all(target_os = "linux", target_arch = "aarch64"),
  all(target_os = "macos", target_arch = "aarch64")
)) {
  Some("linux-aarch64")
} else {
  None
};
const GUEST_CAPABILITY: Option<OciCapability> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
  Some(OciCapability {
    platform: ExecutionPlatform {
      os: ExecutionOs::Linux,
      architecture: ExecutionArchitecture::Amd64,
    },
    isolation: OciIsolation::Hypervisor,
  })
} else if cfg!(any(
  all(target_os = "linux", target_arch = "aarch64"),
  all(target_os = "macos", target_arch = "aarch64")
)) {
  Some(OciCapability {
    platform: ExecutionPlatform {
      os: ExecutionOs::Linux,
      architecture: ExecutionArchitecture::Arm64,
    },
    isolation: OciIsolation::Hypervisor,
  })
} else {
  None
};

/// OCI hypervisor engine using the official embedded Microsandbox Rust SDK.
pub struct MicrosandboxEngine {
  agent_id: String,
  work_root: PathBuf,
  executable: PathBuf,
  libkrunfw: PathBuf,
  backend: std::sync::Arc<dyn Backend>,
  cleanup_timeout: Duration,
  metrics_sample_interval: Duration,
}

/// Operator-installed Microsandbox runtime and agent-owned state roots.
#[derive(Clone, Debug)]
pub struct MicrosandboxEngineConfig {
  /// Stable agent owner identifier placed on sandbox labels.
  pub agent_id: String,
  /// Agent-owned root for Microsandbox persistent state.
  pub state_root: PathBuf,
  /// Agent-owned job workspace root.
  pub work_root: PathBuf,
  /// Linux guest platform required by the installed runner.
  pub runner_platform: String,
  /// Exact `msb` executable.
  pub executable: PathBuf,
  /// Exact libkrun firmware library.
  pub libkrunfw: PathBuf,
  /// Bound for sandbox cleanup operations.
  pub cleanup_timeout: Duration,
  /// Interval at which the SDK collects microVM accounting samples.
  pub metrics_sample_interval: Duration,
}

impl MicrosandboxEngine {
  /// Validates operator-owned runtime paths and initializes an isolated SDK backend.
  pub fn new(config: MicrosandboxEngineConfig) -> Result<Self, ExecutionError> {
    let MicrosandboxEngineConfig {
      agent_id,
      state_root,
      work_root,
      runner_platform,
      executable,
      libkrunfw,
      cleanup_timeout,
      metrics_sample_interval,
    } = config;
    let guest_platform = GUEST_PLATFORM.ok_or_else(|| {
      unavailable(format!(
        "Microsandbox execution is unsupported on {}-{}; supported hosts are Linux x86_64/aarch64 and macOS aarch64",
        std::env::consts::OS,
        std::env::consts::ARCH
      ))
    })?;
    if runner_platform != guest_platform {
      return Err(invalid(format!(
        "Microsandbox on this host requires runner platform '{guest_platform}', got '{runner_platform}'"
      )));
    }
    if agent_id.trim().is_empty() || agent_id.chars().any(char::is_control) {
      return Err(invalid("agent_id must not be empty or contain control characters"));
    }
    if cleanup_timeout.is_zero() || metrics_sample_interval.is_zero() {
      return Err(invalid(
        "cleanup timeout and metrics sample interval must be greater than zero",
      ));
    }
    if !state_root.is_absolute() || !state_root.is_dir() {
      return Err(invalid("state_root must be an existing absolute directory"));
    }
    let state_root = state_root
      .canonicalize()
      .map_err(|error| backend(format!("canonicalize Microsandbox state root: {error}")))?;
    if !work_root.is_absolute() || !work_root.is_dir() {
      return Err(invalid("work_root must be an existing absolute directory"));
    }
    let work_root = work_root
      .canonicalize()
      .map_err(|error| backend(format!("canonicalize Microsandbox work root: {error}")))?;
    let executable = canonical_runtime_file("Microsandbox executable", &executable)?;
    let libkrunfw = canonical_runtime_file("Microsandbox libkrunfw", &libkrunfw)?;
    let backend = LocalBackend::builder()
      .home(state_root.join("microsandbox"))
      .try_build_lazy()
      .map_err(msb_error)?;
    Ok(Self {
      agent_id,
      work_root,
      executable,
      libkrunfw,
      backend: std::sync::Arc::new(backend),
      cleanup_timeout,
      metrics_sample_interval,
    })
  }
}

#[async_trait]
impl OciEngine for MicrosandboxEngine {
  fn capabilities(&self) -> Vec<OciCapability> {
    GUEST_CAPABILITY.into_iter().collect()
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
    let deadline = Instant::now()
      .checked_add(request.max_duration)
      .ok_or_else(|| invalid("Microsandbox execution timeout is too large"))?;
    self.configure_runtime()?;
    let capability = requested_capability(&request)?;
    if Some(capability) != GUEST_CAPABILITY {
      return Err(unavailable(format!(
        "Microsandbox does not provide requested OCI capability {capability:?}"
      )));
    }
    let request_root = request.workspace_root.canonicalize().map_err(ExecutionError::Io)?;
    if request_root != self.work_root {
      return Err(invalid(
        "execution workspace_root does not match the Microsandbox backend work_root",
      ));
    }
    let plan = before_deadline(
      deadline,
      &cancellation,
      "building the Microsandbox plan",
      SandboxPlan::build(&self.agent_id, runner, &request),
    )
    .await??;
    let network = network_policy(&request.network)?;
    info!(execution_id = %request.execution_id, sandbox = %plan.name, image = %plan.image, "starting Microsandbox");
    let creation_result = {
      let creation = with_backend(self.backend.clone(), async {
        let mut builder = Sandbox::builder(&plan.name)
          .image(plan.image.clone())
          .root_disk_with(|disk| disk.tmpfs().size(plan.root_tmpfs_mib))
          .cpus(plan.cpus)
          .memory(plan.memory_mib)
          .max_duration(duration_seconds_ceil(request.max_duration))
          .metrics_sample_interval(self.metrics_sample_interval)
          .label(OWNER_LABEL, &self.agent_id)
          .volume(&plan.guest_workspace, |mount| {
            mount
              .bind(&request.workspace)
              .quota(plan.workspace_quota_mib)
              .nosuid()
              .nodev()
          })
          .volume(&plan.guest_release, |mount| {
            mount.bind(&runner.release_root).readonly().nosuid().nodev()
          });
        builder = match network {
          Some(policy) => builder.network(|configuration| configuration.policy(policy)),
          None => builder.disable_network(),
        };
        builder.create().await
      });
      before_deadline(deadline, &cancellation, "starting Microsandbox", creation)
        .await?
        .map_err(msb_error)
    };
    // End the SDK creation future before looking up its exact persisted name;
    // otherwise creation and rollback could mutate the same sandbox at once.
    let sandbox = match creation_result {
      Ok(sandbox) => sandbox,
      Err(operation) => {
        return Err(
          cleanup_named_sandbox_after_start_failure(self.backend.clone(), &plan.name, self.cleanup_timeout, operation)
            .await,
        );
      }
    };
    if cancellation.is_cancelled() {
      return Err(cleanup_after_start_failure(&sandbox, self.cleanup_timeout, ExecutionError::Cancelled).await);
    }
    let exec = sandbox.exec_stream_with(&plan.guest_executable, |options| {
      options.stdin_pipe().cwd(&plan.guest_workspace)
    });
    let mut handle = match before_deadline(deadline, &cancellation, "starting the Microsandbox runner", exec).await {
      Ok(Ok(handle)) => handle,
      Ok(Err(error)) => {
        return Err(cleanup_after_start_failure(&sandbox, self.cleanup_timeout, msb_error(error)).await);
      }
      Err(error) => {
        return Err(cleanup_after_start_failure(&sandbox, self.cleanup_timeout, error).await);
      }
    };
    if cancellation.is_cancelled() {
      drop(handle);
      return Err(cleanup_after_start_failure(&sandbox, self.cleanup_timeout, ExecutionError::Cancelled).await);
    }
    let control = handle.control();
    let stdin = match handle.take_stdin() {
      Some(stdin) => stdin,
      None => {
        return Err(cleanup_after_start_failure(&sandbox, self.cleanup_timeout, ExecutionError::IoTaken).await);
      }
    };
    let (supervisor_stdin, bridge_stdin) = tokio::io::duplex(STREAM_BUFFER_BYTES);
    let (bridge_stdout, supervisor_stdout) = tokio::io::duplex(STREAM_BUFFER_BYTES);
    let (bridge_stderr, supervisor_stderr) = tokio::io::duplex(STREAM_BUFFER_BYTES);
    let stdin_task = tokio::spawn(pump_stdin(bridge_stdin, stdin));
    let (exit_sender, exit_receiver) = oneshot::channel();
    let event_task = tokio::spawn(pump_events(handle, bridge_stdout, bridge_stderr, exit_sender));

    Ok(Box::new(MicrosandboxExecution {
      sandbox,
      control,
      io: Some(ExecutionIo {
        stdin: Box::pin(supervisor_stdin) as ExecutionWriter,
        stdout: Box::pin(supervisor_stdout) as ExecutionReader,
        stderr: Box::pin(supervisor_stderr) as ExecutionReader,
      }),
      paths: ExecutionPaths {
        workspace: PathBuf::from(&plan.guest_workspace),
        data_dir: plan.guest_data_dir,
        plugins_dir: plan.guest_plugins_dir,
        plugin_lock: plan.guest_plugin_lock,
      },
      exit_receiver: Some(exit_receiver),
      exit_code: None,
      stdin_task,
      event_task,
      cleanup_timeout: self.cleanup_timeout,
      started: Instant::now(),
      host_workspace: request.workspace,
      memory_peak_bytes: 0,
      disk_peak_bytes: 0,
    }))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    self.configure_runtime()?;
    let handles = with_backend(self.backend.clone(), async {
      let mut cursor = None;
      let mut handles = Vec::new();
      loop {
        let page = Sandbox::list_with(|list| {
          let list = list.limit(100).label(OWNER_LABEL, &self.agent_id);
          match &cursor {
            Some(cursor) => list.cursor(cursor),
            None => list,
          }
        })
        .await?;
        handles.extend(page.sandboxes);
        cursor = page.next_cursor;
        if cursor.is_none() {
          break;
        }
      }
      Ok::<_, microsandbox::MicrosandboxError>(handles)
    })
    .await
    .map_err(msb_error)?;

    let mut first_error = None;
    for handle in handles {
      if let Err(error) = handle.stop_with_timeout(self.cleanup_timeout).await {
        warn!(sandbox = %handle.name(), error = %error, "graceful orphan cleanup failed; forcing stop");
        if let Err(error) = handle.kill_with_timeout(self.cleanup_timeout).await
          && first_error.is_none()
        {
          first_error = Some(msb_error(error));
        }
      }
      if let Err(error) = handle.remove().await
        && first_error.is_none()
      {
        first_error = Some(msb_error(error));
      }
    }
    first_error.map_or(Ok(()), Err)
  }
}

impl MicrosandboxEngine {
  fn configure_runtime(&self) -> Result<(), ExecutionError> {
    microsandbox::config::set_sdk_msb_path(&self.executable);
    microsandbox::set_libkrunfw_path(&self.libkrunfw);
    let local = self
      .backend
      .as_local()
      .ok_or_else(|| backend("Microsandbox engine requires a local backend"))?;
    let resolved_executable = local.config().resolve_msb_path().map_err(msb_error)?;
    let resolved_libkrunfw = local.config().resolve_libkrunfw_path().map_err(msb_error)?;
    if resolved_executable != self.executable || resolved_libkrunfw != self.libkrunfw {
      return Err(unavailable(
        "Microsandbox runtime was already configured with different process-global paths",
      ));
    }
    Ok(())
  }
}

fn requested_capability(request: &StartExecution) -> Result<OciCapability, ExecutionError> {
  let ExecutionTarget::Oci {
    platform, isolation, ..
  } = &request.root
  else {
    return Err(unavailable("Microsandbox execution requires an OCI root image"));
  };
  Ok(OciCapability {
    platform: *platform,
    isolation: *isolation,
  })
}

async fn before_deadline<T, F>(
  deadline: Instant,
  cancellation: &CancellationToken,
  operation: &'static str,
  future: F,
) -> Result<T, ExecutionError>
where
  F: std::future::Future<Output = T>,
{
  tokio::select! {
    biased;
    () = cancellation.cancelled() => Err(ExecutionError::Cancelled),
    result = timeout_at(deadline, future) => result.map_err(|_| ExecutionError::TimedOut { operation }),
  }
}

fn network_policy(access: &NetworkAccess) -> Result<Option<NetworkPolicy>, ExecutionError> {
  match access {
    NetworkAccess::Unrestricted => Ok(Some(NetworkPolicy::allow_all())),
    NetworkAccess::Disabled => Ok(None),
    NetworkAccess::Restricted { allowed_hosts } => NetworkPolicy::builder()
      .default_deny()
      .egress(|rules| rules.tcp().udp().port(53).allow_host())
      .egress(|rules| rules.tcp().allow_domains(allowed_hosts.iter().cloned()))
      .build()
      .map(Some)
      .map_err(|error| unavailable(format!("invalid Microsandbox network policy: {error}"))),
  }
}

async fn cleanup_after_start_failure(
  sandbox: &Sandbox,
  timeout: Duration,
  operation: ExecutionError,
) -> ExecutionError {
  match cleanup_sandbox_result(sandbox, timeout).await {
    Ok(()) => operation,
    Err(cleanup) => backend(format!("{operation}; Microsandbox cleanup also failed: {cleanup}")),
  }
}

async fn cleanup_named_sandbox_after_start_failure(
  backend_handle: std::sync::Arc<dyn Backend>,
  name: &str,
  timeout: Duration,
  operation: ExecutionError,
) -> ExecutionError {
  let cleanup = with_backend(backend_handle, async {
    let handle = match Sandbox::get(name).await {
      Ok(handle) => handle,
      Err(microsandbox::MicrosandboxError::SandboxNotFound(_)) => return Ok(()),
      Err(error) => return Err(msb_error(error)),
    };
    if let Err(error) = handle.stop_with_timeout(timeout).await {
      warn!(sandbox = name, error = %error, "graceful Microsandbox startup rollback failed; forcing stop");
      handle.kill_with_timeout(timeout).await.map_err(msb_error)?;
    }
    handle.remove().await.map_err(msb_error)
  })
  .await;
  match cleanup {
    Ok(()) => operation,
    Err(cleanup) => backend(format!("{operation}; Microsandbox cleanup also failed: {cleanup}")),
  }
}

async fn cleanup_sandbox_result(sandbox: &Sandbox, timeout: Duration) -> Result<(), ExecutionError> {
  let mut stop_error = None;
  if let Err(error) = sandbox.stop_with_timeout(timeout).await {
    warn!(sandbox = %sandbox.name(), error = %error, "graceful Microsandbox stop failed; forcing stop");
    if let Err(error) = sandbox.kill_with_timeout(timeout).await {
      stop_error = Some(msb_error(error));
    }
  }
  let remove_error = match sandbox.remove_persisted().await {
    Ok(()) => {
      debug!(sandbox = %sandbox.name(), "removed Microsandbox state");
      None
    }
    Err(error) => Some(msb_error(error)),
  };
  match (stop_error, remove_error) {
    (None, None) => Ok(()),
    (Some(error), None) | (None, Some(error)) => Err(error),
    (Some(stop), Some(remove)) => Err(backend(format!(
      "{stop}; persisted-state removal also failed: {remove}"
    ))),
  }
}

fn millis(duration: Duration) -> u64 {
  u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn duration_seconds_ceil(duration: Duration) -> u64 {
  duration.as_secs() + u64::from(duration.subsec_nanos() != 0)
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

fn msb_error(error: microsandbox::MicrosandboxError) -> ExecutionError {
  backend(error.to_string())
}

#[cfg(test)]
mod tests;
