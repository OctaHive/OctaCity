//! Native Linux execution through a dedicated cgroup v2 and workspace mount.
//!
//! Native execution is an explicit operator choice, not a sandbox. It still
//! enforces CPU and memory through cgroup v2 and requires workspace storage on
//! a dedicated quota-sized mount so disk limits cannot silently be advisory.

use std::path::Path;

#[cfg(target_os = "linux")]
use std::path::PathBuf;

use async_trait::async_trait;

use octacity_execution::{ExecutionBackend, ExecutionError, RunnerProgram, RunningExecution, StartExecution};

#[cfg(target_os = "linux")]
mod linux {
  use std::{
    fs,
    os::unix::fs::MetadataExt as _,
    process::Stdio,
    time::{Duration, Instant},
  };

  use sha2::{Digest as _, Sha256};
  use tokio::{
    process::{Child, Command},
    time::sleep,
  };
  use tracing::{debug, info};

  use super::*;
  use octacity_execution::{ExecutionExit, ExecutionIo, ExecutionPaths, ResourceUsage};

  const CPU_PERIOD_MICROS: u64 = 100_000;
  const DEFAULT_PIDS_LIMIT: u32 = 4096;
  const CGROUP_CLEANUP_ATTEMPTS: usize = 100;
  const CGROUP_CLEANUP_INTERVAL: Duration = Duration::from_millis(50);

  /// Linux backend that confines the verified runner to a dedicated cgroup
  /// and a quota-sized workspace filesystem.
  pub struct NativeBackend {
    cgroup_root: PathBuf,
  }

  impl NativeBackend {
    /// Opens an operator-delegated cgroup-v2 root used exclusively by OctaCity.
    pub fn new(cgroup_root: &Path) -> Result<Self, ExecutionError> {
      let cgroup_root = cgroup_root
        .canonicalize()
        .map_err(|error| unavailable(format!("native cgroup root '{}': {error}", cgroup_root.display())))?;
      if !cgroup_root.is_dir() || !cgroup_root.join("cgroup.controllers").is_file() {
        return Err(unavailable(format!(
          "native cgroup root '{}' is not a delegated cgroup v2 directory",
          cgroup_root.display()
        )));
      }
      Ok(Self { cgroup_root })
    }
  }

  #[async_trait]
  impl ExecutionBackend for NativeBackend {
    async fn start(
      &self,
      runner: &RunnerProgram,
      request: StartExecution,
    ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
      request.validate()?;
      validate_workspace_mount(&request.workspace, request.writable_disk_bytes)?;

      let cgroup = self.cgroup_root.join(cgroup_name(&request.execution_id));
      fs::create_dir(&cgroup)
        .map_err(|error| backend(format!("create execution cgroup '{}': {error}", cgroup.display())))?;
      if let Err(error) = configure_cgroup(&cgroup, &request) {
        let _ = fs::remove_dir(&cgroup);
        return Err(error);
      }

      let mut command = Command::new(&runner.executable);
      command
        .env_clear()
        .current_dir(&request.workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
      let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
          let _ = fs::remove_dir(&cgroup);
          return Err(ExecutionError::Io(error));
        }
      };
      let pid = child
        .id()
        .ok_or_else(|| backend("octa-runner has no process identifier"))?;
      if let Err(error) = fs::write(cgroup.join("cgroup.procs"), pid.to_string()) {
        kill_process_group(&mut child, libc::SIGKILL);
        let _ = child.wait().await;
        let _ = fs::remove_dir(&cgroup);
        return Err(backend(format!("attach octa-runner to its cgroup: {error}")));
      }
      let io = match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
        (Some(stdin), Some(stdout), Some(stderr)) => ExecutionIo {
          stdin: Box::pin(stdin),
          stdout: Box::pin(stdout),
          stderr: Box::pin(stderr),
        },
        _ => {
          let _ = kill_cgroup(&cgroup);
          let _ = child.wait().await;
          let _ = remove_cgroup(&cgroup).await;
          return Err(ExecutionError::IoTaken);
        }
      };
      let disk_peak_bytes = filesystem_usage(&request.workspace)?.used_bytes;
      info!(
        execution_id = %request.execution_id,
        pid,
        cgroup = %cgroup.display(),
        "started native octa-runner"
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
        workspace: request.workspace,
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
        if !name.to_string_lossy().starts_with("execution-") {
          return Err(backend(format!(
            "unexpected directory '{}' in the dedicated native cgroup root",
            entry.path().display()
          )));
        }
        kill_cgroup(&entry.path())?;
        remove_cgroup(&entry.path()).await?;
      }
      Ok(())
    }
  }

  struct NativeExecution {
    child: Child,
    io: Option<ExecutionIo>,
    paths: ExecutionPaths,
    cgroup: PathBuf,
    workspace: PathBuf,
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
      let disk = filesystem_usage(&self.workspace)?;
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
      kill_cgroup(&self.cgroup)
    }

    async fn destroy(mut self: Box<Self>) -> Result<(), ExecutionError> {
      kill_cgroup(&self.cgroup)?;
      if !self.reaped {
        let _ = self.child.wait().await;
        self.reaped = true;
      }
      remove_cgroup(&self.cgroup).await?;
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

  fn configure_cgroup(cgroup: &Path, request: &StartExecution) -> Result<(), ExecutionError> {
    let quota = u64::from(request.cpu_millis)
      .checked_mul(CPU_PERIOD_MICROS)
      .and_then(|value| value.checked_div(1000))
      .ok_or_else(|| backend("CPU limit overflow"))?;
    write_control(cgroup, "cpu.max", format!("{quota} {CPU_PERIOD_MICROS}"))?;
    write_control(cgroup, "memory.max", request.memory_bytes.to_string())?;
    write_control(cgroup, "memory.swap.max", "0")?;
    write_control(cgroup, "pids.max", DEFAULT_PIDS_LIMIT.to_string())?;
    for required in ["cpu.stat", "memory.current", "memory.peak", "io.stat", "cgroup.kill"] {
      if !cgroup.join(required).exists() {
        return Err(unavailable(format!(
          "cgroup v2 does not expose required controller file '{required}'"
        )));
      }
    }
    Ok(())
  }

  fn write_control(cgroup: &Path, name: &str, value: impl AsRef<[u8]>) -> Result<(), ExecutionError> {
    fs::write(cgroup.join(name), value)
      .map_err(|error| backend(format!("configure cgroup controller '{name}': {error}")))
  }

  fn read_cpu_time(cgroup: &Path) -> Result<u64, ExecutionError> {
    let contents = read_control(cgroup.join("cpu.stat"))?;
    let usage = contents
      .lines()
      .find_map(|line| line.strip_prefix("usage_usec "))
      .ok_or_else(|| backend("cpu.stat does not contain usage_usec"))?;
    parse_number("cpu.stat usage_usec", usage).map(|value| value / 1000)
  }

  fn read_io(cgroup: &Path) -> Result<(u64, u64), ExecutionError> {
    let contents = read_control(cgroup.join("io.stat"))?;
    let mut read_bytes = 0_u64;
    let mut written_bytes = 0_u64;
    for field in contents.lines().flat_map(str::split_whitespace) {
      if let Some(value) = field.strip_prefix("rbytes=") {
        read_bytes = read_bytes
          .checked_add(parse_number("io.stat rbytes", value)?)
          .ok_or_else(|| backend("io.stat read byte counter overflow"))?;
      } else if let Some(value) = field.strip_prefix("wbytes=") {
        written_bytes = written_bytes
          .checked_add(parse_number("io.stat wbytes", value)?)
          .ok_or_else(|| backend("io.stat written byte counter overflow"))?;
      }
    }
    Ok((read_bytes, written_bytes))
  }

  fn read_number(path: &Path) -> Result<u64, ExecutionError> {
    let name = path
      .file_name()
      .and_then(|name| name.to_str())
      .unwrap_or("cgroup counter");
    parse_number(name, read_control(path)?.trim())
  }

  fn read_control(path: impl AsRef<Path>) -> Result<String, ExecutionError> {
    let path = path.as_ref();
    fs::read_to_string(path).map_err(|error| backend(format!("read cgroup controller '{}': {error}", path.display())))
  }

  fn parse_number(name: &str, value: &str) -> Result<u64, ExecutionError> {
    value
      .parse()
      .map_err(|_| backend(format!("{name} contains an invalid counter")))
  }

  fn kill_cgroup(cgroup: &Path) -> Result<(), ExecutionError> {
    match fs::write(cgroup.join("cgroup.kill"), "1") {
      Ok(()) => Ok(()),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
      Err(error) => Err(backend(format!(
        "kill execution cgroup '{}': {error}",
        cgroup.display()
      ))),
    }
  }

  async fn remove_cgroup(cgroup: &Path) -> Result<(), ExecutionError> {
    for _ in 0..CGROUP_CLEANUP_ATTEMPTS {
      match fs::remove_dir(cgroup) {
        Ok(()) => return Ok(()),
        Err(error) if error.raw_os_error() == Some(libc::EBUSY) || error.raw_os_error() == Some(libc::ENOTEMPTY) => {
          sleep(CGROUP_CLEANUP_INTERVAL).await;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
          return Err(backend(format!(
            "remove execution cgroup '{}': {error}",
            cgroup.display()
          )));
        }
      }
    }
    Err(backend(format!(
      "execution cgroup '{}' remained populated after forced termination",
      cgroup.display()
    )))
  }

  fn cgroup_name(execution_id: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(execution_id.as_bytes()));
    format!("execution-{}", &digest[..32])
  }

  fn kill_process_group(child: &mut Child, signal: i32) {
    if let Some(id) = child.id().and_then(|id| i32::try_from(id).ok()) {
      // SAFETY: the runner is the leader of a process group created at spawn.
      unsafe {
        libc::kill(-id, signal);
      }
    }
    if signal == libc::SIGKILL {
      let _ = child.start_kill();
    }
  }

  struct FilesystemUsage {
    capacity_bytes: u64,
    used_bytes: u64,
  }

  fn validate_workspace_mount(workspace: &Path, limit: u64) -> Result<(), ExecutionError> {
    let workspace_metadata = fs::metadata(workspace).map_err(ExecutionError::Io)?;
    let parent = workspace
      .parent()
      .ok_or_else(|| unavailable("native workspace has no parent directory"))?;
    let parent_metadata = fs::metadata(parent).map_err(ExecutionError::Io)?;
    if workspace_metadata.dev() == parent_metadata.dev() {
      return Err(unavailable(
        "native workspace must be a dedicated filesystem mount so its disk limit is enforceable",
      ));
    }
    let usage = filesystem_usage(workspace)?;
    if usage.capacity_bytes > limit {
      return Err(unavailable(format!(
        "native workspace capacity {} exceeds signed writable disk limit {limit}",
        usage.capacity_bytes
      )));
    }
    Ok(())
  }

  fn filesystem_usage(path: &Path) -> Result<FilesystemUsage, ExecutionError> {
    use std::{ffi::CString, mem::MaybeUninit, os::unix::ffi::OsStrExt as _};

    let path = CString::new(path.as_os_str().as_bytes())
      .map_err(|_| ExecutionError::Invalid("workspace path contains a NUL byte".to_owned()))?;
    let mut stats = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `stats` points to writable memory.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
      return Err(ExecutionError::Io(std::io::Error::last_os_error()));
    }
    // SAFETY: statvfs initialized `stats` after returning success.
    let stats = unsafe { stats.assume_init() };
    let block_size = stats.f_frsize;
    let capacity_bytes = stats.f_blocks.saturating_mul(block_size);
    let free_bytes = stats.f_bfree.saturating_mul(block_size);
    Ok(FilesystemUsage {
      capacity_bytes,
      used_bytes: capacity_bytes.saturating_sub(free_bytes),
    })
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
    fn creates_safe_stable_cgroup_names() {
      assert_eq!(cgroup_name("job-1"), cgroup_name("job-1"));
      assert!(cgroup_name("../../escape").starts_with("execution-"));
      assert!(!cgroup_name("../../escape").contains('/'));
    }

    #[test]
    fn parses_cgroup_io_counters() {
      let directory = tempfile::tempdir().unwrap();
      fs::write(
        directory.path().join("io.stat"),
        "8:0 rbytes=10 wbytes=20 rios=1 wios=2\n8:1 rbytes=3 wbytes=4\n",
      )
      .unwrap();
      assert_eq!(read_io(directory.path()).unwrap(), (13, 24));
    }
  }
}

#[cfg(target_os = "linux")]
pub use linux::NativeBackend;

#[cfg(not(target_os = "linux"))]
pub struct NativeBackend;

#[cfg(not(target_os = "linux"))]
impl NativeBackend {
  /// Reports Native execution as unavailable on non-Linux platforms.
  pub fn new(_cgroup_root: &Path) -> Result<Self, ExecutionError> {
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
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    Err(ExecutionError::Unavailable(
      "NativeBackend v1 is available only on Linux".to_owned(),
    ))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    Ok(())
  }
}
