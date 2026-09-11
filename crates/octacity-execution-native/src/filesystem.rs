//! Validates the quota-backed Native workspace filesystem.

use super::*;

/// Capacity and current usage of the filesystem enforcing a job's disk limit.
pub(super) struct FilesystemUsage {
  pub(super) capacity_bytes: u64,
  pub(super) used_bytes: u64,
}

/// Requires Native workspaces to live on a dedicated, bounded filesystem.
pub(super) fn validate_workspace_root(root: &Path, limit: u64) -> Result<(), ExecutionError> {
  let root_metadata = fs::metadata(root).map_err(ExecutionError::Io)?;
  let parent = root
    .parent()
    .ok_or_else(|| unavailable("native work_root has no parent directory"))?;
  let parent_metadata = fs::metadata(parent).map_err(ExecutionError::Io)?;
  if root_metadata.dev() == parent_metadata.dev() {
    return Err(unavailable(
      "native work_root must be a dedicated filesystem mount so its disk limit is enforceable",
    ));
  }
  let usage = filesystem_usage(root)?;
  if usage.capacity_bytes > limit {
    return Err(unavailable(format!(
      "native work_root capacity {} exceeds signed writable disk limit {limit}",
      usage.capacity_bytes
    )));
  }
  Ok(())
}

/// Ensures a prepared workspace did not escape its configured filesystem.
pub(super) fn validate_workspace_filesystem(root: &Path, workspace: &Path, limit: u64) -> Result<(), ExecutionError> {
  let root_metadata = fs::metadata(root).map_err(ExecutionError::Io)?;
  let workspace_metadata = fs::metadata(workspace).map_err(ExecutionError::Io)?;
  if workspace_metadata.dev() != root_metadata.dev() {
    return Err(unavailable(
      "native workspace must remain on the configured work_root filesystem",
    ));
  }
  validate_workspace_root(root, limit)
}

/// Reads filesystem-wide capacity and consumption for resource reporting.
pub(super) fn filesystem_usage(path: &Path) -> Result<FilesystemUsage, ExecutionError> {
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
