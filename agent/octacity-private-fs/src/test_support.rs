//! Cross-platform private-directory fixtures for downstream tests.

use std::path::{Path, PathBuf};

#[cfg(windows)]
const CREATE_ATTEMPTS: usize = 16;

/// An automatically removed directory with production-equivalent private access.
pub struct PrivateDirectoryFixture {
  path: PathBuf,
  #[cfg(not(windows))]
  _temporary: tempfile::TempDir,
}

impl PrivateDirectoryFixture {
  /// Creates a unique private directory suitable for credential and state tests.
  pub fn new() -> std::io::Result<Self> {
    #[cfg(windows)]
    {
      let profile = std::env::var_os("USERPROFILE")
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Windows tests require USERPROFILE"))?;
      let profile = std::fs::canonicalize(profile)?;
      let volume_root = profile.ancestors().last().ok_or_else(|| {
        std::io::Error::new(
          std::io::ErrorKind::InvalidInput,
          "Windows USERPROFILE has no volume root",
        )
      })?;
      for _ in 0..CREATE_ATTEMPTS {
        let path = volume_root.join(format!(".octacity-private-test-{}", uuid::Uuid::new_v4().simple()));
        match crate::create_private_directory(&path) {
          Ok(()) => return Ok(Self { path }),
          Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
          Err(error) => return Err(error),
        }
      }
      Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "failed to allocate a unique protected Windows test directory",
      ))
    }
    #[cfg(not(windows))]
    {
      use std::os::unix::fs::PermissionsExt as _;

      let temporary = tempfile::tempdir()?;
      std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o700))?;
      Ok(Self {
        path: temporary.path().canonicalize()?,
        _temporary: temporary,
      })
    }
  }

  /// Returns the protected directory path.
  pub fn path(&self) -> &Path {
    &self.path
  }
}

#[cfg(windows)]
impl Drop for PrivateDirectoryFixture {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.path);
  }
}
