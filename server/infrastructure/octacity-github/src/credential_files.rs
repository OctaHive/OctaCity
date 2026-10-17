//! Descriptor-relative private installation writes; paths never come from a credential request.
use octacity_server_secrets::ProviderCredentialError as Error;
use rustix::fs::{self, Mode, OFlags, RenameFlags};
use std::{
  fs::File,
  io::{Read as _, Write as _},
  os::{
    fd::{AsFd, BorrowedFd},
    unix::fs::MetadataExt as _,
  },
  path::{Component, Path, PathBuf},
};

const DIRECTORY: OFlags = OFlags::RDONLY
  .union(OFlags::DIRECTORY)
  .union(OFlags::NOFOLLOW)
  .union(OFlags::CLOEXEC);

pub(super) struct CredentialDirectory {
  root: PathBuf,
  relative: PathBuf,
  root_identity: (u64, u64),
  directory_identity: (u64, u64),
}
/// Owns the operation's lock, independently of duplicate/inherited descriptors.
pub(super) struct CredentialLock(File);
impl AsFd for CredentialLock {
  fn as_fd(&self) -> BorrowedFd<'_> {
    self.0.as_fd()
  }
}
impl Drop for CredentialLock {
  fn drop(&mut self) {
    // CLOEXEC closes inherited descriptors only at exec. Release the lock at
    // scope exit even if a concurrent child still holds the open file description.
    let _ = fs::flock(&self.0, fs::FlockOperation::Unlock);
  }
}
impl CredentialDirectory {
  pub(super) fn lock(&self) -> Result<CredentialLock, Error> {
    let directory = self.directory()?;
    fs::flock(&directory, fs::FlockOperation::NonBlockingLockExclusive).map_err(|_| Error::Conflict)?;
    Ok(CredentialLock(directory))
  }
  pub(super) fn read(&self, handle: &str) -> Result<zeroize::Zeroizing<Vec<u8>>, Error> {
    let directory = self.directory()?;
    let file = File::from(
      fs::openat(
        &directory,
        crate::config::credential_file_name(handle),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
      )
      .map_err(|error| {
        if error == rustix::io::Errno::NOENT {
          Error::NotConfigured
        } else {
          Error::Denied
        }
      })?,
    );
    let meta = file.metadata().map_err(|_| Error::Denied)?;
    if !meta.is_file()
      || meta.nlink() != 1
      || meta.mode() & 0o077 != 0
      || meta.uid() != rustix::process::geteuid().as_raw()
      || meta.len() > crate::config::MAX_CREDENTIAL_FILE_BYTES as u64
    {
      return Err(Error::Denied);
    }
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    file
      .take(crate::config::MAX_CREDENTIAL_FILE_BYTES as u64 + 1)
      .read_to_end(&mut bytes)
      .map_err(|_| Error::Unavailable)?;
    if bytes.len() > crate::config::MAX_CREDENTIAL_FILE_BYTES {
      return Err(Error::Denied);
    }
    Ok(bytes)
  }
  pub(super) fn new(root: &Path, relative: &Path) -> Result<Self, Error> {
    if !root.is_absolute()
      || std::fs::symlink_metadata(root)
        .map_err(|_| Error::Denied)?
        .file_type()
        .is_symlink()
      || relative.as_os_str().is_empty()
      || !relative.components().all(|c| matches!(c, Component::Normal(_)))
    {
      return Err(Error::Denied);
    }
    let root = root.canonicalize().map_err(|_| Error::Denied)?;
    let base = open_absolute(&root)?;
    let directory = descend(&base, relative)?;
    Ok(Self {
      root,
      relative: relative.into(),
      root_identity: identity(&base)?,
      directory_identity: identity(&directory)?,
    })
  }
  fn directory(&self) -> Result<File, Error> {
    let root = open_absolute(&self.root)?;
    if identity(&root)? != self.root_identity {
      return Err(Error::Denied);
    }
    let directory = descend(&root, &self.relative)?;
    if identity(&directory)? != self.directory_identity {
      return Err(Error::Denied);
    }
    Ok(directory)
  }
  pub(super) fn publish(
    &self,
    handle: &str,
    bytes: &[u8],
    mode: crate::credential_management::PublishMode,
    publication: Option<&octacity_server_secrets::CredentialPublication>,
  ) -> Result<(), Error> {
    let directory = self.directory()?;
    let name = crate::config::credential_file_name(handle);
    // All callers hold the directory lock. Stable operation-scoped names let
    // recovery erase partial writes without enumerating unrelated preparations.
    self.remove_temporary(handle, publication)?;
    let temporary = temporary_name(handle, publication);
    let mut file = File::from(
      fs::openat(
        &directory,
        &temporary,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
      )
      .map_err(|_| Error::Unavailable)?,
    );
    let result = (|| {
      fs::fchmod(&file, Mode::RUSR | Mode::WUSR).map_err(|_| Error::Unavailable)?;
      file.write_all(bytes).map_err(|_| Error::Unavailable)?;
      file.sync_all().map_err(|_| Error::Unavailable)?;
      // Recheck the installed ancestry before publishing through its opened descriptor.
      self.directory()?;
      fs::renameat_with(
        &directory,
        &temporary,
        &directory,
        &name,
        match mode {
          crate::credential_management::PublishMode::Create => RenameFlags::NOREPLACE,
          crate::credential_management::PublishMode::Rotate => RenameFlags::empty(),
        },
      )
      .map_err(|e| {
        if e == rustix::io::Errno::EXIST {
          Error::Conflict
        } else {
          Error::Unavailable
        }
      })?;
      directory.sync_all().map_err(|_| Error::Unavailable)
    })();
    // Remove only our still-owned temporary inode, never a substituted path.
    if let (Ok(current), Ok(original)) = (
      fs::statat(&directory, &temporary, fs::AtFlags::SYMLINK_NOFOLLOW),
      file.metadata(),
    ) {
      #[cfg(target_os = "macos")]
      let device = current.st_dev as u64;
      #[cfg(target_os = "linux")]
      let device = current.st_dev;
      if device == original.dev() && current.st_ino == original.ino() {
        let _ = fs::unlinkat(&directory, &temporary, fs::AtFlags::empty());
      }
    }
    result
  }

  /// Removes only this publication's temporary inode under the caller's directory lock.
  /// Exact descriptor-relative names also cover incomplete, undecodable JSON writes.
  pub(super) fn remove_temporary(
    &self,
    handle: &str,
    publication: Option<&octacity_server_secrets::CredentialPublication>,
  ) -> Result<(), Error> {
    let directory = self.directory()?;
    let name = temporary_name(handle, publication);
    let file = match fs::openat(
      &directory,
      &name,
      OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
      Mode::empty(),
    ) {
      Ok(file) => File::from(file),
      Err(rustix::io::Errno::NOENT) => return Ok(()),
      Err(_) => return Err(Error::Denied),
    };
    let original = file.metadata().map_err(|_| Error::Denied)?;
    if !original.is_file()
      || original.nlink() != 1
      || original.mode() & 0o077 != 0
      || original.uid() != rustix::process::geteuid().as_raw()
    {
      return Err(Error::Denied);
    }
    let current = fs::statat(&directory, &name, fs::AtFlags::SYMLINK_NOFOLLOW).map_err(|_| Error::Denied)?;
    #[cfg(target_os = "macos")]
    let device = current.st_dev as u64;
    #[cfg(target_os = "linux")]
    let device = current.st_dev;
    if device != original.dev() || current.st_ino != original.ino() {
      return Err(Error::Denied);
    }
    self.directory()?;
    fs::unlinkat(&directory, &name, fs::AtFlags::empty()).map_err(|_| Error::Unavailable)?;
    directory.sync_all().map_err(|_| Error::Unavailable)
  }
}
fn temporary_name(handle: &str, publication: Option<&octacity_server_secrets::CredentialPublication>) -> String {
  let name = crate::config::credential_file_name(handle);
  match publication {
    Some(publication) => format!(".octacity-credential-{name}-{}.tmp", hex::encode(publication.operation)),
    None => format!(".octacity-credential-{name}.tmp"),
  }
}
fn identity(file: &File) -> Result<(u64, u64), Error> {
  let meta = file.metadata().map_err(|_| Error::Denied)?;
  if !meta.is_dir() || meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o077 != 0 {
    return Err(Error::Denied);
  }
  Ok((meta.dev(), meta.ino()))
}
fn open_absolute(path: &Path) -> Result<File, Error> {
  let mut directory = File::from(fs::open("/", DIRECTORY, Mode::empty()).map_err(|_| Error::Denied)?);
  for component in path.components() {
    if let Component::Normal(name) = component {
      directory = File::from(fs::openat(&directory, name, DIRECTORY, Mode::empty()).map_err(|_| Error::Denied)?);
    }
  }
  identity(&directory)?;
  Ok(directory)
}
fn descend(root: &File, relative: &Path) -> Result<File, Error> {
  let mut directory = root.try_clone().map_err(|_| Error::Denied)?;
  for component in relative.components() {
    let Component::Normal(name) = component else {
      return Err(Error::Denied);
    };
    directory = File::from(fs::openat(&directory, name, DIRECTORY, Mode::empty()).map_err(|_| Error::Denied)?);
    identity(&directory)?;
  }
  Ok(directory)
}
