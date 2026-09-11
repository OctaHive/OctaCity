//! Linux implementation of the Native execution backend.

use std::{
  collections::BTreeMap,
  ffi::CString,
  fs::{self, File},
  io::{Seek as _, Write as _},
  os::fd::{AsRawFd as _, FromRawFd as _},
  os::unix::fs::{MetadataExt as _, PermissionsExt as _},
  os::unix::{ffi::OsStrExt as _, process::CommandExt as _},
  process::Stdio,
  time::{Duration, Instant},
};

use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, TargetArch};
use sha2::{Digest as _, Sha256};
use tokio::{
  process::{Child, Command},
  time::sleep,
};
use tracing::{debug, info};

use super::*;
use octacity_execution::{
  ExecutionArchitecture, ExecutionExit, ExecutionIo, ExecutionOs, ExecutionPaths, ExecutionPlatform, ResourceUsage,
};

#[path = "cgroup.rs"]
mod cgroup;
#[path = "filesystem.rs"]
mod filesystem;
#[path = "security.rs"]
mod security;

use cgroup::{
  attach_to_cgroup_before_exec, cgroup_name, configure_cgroup, is_cgroup_name, kill_cgroup, kill_process_group,
  read_cpu_time, read_io, read_number, remove_cgroup, validate_cgroup_root,
};
use filesystem::{filesystem_usage, validate_workspace_filesystem, validate_workspace_root};
use security::{
  add_native_filesystem, create_seccomp_filter, environment_arguments, host_execution_platform, network_arguments,
  validate_native_path,
};

const CPU_PERIOD_MICROS: u64 = 100_000;
const SECURITY_PROFILE_VERSION: &str = "native-linux-v1";
const CGROUP_CLEANUP_INTERVAL: Duration = Duration::from_millis(50);

/// Linux backend that confines the verified runner to a dedicated cgroup
/// and a quota-sized workspace filesystem.
pub struct NativeBackend {
  cgroup_root: PathBuf,
  work_root: PathBuf,
  bubblewrap: PathBuf,
  readonly_paths: Vec<PathBuf>,
  cleanup_timeout: Duration,
  environment: BTreeMap<String, String>,
  pids_limit: u32,
}

impl NativeBackend {
  /// Opens an operator-delegated cgroup-v2 root used exclusively by OctaCity.
  pub fn new(config: LinuxNativeConfig) -> Result<Self, ExecutionError> {
    let LinuxNativeConfig {
      cgroup_root,
      work_root,
      bubblewrap,
      readonly_paths,
      runner_platform,
      max_workspace_bytes,
      cleanup_timeout,
      pids_limit,
      environment,
    } = config;
    let host_platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    if runner_platform != host_platform {
      return Err(ExecutionError::Invalid(format!(
        "Native execution on this host requires runner platform '{host_platform}', got '{runner_platform}'"
      )));
    }
    if cleanup_timeout.is_zero() || pids_limit == 0 {
      return Err(ExecutionError::Invalid(
        "native cleanup timeout and process limit must be greater than zero".to_owned(),
      ));
    }
    let cgroup_root = cgroup_root
      .canonicalize()
      .map_err(|error| unavailable(format!("native cgroup root '{}': {error}", cgroup_root.display())))?;
    validate_cgroup_root(&cgroup_root)?;
    let work_root = work_root
      .canonicalize()
      .map_err(|error| unavailable(format!("native work root '{}': {error}", work_root.display())))?;
    validate_workspace_root(&work_root, max_workspace_bytes)?;
    let bubblewrap = bubblewrap
      .canonicalize()
      .map_err(|error| unavailable(format!("Bubblewrap executable '{}': {error}", bubblewrap.display())))?;
    if !bubblewrap.is_file() {
      return Err(unavailable(format!(
        "Bubblewrap executable '{}' is not a regular file",
        bubblewrap.display()
      )));
    }
    let mode = bubblewrap.metadata().map_err(ExecutionError::Io)?.permissions().mode();
    if mode & 0o111 == 0 || mode & 0o022 != 0 {
      return Err(unavailable(format!(
        "Bubblewrap executable '{}' must be executable and not writable by group or other users",
        bubblewrap.display()
      )));
    }
    let readonly_paths = readonly_paths
      .into_iter()
      .map(|path| {
        path
          .canonicalize()
          .map_err(|error| unavailable(format!("native read-only path '{}': {error}", path.display())))
      })
      .collect::<Result<Vec<_>, _>>()?;
    if readonly_paths.iter().any(|path| path == Path::new("/")) {
      return Err(ExecutionError::Invalid(
        "native_linux_readonly_paths must not expose the complete host root".to_owned(),
      ));
    }
    if environment.get("PATH").is_none_or(|path| path.trim().is_empty())
      || environment
        .iter()
        .any(|(name, value)| name.is_empty() || name.contains(['=', '\0']) || value.contains('\0'))
    {
      return Err(ExecutionError::Invalid(
        "native environment requires PATH and valid process environment entries".to_owned(),
      ));
    }
    validate_native_path(&environment, &readonly_paths)?;
    Ok(Self {
      cgroup_root,
      work_root,
      bubblewrap,
      readonly_paths,
      cleanup_timeout,
      pids_limit,
      environment,
    })
  }
}

#[async_trait]
impl ExecutionBackend for NativeBackend {
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
    let host_platform = host_execution_platform()?;
    if request.root
      != (ExecutionTarget::Native {
        platform: host_platform,
      })
    {
      return Err(ExecutionError::Unavailable(format!(
        "native execution requires the host platform {host_platform:?}"
      )));
    }
    if matches!(request.network, NetworkAccess::Restricted { .. }) {
      return Err(ExecutionError::Unavailable(
        "Linux Native cannot enforce a hostname allow-list; use disabled or unrestricted network access".to_owned(),
      ));
    }
    let request_root = request.workspace_root.canonicalize().map_err(ExecutionError::Io)?;
    if request_root != self.work_root {
      return Err(ExecutionError::Invalid(
        "execution workspace_root does not match the Native backend work_root".to_owned(),
      ));
    }
    validate_workspace_filesystem(&self.work_root, &request.workspace, request.writable_disk_bytes)?;
    let temporary_directory = request.data_dir.join("tmp");
    let home_directory = request.data_dir.join("home");
    fs::create_dir(&temporary_directory).map_err(ExecutionError::Io)?;
    fs::create_dir(&home_directory).map_err(ExecutionError::Io)?;

    let seccomp = create_seccomp_filter()?;
    let mut command = Command::new(&self.bubblewrap);
    command
      .arg("--die-with-parent")
      .arg("--new-session")
      .arg("--unshare-all")
      .arg("--unshare-user")
      .args(network_arguments(&request.network))
      .arg("--tmpfs")
      .arg("/")
      .env_clear()
      .current_dir(&request.workspace)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true)
      .process_group(0);
    add_native_filesystem(
      &mut command,
      runner,
      &request,
      &temporary_directory,
      &home_directory,
      &self.readonly_paths,
    )?;
    command
      .arg("--chdir")
      .arg(&request.workspace)
      .arg("--clearenv")
      .args(environment_arguments(&self.environment))
      .arg("--cap-drop")
      .arg("ALL")
      .arg("--disable-userns")
      .arg("--seccomp")
      .arg(seccomp.as_raw_fd().to_string())
      .arg("--")
      .arg(&runner.executable);

    // Create the kernel-owned execution state only after every fallible
    // userspace preparation step has succeeded. From this point onward each
    // error path removes the cgroup before returning.
    let cgroup = self.cgroup_root.join(cgroup_name(&request.execution_id));
    fs::create_dir(&cgroup)
      .map_err(|error| backend(format!("create execution cgroup '{}': {error}", cgroup.display())))?;
    if let Err(error) = configure_cgroup(&cgroup, &request, self.pids_limit) {
      return Err(rollback_start_failure(error, &cgroup, None, self.cleanup_timeout).await);
    }
    if cancellation.is_cancelled() {
      return Err(rollback_start_failure(ExecutionError::Cancelled, &cgroup, None, self.cleanup_timeout).await);
    }
    if let Err(error) = attach_to_cgroup_before_exec(&mut command, &cgroup) {
      return Err(rollback_start_failure(error, &cgroup, None, self.cleanup_timeout).await);
    }
    let mut child = match command.spawn() {
      Ok(child) => child,
      Err(error) => {
        return Err(rollback_start_failure(ExecutionError::Io(error), &cgroup, None, self.cleanup_timeout).await);
      }
    };
    let Some(pid) = child.id() else {
      return Err(
        rollback_start_failure(
          backend("octa-runner has no process identifier"),
          &cgroup,
          Some(&mut child),
          self.cleanup_timeout,
        )
        .await,
      );
    };
    let io = match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
      (Some(stdin), Some(stdout), Some(stderr)) => ExecutionIo {
        stdin: Box::pin(stdin),
        stdout: Box::pin(stdout),
        stderr: Box::pin(stderr),
      },
      _ => {
        return Err(
          rollback_start_failure(ExecutionError::IoTaken, &cgroup, Some(&mut child), self.cleanup_timeout).await,
        );
      }
    };
    let disk_peak_bytes = filesystem_usage(&request.workspace_root)?.used_bytes;
    info!(
      execution_id = %request.execution_id,
      pid,
      cgroup = %cgroup.display(),
      security_profile = SECURITY_PROFILE_VERSION,
      "started sandboxed native octa-runner"
    );
    Ok(Box::new(NativeExecution {
      child,
      io: Some(io),
      paths: ExecutionPaths {
        workspace: request.workspace.clone(),
        data_dir: request.data_dir,
        plugins_dir: runner.plugins_dir.clone(),
        plugin_lock: runner.plugin_lock.clone(),
      },
      cgroup,
      filesystem_root: request.workspace_root,
      cleanup_timeout: self.cleanup_timeout,
      started: Instant::now(),
      disk_peak_bytes,
      reaped: false,
    }))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    let entries = fs::read_dir(&self.cgroup_root).map_err(|error| {
      backend(format!(
        "read native cgroup root '{}': {error}",
        self.cgroup_root.display()
      ))
    })?;
    for entry in entries {
      let entry = entry.map_err(|error| backend(format!("read native cgroup entry: {error}")))?;
      if !entry
        .file_type()
        .map_err(|error| backend(format!("inspect native cgroup entry: {error}")))?
        .is_dir()
      {
        continue;
      }
      let name = entry.file_name();
      // Refuse to touch an unknown directory: the configured root may be
      // wrong, and orphan cleanup must never kill an operator-owned cgroup.
      if !is_cgroup_name(&name) {
        return Err(backend(format!(
          "unexpected directory '{}' in the dedicated native cgroup root",
          entry.path().display()
        )));
      }
      kill_cgroup(&entry.path())?;
      remove_cgroup(&entry.path(), self.cleanup_timeout).await?;
    }
    Ok(())
  }
}

async fn rollback_start_failure(
  operation: ExecutionError,
  cgroup: &Path,
  mut child: Option<&mut Child>,
  cleanup_timeout: Duration,
) -> ExecutionError {
  let mut failures = Vec::new();
  if child.is_some()
    && let Err(error) = kill_cgroup(cgroup)
  {
    if let Some(child) = child.as_deref_mut() {
      kill_process_group(child, libc::SIGKILL);
    }
    failures.push(error.to_string());
  }
  if let Some(child) = child {
    match tokio::time::timeout(cleanup_timeout, child.wait()).await {
      Ok(Ok(_)) => {}
      Ok(Err(error)) => failures.push(format!("reap native runner during startup rollback: {error}")),
      Err(_) => failures.push("native runner did not exit during startup rollback".to_owned()),
    }
  }
  if let Err(error) = remove_cgroup(cgroup, cleanup_timeout).await {
    failures.push(error.to_string());
  }
  if failures.is_empty() {
    operation
  } else {
    operation.with_cleanup(backend(failures.join("; ")))
  }
}

struct NativeExecution {
  child: Child,
  io: Option<ExecutionIo>,
  paths: ExecutionPaths,
  cgroup: PathBuf,
  filesystem_root: PathBuf,
  cleanup_timeout: Duration,
  started: Instant,
  disk_peak_bytes: u64,
  reaped: bool,
}

#[async_trait]
impl RunningExecution for NativeExecution {
  fn paths(&self) -> &ExecutionPaths {
    &self.paths
  }

  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.io.take().ok_or(ExecutionError::IoTaken)
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    let cpu_time_ms = read_cpu_time(&self.cgroup)?;
    let memory_current_bytes = read_number(&self.cgroup.join("memory.current"))?;
    let memory_peak_bytes = read_number(&self.cgroup.join("memory.peak"))?;
    let (io_read_bytes, io_written_bytes) = read_io(&self.cgroup)?;
    let disk = filesystem_usage(&self.filesystem_root)?;
    self.disk_peak_bytes = self.disk_peak_bytes.max(disk.used_bytes);
    Ok(ResourceUsage {
      elapsed_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
      cpu_time_ms,
      memory_current_bytes,
      memory_peak_bytes,
      disk_current_bytes: disk.used_bytes,
      disk_peak_bytes: self.disk_peak_bytes,
      io_read_bytes,
      io_written_bytes,
      network_received_bytes: None,
      network_transmitted_bytes: None,
    })
  }

  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    if self.reaped {
      return Err(ExecutionError::Reaped);
    }
    let status = self.child.wait().await.map_err(ExecutionError::Io)?;
    self.reaped = true;
    Ok(ExecutionExit { code: status.code() })
  }

  async fn kill(&mut self) -> Result<(), ExecutionError> {
    match kill_cgroup(&self.cgroup) {
      Ok(()) => Ok(()),
      Err(error) => {
        kill_process_group(&mut self.child, libc::SIGKILL);
        Err(error)
      }
    }
  }

  async fn destroy(mut self: Box<Self>) -> Result<(), ExecutionError> {
    let mut failures = Vec::new();
    if let Err(error) = kill_cgroup(&self.cgroup) {
      kill_process_group(&mut self.child, libc::SIGKILL);
      failures.push(error.to_string());
    }
    if !self.reaped {
      match tokio::time::timeout(self.cleanup_timeout, self.child.wait()).await {
        Ok(Ok(_)) => self.reaped = true,
        Ok(Err(error)) => failures.push(format!("reap native runner: {error}")),
        Err(_) => {
          kill_process_group(&mut self.child, libc::SIGKILL);
          failures.push("native runner did not exit within the cleanup timeout".to_owned());
        }
      }
    }
    if let Err(error) = remove_cgroup(&self.cgroup, self.cleanup_timeout).await {
      failures.push(error.to_string());
    }
    if !failures.is_empty() {
      return Err(backend(failures.join("; ")));
    }
    debug!(cgroup = %self.cgroup.display(), "destroyed native execution");
    Ok(())
  }
}

impl Drop for NativeExecution {
  fn drop(&mut self) {
    if !self.reaped {
      kill_process_group(&mut self.child, libc::SIGKILL);
    }
    let _ = fs::write(self.cgroup.join("cgroup.kill"), "1");
  }
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
