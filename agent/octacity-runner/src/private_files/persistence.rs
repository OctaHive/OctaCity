//! Serializes a stand-owned credential installation; the native client owns
//! authentication and refresh. No provider format or OAuth operation lives here.
#[cfg(unix)]
use std::{fs::File, io::Write as _, path::PathBuf};
use std::{io, path::Path};
#[cfg(unix)]
use zeroize::Zeroizing;

pub(super) struct UpdateLease {
  #[cfg(unix)]
  path: PathBuf,
  #[cfg(unix)]
  expected: Zeroizing<Vec<u8>>,
  #[cfg(unix)]
  _lock: File,
}
impl UpdateLease {
  #[cfg(unix)]
  pub(super) fn acquire(path: &Path, expected: &[u8]) -> io::Result<Self> {
    let parent = path.parent().ok_or_else(invalid)?;
    octacity_private_fs::validate_trusted_directory_chain(parent)?;
    octacity_private_fs::validate_private_access(parent)?;
    let name = path.file_name().ok_or_else(invalid)?;
    let mut lock_name = name.to_os_string();
    lock_name.push(".octacity.lock");
    let lock_path = parent.join(lock_name);
    match tempfile::NamedTempFile::new_in(parent)?.persist_noclobber(&lock_path) {
      Ok(_) => {}
      Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {}
      Err(error) => return Err(error.error),
    }
    let lock = octacity_private_fs::open_regular_file_no_follow(&lock_path)?;
    octacity_private_fs::validate_trusted_owner(&lock_path)?;
    octacity_private_fs::validate_private_access(&lock_path)?;
    lock.try_lock().map_err(io::Error::other)?;
    let lease = Self {
      path: path.to_owned(),
      expected: Zeroizing::new(expected.to_vec()),
      _lock: lock,
    };
    lease.verify_current()?;
    Ok(lease)
  }

  #[cfg(unix)]
  fn verify_current(&self) -> io::Result<()> {
    octacity_private_fs::validate_trusted_owner(&self.path)?;
    octacity_private_fs::validate_private_access(&self.path)?;
    let actual = Zeroizing::new(octacity_private_fs::read_bounded_regular_file(
      &self.path,
      super::MAX_PRIVATE_JSON_BYTES,
    )?);
    if actual != self.expected {
      return Err(invalid());
    }
    Ok(())
  }

  #[cfg(unix)]
  pub(super) fn persist(&mut self, current: &[u8]) -> io::Result<()> {
    self.verify_current()?;
    if self.expected.as_slice() == current {
      return Ok(());
    }
    let parent = self.path.parent().ok_or_else(invalid)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(current)?;
    staged.as_file().sync_all()?;
    staged.persist(&self.path).map_err(|error| error.error)?;
    File::open(parent)?.sync_all()?;
    self.expected = Zeroizing::new(current.to_vec());
    Ok(())
  }

  #[cfg(not(unix))]
  pub(super) fn acquire(_: &Path, _: &[u8]) -> io::Result<Self> {
    Err(invalid())
  }
  #[cfg(not(unix))]
  pub(super) fn persist(&mut self, _: &[u8]) -> io::Result<()> {
    Err(invalid())
  }
}
fn invalid() -> io::Error {
  io::Error::other("private credential update ownership is invalid")
}
