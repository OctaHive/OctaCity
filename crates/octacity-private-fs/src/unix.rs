//! Unix mode implementation for private agent state.

use std::{
  fs,
  os::unix::fs::{DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _},
  path::Path,
};

pub(super) fn create_private_directory(path: &Path) -> std::io::Result<()> {
  let mut builder = fs::DirBuilder::new();
  builder.mode(0o700).create(path)
}

pub(super) fn validate_private_access(path: &Path) -> std::io::Result<()> {
  let mode = fs::metadata(path)?.permissions().mode();
  if mode & 0o077 != 0 {
    return Err(std::io::Error::new(
      std::io::ErrorKind::PermissionDenied,
      "path is accessible by group or other users",
    ));
  }
  Ok(())
}

pub(super) fn validate_trusted_owner(path: &Path) -> std::io::Result<()> {
  let owner = fs::symlink_metadata(path)?.uid();
  // SAFETY: geteuid has no preconditions and does not retain memory.
  let agent = unsafe { libc::geteuid() };
  if trusted_uid(owner, agent) {
    Ok(())
  } else {
    Err(std::io::Error::new(
      std::io::ErrorKind::PermissionDenied,
      "path is not owned by the agent account or root",
    ))
  }
}

fn trusted_uid(owner: u32, agent: u32) -> bool {
  owner == agent || owner == 0
}

pub(super) fn validate_trusted_directory_chain(path: &Path) -> std::io::Result<()> {
  for directory in path.ancestors() {
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir() {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "trusted path chain contains a non-directory",
      ));
    }
    validate_trusted_owner(directory)?;
    let mode = metadata.mode();
    if mode & 0o022 != 0 && mode & libc::S_ISVTX as u32 == 0 {
      return Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "trusted path chain contains a writable non-sticky directory",
      ));
    }
  }
  Ok(())
}

pub(super) fn open_regular_file_no_follow(path: &Path) -> std::io::Result<fs::File> {
  let file = fs::OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
    .open(path)?;
  if !file.metadata()?.file_type().is_file() {
    return Err(std::io::Error::new(
      std::io::ErrorKind::InvalidInput,
      "path is not a regular file",
    ));
  }
  Ok(file)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn trusts_only_the_agent_and_root_owners() {
    assert!(trusted_uid(1000, 1000));
    assert!(trusted_uid(0, 1000));
    assert!(!trusted_uid(2000, 1000));
  }

  #[test]
  fn rejects_a_writable_non_sticky_directory_in_the_chain() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("replaceable");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o777)).unwrap();

    assert!(validate_trusted_directory_chain(&directory).is_err());
  }
}
