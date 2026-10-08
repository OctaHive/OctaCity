//! Direct runner execution on the Agent host operating system.
//!
//! This backend deliberately provides no filesystem, process, network, or
//! resource-isolation claim. It still owns one private workspace, starts the
//! runner in a kill-on-drop process group, uses an explicit clean environment,
//! samples resource use, and removes no path outside the Agent-owned work root.

#![warn(missing_docs)]

use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  path::{Path, PathBuf},
  process::Stdio,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::{Duration, Instant},
};

use async_trait::async_trait;
use octacity_execution::{
  ExecutionArchitecture, ExecutionBackend, ExecutionError, ExecutionExit, ExecutionIo, ExecutionOs, ExecutionPaths,
  ExecutionPlatform, ExecutionTarget, NetworkAccess, ResourceUsage, RunnerProgram, RunningExecution, StartExecution,
};
use processkit::ProcessGroup;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::{
  process::{Child, Command},
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

/// Stable inventory and diagnostic identity for direct host execution.
pub const HOST_BACKEND_NAME: &str = "host";

/// Operator-owned inputs for direct host execution.
#[derive(Clone, Debug)]
pub struct HostBackendConfig {
  /// Dedicated Agent-owned root containing ephemeral job workspaces.
  pub work_root: PathBuf,
  /// Platform declared by the verified installed runner bundle.
  pub runner_platform: String,
  /// Complete clean environment inherited by the runner and its tasks.
  pub environment: BTreeMap<String, String>,
  /// Maximum wait while reaping a force-stopped runner.
  pub cleanup_timeout: Duration,
  /// Maximum filesystem entries visited by one workspace accounting scan.
  pub max_accounted_workspace_entries: usize,
}

/// Cross-platform backend that invokes the verified runner as a host process.
pub struct HostBackend {
  work_root: PathBuf,
  platform: ExecutionPlatform,
  environment: BTreeMap<String, String>,
  cleanup_timeout: Duration,
  max_accounted_workspace_entries: usize,
}

impl HostBackend {
  /// Validates the host identity, runner platform, work root, and clean environment.
  pub fn new(config: HostBackendConfig) -> Result<Self, ExecutionError> {
    let platform = host_platform()?;
    let expected_runner = runner_platform(platform);
    if config.runner_platform != expected_runner {
      return Err(ExecutionError::Invalid(format!(
        "host execution requires runner platform '{expected_runner}', got '{}'",
        config.runner_platform
      )));
    }
    if config.cleanup_timeout.is_zero() {
      return Err(ExecutionError::Invalid(
        "host cleanup timeout must be greater than zero".to_owned(),
      ));
    }
    if config.max_accounted_workspace_entries == 0 {
      return Err(ExecutionError::Invalid(
        "host accounting entry limit must be greater than zero".to_owned(),
      ));
    }
    validate_environment(&config.environment)?;
    let work_root = config
      .work_root
      .canonicalize()
      .map_err(|error| unavailable(format!("host work root '{}': {error}", config.work_root.display())))?;
    if !work_root.is_dir() {
      return Err(unavailable("host work_root must be a directory"));
    }
    Ok(Self {
      work_root,
      platform,
      environment: config.environment,
      cleanup_timeout: config.cleanup_timeout,
      max_accounted_workspace_entries: config.max_accounted_workspace_entries,
    })
  }

  /// Exact platform on which this backend can execute.
  #[must_use]
  pub const fn platform(&self) -> ExecutionPlatform {
    self.platform
  }
}

#[async_trait]
impl ExecutionBackend for HostBackend {
  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    if cancellation.is_cancelled() {
      return Err(ExecutionError::Cancelled);
    }
    request.validate()?;
    if request.workload_identity.is_some() {
      return Err(ExecutionError::Unavailable(
        "direct host execution cannot safely project workload identity".to_owned(),
      ));
    }
    if request.root
      != (ExecutionTarget::Host {
        platform: self.platform,
      })
    {
      return Err(ExecutionError::Unavailable(format!(
        "direct host execution requires identical host and target platform {:?}",
        self.platform
      )));
    }
    if !matches!(request.network, NetworkAccess::Unrestricted) {
      return Err(ExecutionError::Unavailable(
        "direct host execution cannot enforce disabled or restricted network access".to_owned(),
      ));
    }
    let request_root = request.workspace_root.canonicalize().map_err(ExecutionError::Io)?;
    if request_root != self.work_root {
      return Err(ExecutionError::Invalid(
        "execution workspace_root does not match the Host backend work_root".to_owned(),
      ));
    }
    let workspace = canonical_job_workspace(&request_root, &request.workspace)?;
    let data_dir = request.data_dir.canonicalize().map_err(ExecutionError::Io)?;
    if !data_dir.starts_with(&workspace) {
      return Err(ExecutionError::Invalid(
        "host data_dir must resolve inside the job workspace".to_owned(),
      ));
    }
    runner.validate()?;

    let mut command = Command::new(&runner.executable);
    command
      .env_clear()
      .envs(&self.environment)
      .envs(&runner.external_executables)
      .current_dir(&workspace)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);
    let group = ProcessGroup::new().map_err(|error| backend(format!("create host process group: {error}")))?;
    let mut child = group
      .spawn(command)
      .map_err(|error| backend(format!("start host runner: {error}")))?;
    let pid = child
      .id()
      .ok_or_else(|| backend("host runner has no process identifier"))?;
    let io = match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
      (Some(stdin), Some(stdout), Some(stderr)) => ExecutionIo {
        stdin: Box::pin(stdin),
        stdout: Box::pin(stdout),
        stderr: Box::pin(stderr),
      },
      _ => {
        let _ = group.kill_all();
        let _ = child.wait().await;
        return Err(ExecutionError::IoTaken);
      }
    };
    let disk_usage = WorkspaceDiskUsage::new(workspace.clone(), self.max_accounted_workspace_entries);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let root_started_at = system.process(Pid::from_u32(pid)).map(sysinfo::Process::start_time);
    let initial_process = sample_process_tree(&system, Pid::from_u32(pid), root_started_at);
    info!(execution_id = %request.execution_id, pid, "started direct host octa-runner");
    Ok(Box::new(HostExecution {
      child,
      group,
      io: Some(io),
      paths: ExecutionPaths {
        workspace: workspace.clone(),
        data_dir,
        plugins_dir: runner.plugins_dir.clone(),
        plugin_lock: runner.plugin_lock.clone(),
        cache: request
          .cache
          .as_ref()
          .map(octacity_execution::ExecutionCacheMounts::host_paths),
      },
      workspace,
      disk_usage,
      cleanup_timeout: self.cleanup_timeout,
      root_pid: Pid::from_u32(pid),
      root_started_at,
      system,
      started: Instant::now(),
      peak_memory_bytes: initial_process.memory_bytes,
      cpu_time_ms: initial_process.cpu_time_ms,
      io_read_bytes: initial_process.io_read_bytes,
      io_written_bytes: initial_process.io_written_bytes,
      reaped: false,
    }))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    // There is no safe cross-platform way to rediscover ownership after an
    // unclean Agent exit. Live executions are killed through their owned
    // process-group handle; JobExecutor separately removes recognized stale
    // workspaces on the next startup.
    Ok(())
  }
}

struct HostExecution {
  child: Child,
  group: ProcessGroup,
  io: Option<ExecutionIo>,
  paths: ExecutionPaths,
  workspace: PathBuf,
  disk_usage: WorkspaceDiskUsage,
  cleanup_timeout: Duration,
  root_pid: Pid,
  root_started_at: Option<u64>,
  system: System,
  started: Instant,
  peak_memory_bytes: u64,
  cpu_time_ms: u64,
  io_read_bytes: u64,
  io_written_bytes: u64,
  reaped: bool,
}

#[async_trait]
impl RunningExecution for HostExecution {
  fn paths(&self) -> &ExecutionPaths {
    &self.paths
  }

  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.io.take().ok_or(ExecutionError::IoTaken)
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    self.system.refresh_processes(ProcessesToUpdate::All, true);
    let process = sample_process_tree(&self.system, self.root_pid, self.root_started_at);
    self.cpu_time_ms = self.cpu_time_ms.max(process.cpu_time_ms);
    self.peak_memory_bytes = self.peak_memory_bytes.max(process.memory_bytes);
    self.io_read_bytes = self.io_read_bytes.max(process.io_read_bytes);
    self.io_written_bytes = self.io_written_bytes.max(process.io_written_bytes);
    let (disk_current_bytes, disk_peak_bytes) = self.disk_usage.sample().await?;
    Ok(ResourceUsage {
      elapsed_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
      cpu_time_ms: self.cpu_time_ms,
      memory_current_bytes: process.memory_bytes,
      memory_peak_bytes: self.peak_memory_bytes,
      disk_current_bytes,
      disk_peak_bytes,
      io_read_bytes: self.io_read_bytes,
      io_written_bytes: self.io_written_bytes,
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
    self.disk_usage.cancel();
    self
      .group
      .kill_all()
      .map_err(|error| backend(format!("kill host process group: {error}")))?;
    if !self.reaped {
      let _ = self.child.start_kill();
    }
    Ok(())
  }

  async fn destroy(mut self: Box<Self>) -> Result<(), ExecutionError> {
    self.disk_usage.cancel();
    let mut failures = Vec::new();
    if let Err(error) = self.group.kill_all() {
      failures.push(format!("kill host process group: {error}"));
      let _ = self.child.start_kill();
    }
    if !self.reaped {
      match tokio::time::timeout(self.cleanup_timeout, self.child.wait()).await {
        Ok(Ok(_)) => self.reaped = true,
        Ok(Err(error)) => failures.push(format!("reap host runner: {error}")),
        Err(_) => failures.push("host runner did not exit within the cleanup timeout".to_owned()),
      }
    }
    if failures.is_empty() {
      debug!(workspace = %self.workspace.display(), "destroyed direct host execution");
      Ok(())
    } else {
      Err(backend(failures.join("; ")))
    }
  }
}

struct WorkspaceDiskUsage {
  root: PathBuf,
  max_entries: usize,
  cancellation: Arc<AtomicBool>,
  scan: Option<JoinHandle<Result<u64, ExecutionError>>>,
  current_bytes: u64,
  peak_bytes: u64,
}

impl WorkspaceDiskUsage {
  fn new(root: PathBuf, max_entries: usize) -> Self {
    let mut usage = Self {
      root,
      max_entries,
      cancellation: Arc::new(AtomicBool::new(false)),
      scan: None,
      current_bytes: 0,
      peak_bytes: 0,
    };
    usage.start_scan();
    usage
  }

  async fn sample(&mut self) -> Result<(u64, u64), ExecutionError> {
    if self.scan.is_none() {
      self.start_scan();
    }
    let Some(scan) = self.scan.as_mut() else {
      return Err(backend("workspace accounting task was not initialized"));
    };
    let result = scan.await;
    self.scan = None;
    let result = result.map_err(|error| backend(format!("workspace accounting task failed: {error}")))?;
    self.current_bytes = result?;
    self.peak_bytes = self.peak_bytes.max(self.current_bytes);
    self.start_scan();
    Ok((self.current_bytes, self.peak_bytes))
  }

  fn start_scan(&mut self) {
    let root = self.root.clone();
    let max_entries = self.max_entries;
    let cancellation = self.cancellation.clone();
    self.scan = Some(tokio::task::spawn_blocking(move || {
      directory_size_sync(&root, max_entries, &cancellation)
    }));
  }

  fn cancel(&mut self) {
    self.cancellation.store(true, Ordering::Release);
    if let Some(scan) = &self.scan {
      scan.abort();
    }
  }
}

impl Drop for WorkspaceDiskUsage {
  fn drop(&mut self) {
    self.cancel();
  }
}

impl Drop for HostExecution {
  fn drop(&mut self) {
    let _ = self.group.kill_all();
    if !self.reaped {
      let _ = self.child.start_kill();
    }
  }
}

#[derive(Default)]
struct ProcessSample {
  cpu_time_ms: u64,
  memory_bytes: u64,
  io_read_bytes: u64,
  io_written_bytes: u64,
}

fn sample_process_tree(system: &System, root: Pid, root_started_at: Option<u64>) -> ProcessSample {
  let members = system
    .processes()
    .keys()
    .copied()
    .filter(|pid| belongs_to_tree(system, *pid, root, root_started_at))
    .collect::<BTreeSet<_>>();
  members
    .into_iter()
    .filter_map(|pid| system.process(pid))
    .fold(ProcessSample::default(), |mut sample, process| {
      let disk = process.disk_usage();
      sample.cpu_time_ms = sample.cpu_time_ms.saturating_add(process.accumulated_cpu_time());
      sample.memory_bytes = sample.memory_bytes.saturating_add(process.memory());
      sample.io_read_bytes = sample.io_read_bytes.saturating_add(disk.total_read_bytes);
      sample.io_written_bytes = sample.io_written_bytes.saturating_add(disk.total_written_bytes);
      sample
    })
}

fn belongs_to_tree(system: &System, candidate: Pid, root: Pid, root_started_at: Option<u64>) -> bool {
  let mut current = Some(candidate);
  let mut visited = BTreeSet::new();
  while let Some(pid) = current {
    if !visited.insert(pid) {
      return false;
    }
    if pid == root {
      return root_started_at.is_none_or(|started| {
        system
          .process(pid)
          .is_some_and(|process| process.start_time() == started)
      });
    }
    current = system.process(pid).and_then(sysinfo::Process::parent);
  }
  false
}

fn directory_size_sync(root: &Path, max_entries: usize, cancellation: &AtomicBool) -> Result<u64, ExecutionError> {
  let mut total = 0_u64;
  let mut entries = 0_usize;
  let mut pending = vec![root.to_owned()];
  while let Some(directory) = pending.pop() {
    if cancellation.load(Ordering::Acquire) {
      return Err(ExecutionError::Cancelled);
    }
    for entry in fs::read_dir(&directory).map_err(ExecutionError::Io)? {
      if cancellation.load(Ordering::Acquire) {
        return Err(ExecutionError::Cancelled);
      }
      let entry = entry.map_err(ExecutionError::Io)?;
      entries = entries
        .checked_add(1)
        .ok_or_else(|| backend("workspace entry count overflowed"))?;
      if entries > max_entries {
        return Err(backend(format!("workspace contains more than {max_entries} entries")));
      }
      let metadata = fs::symlink_metadata(entry.path()).map_err(ExecutionError::Io)?;
      if metadata.file_type().is_dir() {
        pending.push(entry.path());
      } else {
        total = total
          .checked_add(metadata.len())
          .ok_or_else(|| backend("workspace byte accounting overflowed"))?;
      }
    }
  }
  Ok(total)
}

fn canonical_job_workspace(work_root: &Path, workspace: &Path) -> Result<PathBuf, ExecutionError> {
  let workspace = workspace.canonicalize().map_err(ExecutionError::Io)?;
  let job_root = workspace
    .parent()
    .filter(|parent| parent.parent() == Some(work_root))
    .ok_or_else(|| ExecutionError::Invalid("host workspace must be inside one job-private root".to_owned()))?;
  if workspace.file_name() != Some(std::ffi::OsStr::new("workspace")) || !job_root.starts_with(work_root) {
    return Err(ExecutionError::Invalid(
      "host job-private workspace must be named 'workspace'".to_owned(),
    ));
  }
  Ok(workspace)
}

fn validate_environment(environment: &BTreeMap<String, String>) -> Result<(), ExecutionError> {
  if environment.get("PATH").is_none_or(|path| path.trim().is_empty())
    || environment
      .iter()
      .any(|(name, value)| name.is_empty() || name.contains(['=', '\0']) || value.contains('\0'))
  {
    return Err(ExecutionError::Invalid(
      "host environment requires PATH and valid process environment entries".to_owned(),
    ));
  }
  Ok(())
}

fn host_platform() -> Result<ExecutionPlatform, ExecutionError> {
  let os = match std::env::consts::OS {
    "linux" => ExecutionOs::Linux,
    "macos" => ExecutionOs::Macos,
    "windows" => ExecutionOs::Windows,
    other => return Err(unavailable(format!("unsupported host operating system '{other}'"))),
  };
  let architecture = match std::env::consts::ARCH {
    "x86_64" => ExecutionArchitecture::Amd64,
    "aarch64" => ExecutionArchitecture::Arm64,
    other => return Err(unavailable(format!("unsupported host architecture '{other}'"))),
  };
  Ok(ExecutionPlatform { os, architecture })
}

fn runner_platform(platform: ExecutionPlatform) -> String {
  let os = match platform.os {
    ExecutionOs::Linux => "linux",
    ExecutionOs::Windows => "windows",
    ExecutionOs::Macos => "macos",
  };
  let architecture = match platform.architecture {
    ExecutionArchitecture::Amd64 => "x86_64",
    ExecutionArchitecture::Arm64 => "aarch64",
  };
  format!("{os}-{architecture}")
}

fn unavailable(message: impl Into<String>) -> ExecutionError {
  ExecutionError::Unavailable(message.into())
}

fn backend(message: impl Into<String>) -> ExecutionError {
  ExecutionError::Backend(message.into())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn validates_explicit_host_configuration() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = HostBackend::new(HostBackendConfig {
      work_root: temporary.path().to_owned(),
      runner_platform: runner_platform(host_platform().unwrap()),
      environment: BTreeMap::from([(
        "PATH".to_owned(),
        std::env::var("PATH").unwrap_or_else(|_| ".".to_owned()),
      )]),
      cleanup_timeout: Duration::from_secs(1),
      max_accounted_workspace_entries: 100,
    })
    .unwrap();
    assert_eq!(backend.platform(), host_platform().unwrap());
  }

  #[test]
  fn rejects_implicit_environment_and_wrong_runner_platform() {
    let temporary = tempfile::tempdir().unwrap();
    let invalid_environment = HostBackend::new(HostBackendConfig {
      work_root: temporary.path().to_owned(),
      runner_platform: runner_platform(host_platform().unwrap()),
      environment: BTreeMap::new(),
      cleanup_timeout: Duration::from_secs(1),
      max_accounted_workspace_entries: 100,
    });
    assert!(matches!(invalid_environment, Err(ExecutionError::Invalid(_))));

    let wrong_platform = HostBackend::new(HostBackendConfig {
      work_root: temporary.path().to_owned(),
      runner_platform: "unsupported-runner".to_owned(),
      environment: BTreeMap::from([("PATH".to_owned(), ".".to_owned())]),
      cleanup_timeout: Duration::from_secs(1),
      max_accounted_workspace_entries: 100,
    });
    assert!(matches!(wrong_platform, Err(ExecutionError::Invalid(_))));
  }

  #[test]
  fn workspace_accounting_does_not_follow_symlinks() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    fs::write(workspace.join("data"), b"1234").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(temporary.path(), workspace.join("loop")).unwrap();

    let size = directory_size_sync(&workspace, 100, &AtomicBool::new(false)).unwrap();
    assert!(size >= 4);
    assert!(size < 1024);
  }

  #[tokio::test]
  async fn rejects_workload_identity_before_starting_a_host_process() {
    let temporary = tempfile::tempdir().unwrap();
    let job_root = temporary
      .path()
      .join("job-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
    let workspace = job_root.join("workspace");
    let data_dir = workspace.join(".octacity");
    fs::create_dir_all(&data_dir).unwrap();
    let identity = job_root.join("identity-token");
    fs::write(&identity, "secret").unwrap();
    let identity = identity.canonicalize().unwrap();
    let backend = HostBackend::new(HostBackendConfig {
      work_root: temporary.path().to_owned(),
      runner_platform: runner_platform(host_platform().unwrap()),
      environment: BTreeMap::from([("PATH".to_owned(), ".".to_owned())]),
      cleanup_timeout: Duration::from_secs(1),
      max_accounted_workspace_entries: 100,
    })
    .unwrap();
    let request = StartExecution {
      execution_id: "fixture".to_owned(),
      workspace_root: temporary.path().to_owned(),
      workspace,
      data_dir,
      workload_identity: Some(identity),
      cache: None,
      factory: None,
      cpu_millis: 1,
      memory_bytes: 1,
      writable_disk_bytes: 1,
      max_duration: Duration::from_secs(1),
      root: ExecutionTarget::Host {
        platform: host_platform().unwrap(),
      },
      network: NetworkAccess::Unrestricted,
    };
    let invalid_runner = RunnerProgram {
      release_root: PathBuf::new(),
      executable: PathBuf::new(),
      plugins_dir: PathBuf::new(),
      plugin_lock: PathBuf::new(),
      external_executables: BTreeMap::new(),
    };

    let error = match backend.start(&invalid_runner, request, CancellationToken::new()).await {
      Ok(_) => panic!("Host execution unexpectedly accepted workload identity"),
      Err(error) => error,
    };

    assert!(matches!(error, ExecutionError::Unavailable(message) if message.contains("workload identity")));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn destroying_an_execution_kills_its_descendant_processes() {
    use std::os::unix::fs::PermissionsExt as _;

    let temporary = tempfile::tempdir().unwrap();
    let work_root = temporary.path().join("work");
    let release_root = temporary.path().join("release");
    let plugins_dir = release_root.join("plugins");
    fs::create_dir_all(&work_root).unwrap();
    fs::create_dir_all(&plugins_dir).unwrap();
    let marker = temporary.path().join("descendant-marker");
    let executable = release_root.join("octa-runner");
    fs::write(
      &executable,
      format!(
        "#!/bin/sh\n(sleep 1; printf leaked > '{}') &\nsleep 60\n",
        marker.display()
      ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let plugin_lock = release_root.join("Octa.lock");
    fs::write(&plugin_lock, "fixture").unwrap();
    let job_root = work_root.join("job-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
    let workspace = job_root.join("workspace");
    let data_dir = workspace.join(".octacity");
    fs::create_dir_all(&data_dir).unwrap();
    let backend = HostBackend::new(HostBackendConfig {
      work_root: work_root.clone(),
      runner_platform: runner_platform(host_platform().unwrap()),
      environment: BTreeMap::from([("PATH".to_owned(), "/usr/bin:/bin".to_owned())]),
      cleanup_timeout: Duration::from_secs(2),
      max_accounted_workspace_entries: 100,
    })
    .unwrap();
    let runner = RunnerProgram {
      release_root,
      executable,
      plugins_dir,
      plugin_lock,
      external_executables: BTreeMap::new(),
    };
    let request = StartExecution {
      execution_id: "descendant-cleanup".to_owned(),
      workspace_root: work_root,
      workspace,
      data_dir,
      workload_identity: None,
      cache: None,
      factory: None,
      cpu_millis: 1,
      memory_bytes: 1,
      writable_disk_bytes: 1,
      max_duration: Duration::from_secs(60),
      root: ExecutionTarget::Host {
        platform: host_platform().unwrap(),
      },
      network: NetworkAccess::Unrestricted,
    };

    let execution = backend.start(&runner, request, CancellationToken::new()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    execution.destroy().await.unwrap();
    tokio::time::sleep(Duration::from_millis(1_100)).await;

    assert!(!marker.exists(), "a descendant survived Host execution cleanup");
  }
}
