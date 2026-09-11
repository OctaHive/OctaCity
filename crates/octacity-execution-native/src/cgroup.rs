//! Configures, accounts for, and destroys one delegated cgroup-v2 job.

use super::*;

/// Programs the complete cgroup-v2 resource boundary before the runner starts.
pub(super) fn configure_cgroup(cgroup: &Path, request: &StartExecution, pids_limit: u32) -> Result<(), ExecutionError> {
  let quota = u64::from(request.cpu_millis)
    .checked_mul(CPU_PERIOD_MICROS)
    .and_then(|value| value.checked_div(1000))
    .ok_or_else(|| backend("CPU limit overflow"))?;
  write_control(cgroup, "cpu.max", format!("{quota} {CPU_PERIOD_MICROS}"))?;
  write_control(cgroup, "memory.max", request.memory_bytes.to_string())?;
  write_control(cgroup, "memory.swap.max", "0")?;
  write_control(cgroup, "pids.max", pids_limit.to_string())?;
  for required in ["cpu.stat", "memory.current", "memory.peak", "io.stat", "cgroup.kill"] {
    if !cgroup.join(required).exists() {
      return Err(unavailable(format!(
        "cgroup v2 does not expose required controller file '{required}'"
      )));
    }
  }
  Ok(())
}

/// Verifies that the operator delegated every controller required by Native v1.
pub(super) fn validate_cgroup_root(root: &Path) -> Result<(), ExecutionError> {
  if !root.is_dir() {
    return Err(unavailable(format!(
      "native cgroup root '{}' is not a directory",
      root.display()
    )));
  }
  for file in ["cgroup.controllers", "cgroup.subtree_control"] {
    if !root.join(file).is_file() {
      return Err(unavailable(format!(
        "native cgroup root '{}' is not a delegated cgroup v2 directory",
        root.display()
      )));
    }
  }
  let available = read_control(root.join("cgroup.controllers"))?;
  let enabled = read_control(root.join("cgroup.subtree_control"))?;
  for controller in ["cpu", "memory", "io", "pids"] {
    if !available.split_whitespace().any(|value| value == controller)
      || !enabled.split_whitespace().any(|value| value == controller)
    {
      return Err(unavailable(format!(
        "native cgroup root '{}' must delegate and enable the {controller} controller",
        root.display()
      )));
    }
  }
  Ok(())
}

/// Places the child in its job cgroup in the fork-to-exec window.
///
/// Doing this in `pre_exec` prevents the runner from creating descendants on
/// the agent cgroup before an asynchronous parent-side move can occur.
pub(super) fn attach_to_cgroup_before_exec(command: &mut Command, cgroup: &Path) -> Result<(), ExecutionError> {
  let procs = CString::new(cgroup.join("cgroup.procs").as_os_str().as_bytes())
    .map_err(|_| ExecutionError::Invalid("cgroup path contains a NUL byte".to_owned()))?;
  // SAFETY: only async-signal-safe libc calls run between fork and exec. The
  // path and fixed-size PID buffer are fully allocated in the parent.
  unsafe {
    command.as_std_mut().pre_exec(move || {
      let descriptor = libc::open(procs.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
      if descriptor < 0 {
        return Err(std::io::Error::last_os_error());
      }
      let mut bytes = [0_u8; 20];
      let mut start = bytes.len();
      let mut pid = libc::getpid() as u32;
      while pid != 0 {
        start -= 1;
        bytes[start] = b'0' + (pid % 10) as u8;
        pid /= 10;
      }
      let written = libc::write(descriptor, bytes[start..].as_ptr().cast(), bytes.len() - start);
      let write_error = if written == (bytes.len() - start) as isize {
        None
      } else if written < 0 {
        Some(std::io::Error::last_os_error())
      } else {
        Some(std::io::Error::from_raw_os_error(libc::EIO))
      };
      let close_error = if libc::close(descriptor) == 0 {
        None
      } else {
        Some(std::io::Error::last_os_error())
      };
      match (write_error, close_error) {
        (Some(error), _) | (None, Some(error)) => Err(error),
        (None, None) => Ok(()),
      }
    });
  }
  Ok(())
}

pub(super) fn write_control(cgroup: &Path, name: &str, value: impl AsRef<[u8]>) -> Result<(), ExecutionError> {
  fs::write(cgroup.join(name), value).map_err(|error| backend(format!("configure cgroup controller '{name}': {error}")))
}

/// Reads cumulative CPU time for the complete process tree in milliseconds.
pub(super) fn read_cpu_time(cgroup: &Path) -> Result<u64, ExecutionError> {
  let contents = read_control(cgroup.join("cpu.stat"))?;
  let usage = contents
    .lines()
    .find_map(|line| line.strip_prefix("usage_usec "))
    .ok_or_else(|| backend("cpu.stat does not contain usage_usec"))?;
  parse_number("cpu.stat usage_usec", usage).map(|value| value / 1000)
}

/// Aggregates block I/O counters across every device used by the job cgroup.
pub(super) fn read_io(cgroup: &Path) -> Result<(u64, u64), ExecutionError> {
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

pub(super) fn read_number(path: &Path) -> Result<u64, ExecutionError> {
  let name = path
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or("cgroup counter");
  parse_number(name, read_control(path)?.trim())
}

pub(super) fn read_control(path: impl AsRef<Path>) -> Result<String, ExecutionError> {
  let path = path.as_ref();
  fs::read_to_string(path).map_err(|error| backend(format!("read cgroup controller '{}': {error}", path.display())))
}

pub(super) fn parse_number(name: &str, value: &str) -> Result<u64, ExecutionError> {
  value
    .parse()
    .map_err(|_| backend(format!("{name} contains an invalid counter")))
}

/// Terminates all processes in a job cgroup; absence is already-clean success.
pub(super) fn kill_cgroup(cgroup: &Path) -> Result<(), ExecutionError> {
  match fs::write(cgroup.join("cgroup.kill"), "1") {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(backend(format!(
      "kill execution cgroup '{}': {error}",
      cgroup.display()
    ))),
  }
}

/// Waits for kernel cgroup references to disappear within the cleanup bound.
pub(super) async fn remove_cgroup(cgroup: &Path, timeout: Duration) -> Result<(), ExecutionError> {
  let deadline = Instant::now() + timeout;
  loop {
    match fs::remove_dir(cgroup) {
      Ok(()) => return Ok(()),
      Err(error) if error.raw_os_error() == Some(libc::EBUSY) || error.raw_os_error() == Some(libc::ENOTEMPTY) => {
        if Instant::now() >= deadline {
          break;
        }
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

/// Creates a stable, filesystem-safe cgroup name without exposing the job ID.
pub(super) fn cgroup_name(execution_id: &str) -> String {
  let digest = format!("{:x}", Sha256::digest(execution_id.as_bytes()));
  format!("execution-{}", &digest[..32])
}

/// Restricts orphan cleanup to names that could have been created by this backend.
pub(super) fn is_cgroup_name(name: &std::ffi::OsStr) -> bool {
  let Some(name) = name.to_str() else {
    return false;
  };
  name.len() == 42
    && name.starts_with("execution-")
    && name[10..]
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Signals the complete runner process group, with a direct-child kill fallback.
pub(super) fn kill_process_group(child: &mut Child, signal: i32) {
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
