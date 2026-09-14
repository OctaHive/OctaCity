//! Cross-platform primitives for agent-owned private filesystem state.
//!
//! Unix callers receive explicit owner-only modes. Windows callers receive a
//! protected DACL granting access only to the object owner, LocalSystem, and
//! built-in administrators. Centralizing this boundary avoids security-critical
//! permission no-ops and keeps platform code out of orchestration crates.

#![warn(missing_docs)]

use std::{fs::File, path::Path};

/// Stable identity of one open Windows filesystem object.
///
/// The volume serial and 128-bit file ID distinguish a replacement file even
/// when it has the same path, length, timestamps, and attributes.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIdentity {
  volume_serial: u64,
  file_id: [u8; 16],
}

/// Creates exactly one owner-only directory without creating missing parents.
///
/// The operation fails when `path` already exists. Files subsequently created
/// below the directory inherit its private access policy.
pub fn create_private_directory(path: &Path) -> std::io::Result<()> {
  platform::create_private_directory(path)
}

/// Verifies that no untrusted local principal is granted access to `path`.
///
/// This checks the final object, not merely its portable read-only attribute.
/// Symlink and regular-file checks remain the caller's responsibility.
pub fn validate_private_access(path: &Path) -> std::io::Result<()> {
  platform::validate_private_access(path)
}

/// Verifies that `path` is owned by the agent account or a trusted system owner.
pub fn validate_trusted_owner(path: &Path) -> std::io::Result<()> {
  platform::validate_trusted_owner(path)
}

/// Verifies ownership and replacement safety through the volume-root chain.
///
/// On Unix, a world-writable sticky directory such as `/tmp` remains valid,
/// because the sticky bit prevents another user from renaming an agent-owned
/// child. A writable non-sticky ancestor is rejected. On Windows, untrusted
/// allow-ACEs may retain read/traverse rights but not create, delete-child,
/// ownership, or DACL mutation rights.
pub fn validate_trusted_directory_chain(path: &Path) -> std::io::Result<()> {
  platform::validate_trusted_directory_chain(path)
}

/// Opens the final path only when the opened object is a regular file.
///
/// The final symlink or Windows reparse point is never followed. Validation is
/// performed through the returned handle, closing the usual check-then-open
/// race when an operator-managed credential file is rotated concurrently.
pub fn open_regular_file_no_follow(path: &Path) -> std::io::Result<File> {
  platform::open_regular_file_no_follow(path)
}

/// Reads the stable identity of an already open Windows file handle.
///
/// Callers should keep the handle open while consuming the file and compare
/// this identity with a newly opened path afterward. This detects replacement
/// without using Rust's currently unstable Windows metadata extensions.
#[cfg(windows)]
pub fn file_identity(file: &File) -> std::io::Result<FileIdentity> {
  platform::file_identity(file)
}

/// Reports whether a Windows path is implemented by a reparse point.
///
/// Directory junctions are not consistently reported as Rust symbolic links.
/// Output traversal uses this Windows-specific check to reject every object
/// that could redirect a walk outside the declared artifact root.
#[cfg(windows)]
pub fn is_reparse_point(path: &Path) -> std::io::Result<bool> {
  platform::is_reparse_point(path)
}

#[cfg(unix)]
#[path = "unix.rs"]
mod platform;

#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

#[cfg(not(any(unix, windows)))]
compile_error!("octacity-private-fs requires either a Unix or Windows target");

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn creates_and_validates_a_private_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("private");

    create_private_directory(&directory).unwrap();
    validate_private_access(&directory).unwrap();
    assert!(create_private_directory(&directory).is_err());
  }

  #[test]
  fn opens_only_regular_files() {
    let temporary = tempfile::tempdir().unwrap();
    let file = temporary.path().join("identity");
    std::fs::write(&file, "credential").unwrap();

    assert!(open_regular_file_no_follow(&file).is_ok());
    assert!(open_regular_file_no_follow(temporary.path()).is_err());
  }

  #[cfg(unix)]
  #[test]
  fn never_follows_the_final_symlink() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    let target = temporary.path().join("target");
    let link = temporary.path().join("link");
    std::fs::write(&target, "credential").unwrap();
    symlink(target, &link).unwrap();

    assert!(open_regular_file_no_follow(&link).is_err());
  }

  #[cfg(windows)]
  #[test]
  fn never_follows_the_final_reparse_point() {
    use std::os::windows::fs::symlink_file;

    let temporary = tempfile::tempdir().unwrap();
    let target = temporary.path().join("target");
    let link = temporary.path().join("link");
    std::fs::write(&target, "credential").unwrap();
    match symlink_file(target, &link) {
      Ok(()) => {}
      // Some self-hosted Windows workers do not grant symbolic-link creation
      // to the test account. The production no-follow path still compiles and
      // is exercised wherever the host permits constructing a reparse point.
      Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
      Err(error) => panic!("failed to create the test reparse point: {error}"),
    }

    assert!(open_regular_file_no_follow(&link).is_err());
  }
}
