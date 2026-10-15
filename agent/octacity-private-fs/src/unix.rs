//! Unix mode implementation for private agent state.

use std::{
  ffi::CString,
  fs,
  os::fd::{AsRawFd as _, FromRawFd as _},
  os::unix::ffi::OsStrExt as _,
  os::unix::fs::{DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _},
  path::{Component, Path},
};

// `MetadataExt::mode` uses a portable `u32` representation. Naming the POSIX
// sticky-directory bit here avoids a target-dependent libc integer cast:
// `S_ISVTX` is `u32` on Linux but a signed integer on macOS.
const STICKY_DIRECTORY_BIT: u32 = 0o1000;

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

pub(super) fn validate_no_untrusted_write_access(path: &Path) -> std::io::Result<()> {
  let mode = fs::symlink_metadata(path)?.permissions().mode();
  if mode & 0o022 != 0 {
    return Err(std::io::Error::new(
      std::io::ErrorKind::PermissionDenied,
      "path is writable by group or other users",
    ));
  }
  Ok(())
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
    if mode & 0o022 != 0 && mode & STICKY_DIRECTORY_BIT == 0 {
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

pub(super) fn open_rooted_read_directory(path: &Path) -> std::io::Result<fs::File> {
  fs::OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
    .open(path)
}

pub(super) fn open_rooted_regular_file(root: &fs::File, relative: &Path) -> std::io::Result<fs::File> {
  if relative.as_os_str().is_empty()
    || !relative
      .components()
      .all(|component| matches!(component, Component::Normal(_)))
  {
    return Err(std::io::Error::new(
      std::io::ErrorKind::InvalidInput,
      "read path must remain below its root",
    ));
  }
  let mut directory = root.try_clone()?;
  let mut components = relative.components().peekable();
  while let Some(Component::Normal(component)) = components.next() {
    let name = CString::new(component.as_bytes())
      .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "read path contains a nul byte"))?;
    let is_directory = components.peek().is_some();
    let flags = libc::O_RDONLY
      | libc::O_NOFOLLOW
      | libc::O_CLOEXEC
      | libc::O_NONBLOCK
      | if is_directory { libc::O_DIRECTORY } else { 0 };
    // SAFETY: the retained directory fd and nul-terminated component remain
    // valid for this call. No creation flag is set, so no variadic mode is used.
    let descriptor = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
    if descriptor < 0 {
      return Err(std::io::Error::last_os_error());
    }
    // SAFETY: successful openat transfers exactly one new owned descriptor.
    let opened = unsafe { fs::File::from_raw_fd(descriptor) };
    if !is_directory {
      if !opened.metadata()?.is_file() {
        return Err(std::io::Error::new(
          std::io::ErrorKind::InvalidInput,
          "read target is not a regular file",
        ));
      }
      return Ok(opened);
    }
    directory = opened;
  }
  Err(std::io::Error::new(
    std::io::ErrorKind::InvalidInput,
    "read path does not name a file",
  ))
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
