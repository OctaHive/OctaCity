//! Validates and accounts for agent-owned containerd filesystem state.

use super::*;

/// Canonicalizes a configured directory before it becomes a cleanup boundary.
pub(super) fn canonical_directory(name: &str, path: &Path) -> Result<PathBuf, ExecutionError> {
  if !path.is_absolute() || !path.is_dir() {
    return Err(invalid(format!("{name} must be an existing absolute directory")));
  }
  path
    .canonicalize()
    .map_err(|error| backend(format!("canonicalize {name} '{}': {error}", path.display())))
}

pub(super) fn set_private_permissions(path: &Path) -> Result<(), ExecutionError> {
  use std::os::unix::fs::PermissionsExt as _;
  fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(ExecutionError::Io)
}

/// Creates a private FIFO directly, without invoking an ambient shell.
pub(super) fn create_fifo(path: &Path) -> Result<(), ExecutionError> {
  let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| invalid("containerd FIFO path contains NUL"))?;
  // SAFETY: `path` is a valid NUL-terminated string and mkfifo does not retain it.
  if unsafe { libc::mkfifo(path.as_ptr(), 0o600) } != 0 {
    return Err(ExecutionError::Io(std::io::Error::last_os_error()));
  }
  Ok(())
}

pub(super) fn map_path(root: &Path, path: &Path, guest_root: &Path) -> Result<PathBuf, ExecutionError> {
  let relative = path
    .strip_prefix(root)
    .map_err(|_| invalid(format!("path '{}' is outside '{}'", path.display(), root.display())))?;
  Ok(guest_root.join(relative))
}

/// Produces an agent-scoped opaque resource name for containerd objects.
pub(super) fn resource_id(agent_id: &str, execution_id: &str) -> String {
  let digest = Sha256::digest(format!("{agent_id}\0{execution_id}").as_bytes());
  format!("{}-{:.16x}", agent_resource_prefix(agent_id), digest)
}

pub(super) fn agent_resource_prefix(agent_id: &str) -> String {
  let digest = Sha256::digest(agent_id.as_bytes());
  format!("octacity-{}-{:.12x}", sanitize_id(agent_id), digest)
}

pub(super) fn sanitize_id(value: &str) -> String {
  value
    .chars()
    .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    .take(32)
    .collect()
}

/// Removes only abandoned I/O directories bearing this agent's hashed prefix.
pub(super) fn remove_abandoned_io_directories(root: &Path, agent_id: &str) -> Result<(), ExecutionError> {
  let prefix = format!("{}-", agent_resource_prefix(agent_id));
  for entry in fs::read_dir(root).map_err(ExecutionError::Io)? {
    let entry = entry.map_err(ExecutionError::Io)?;
    let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
      continue;
    };
    if name.starts_with(&prefix) && entry.file_type().map_err(ExecutionError::Io)?.is_dir() {
      fs::remove_dir_all(entry.path()).map_err(ExecutionError::Io)?;
    }
  }
  Ok(())
}

pub(super) fn filesystem_capacity(path: &Path) -> Result<u64, ExecutionError> {
  let stats = filesystem_stats(path)?;
  #[cfg(target_os = "macos")]
  let blocks = u64::from(stats.f_blocks);
  #[cfg(target_os = "linux")]
  let blocks = stats.f_blocks;
  Ok(blocks.saturating_mul(stats.f_frsize))
}

/// Requires `work_root` to be a dedicated filesystem no larger than policy.
pub(super) fn validate_work_root(root: &Path, limit: u64) -> Result<(), ExecutionError> {
  let parent = root
    .parent()
    .ok_or_else(|| unavailable("containerd work_root has no parent directory"))?;
  let root_metadata = fs::metadata(root).map_err(ExecutionError::Io)?;
  let parent_metadata = fs::metadata(parent).map_err(ExecutionError::Io)?;
  if root_metadata.dev() == parent_metadata.dev() {
    return Err(unavailable(
      "containerd work_root must be a dedicated filesystem mount so its disk limit is enforceable",
    ));
  }
  let capacity = filesystem_capacity(root)?;
  if capacity > limit {
    return Err(unavailable(format!(
      "containerd work_root capacity {capacity} exceeds writable disk limit {limit}"
    )));
  }
  Ok(())
}

/// Ensures materialization did not move the workspace across the quota boundary.
pub(super) fn validate_workspace_filesystem(root: &Path, workspace: &Path, limit: u64) -> Result<(), ExecutionError> {
  let root_metadata = fs::metadata(root).map_err(ExecutionError::Io)?;
  let workspace_metadata = fs::metadata(workspace).map_err(ExecutionError::Io)?;
  if workspace_metadata.dev() != root_metadata.dev() {
    return Err(unavailable(
      "containerd workspace must remain on the configured work_root filesystem",
    ));
  }
  if filesystem_capacity(root)? > limit {
    return Err(unavailable(
      "containerd work_root is larger than the signed writable disk limit",
    ));
  }
  Ok(())
}

/// Returns bytes currently consumed on the quota-backed workspace filesystem.
pub(super) fn filesystem_usage(path: &Path) -> Result<u64, ExecutionError> {
  let stats = filesystem_stats(path)?;
  #[cfg(target_os = "macos")]
  let used_blocks = u64::from(stats.f_blocks.saturating_sub(stats.f_bfree));
  #[cfg(target_os = "linux")]
  let used_blocks = stats.f_blocks.saturating_sub(stats.f_bfree);
  Ok(used_blocks.saturating_mul(stats.f_frsize))
}

pub(super) fn filesystem_stats(path: &Path) -> Result<libc::statvfs, ExecutionError> {
  let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| invalid("filesystem path contains NUL"))?;
  let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
  // SAFETY: `path` is NUL-terminated and `stats` points to writable memory.
  if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
    return Err(ExecutionError::Io(std::io::Error::last_os_error()));
  }
  // SAFETY: statvfs initialized `stats` after returning success.
  Ok(unsafe { stats.assume_init() })
}
