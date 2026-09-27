//! Filesystem validation and accounting for host bind mounts.

use std::{
  ffi::CString,
  fs,
  os::unix::ffi::OsStrExt as _,
  os::unix::fs::MetadataExt as _,
  path::{Path, PathBuf},
};

use octacity_execution::ExecutionError;

use super::{backend, invalid};

pub(super) fn canonical_directory(name: &str, path: &Path) -> Result<PathBuf, ExecutionError> {
  if !path.is_absolute() || !path.is_dir() {
    return Err(invalid(format!("{name} must be an existing absolute directory")));
  }
  path
    .canonicalize()
    .map_err(|error| backend(format!("canonicalize {name}: {error}")))
}

pub(super) fn canonical_executable(path: &Path) -> Result<PathBuf, ExecutionError> {
  if !path.is_absolute() || !path.is_file() {
    return Err(invalid("Apple container executable must be an existing absolute file"));
  }
  let metadata = fs::metadata(path).map_err(ExecutionError::Io)?;
  if metadata.mode() & 0o111 == 0 {
    return Err(invalid("Apple container executable must be executable"));
  }
  path
    .canonicalize()
    .map_err(|error| backend(format!("canonicalize Apple container executable: {error}")))
}

/// Ensures the bind-mounted workspace remains on a filesystem whose physical
/// capacity cannot exceed the signed job limit.
pub(super) fn validate_workspace_filesystem(root: &Path, workspace: &Path, limit: u64) -> Result<(), ExecutionError> {
  let root_metadata = fs::metadata(root).map_err(ExecutionError::Io)?;
  let workspace_metadata = fs::metadata(workspace).map_err(ExecutionError::Io)?;
  if root_metadata.dev() != workspace_metadata.dev() {
    return Err(ExecutionError::Unavailable(
      "Apple VF workspace must remain on the configured work_root filesystem".to_owned(),
    ));
  }
  let capacity = filesystem_capacity(root)?;
  if capacity > limit {
    return Err(ExecutionError::Unavailable(format!(
      "Apple VF work_root capacity {capacity} exceeds writable disk limit {limit}"
    )));
  }
  Ok(())
}

pub(super) fn filesystem_usage(path: &Path) -> Result<u64, ExecutionError> {
  let stats = filesystem_stats(path)?;
  let used_blocks = u64::from(stats.f_blocks.saturating_sub(stats.f_bfree));
  Ok(used_blocks.saturating_mul(stats.f_frsize))
}

fn filesystem_capacity(path: &Path) -> Result<u64, ExecutionError> {
  let stats = filesystem_stats(path)?;
  Ok(u64::from(stats.f_blocks).saturating_mul(stats.f_frsize))
}

fn filesystem_stats(path: &Path) -> Result<libc::statvfs, ExecutionError> {
  let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| invalid("filesystem path contains NUL"))?;
  let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
  // SAFETY: `path` is NUL-terminated and `stats` points to writable memory.
  if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
    return Err(ExecutionError::Io(std::io::Error::last_os_error()));
  }
  // SAFETY: statvfs initialized `stats` after returning success.
  Ok(unsafe { stats.assume_init() })
}
