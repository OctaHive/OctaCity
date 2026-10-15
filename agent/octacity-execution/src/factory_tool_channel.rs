//! Trusted private stream endpoint for one controlled Factory execution.

use std::{
  fmt,
  path::{Path, PathBuf},
};

use crate::ExecutionError;

/// Private Agent-owned socket exposed through a backend's guest-to-host stream.
///
/// This is trusted runtime metadata, never a harness or Flow input. The owning
/// supervisor must retain and clean up the listener/helper for the execution.
/// Endpoint validation alone does not qualify a reader or execution backend.
#[derive(Clone, Eq, PartialEq)]
pub struct FactoryToolChannel {
  host_socket: PathBuf,
  guest_port: u32,
}

impl fmt::Debug for FactoryToolChannel {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("FactoryToolChannel")
      .field("guest_port", &self.guest_port)
      .finish_non_exhaustive()
  }
}

impl FactoryToolChannel {
  /// Validates one existing owner-only Unix socket and a finite guest port.
  /// Links, publicly accessible endpoints and unsupported hosts fail closed.
  pub fn new(host_socket: PathBuf, guest_port: u32) -> Result<Self, ExecutionError> {
    let channel = Self {
      host_socket,
      guest_port,
    };
    channel.validate()?;
    Ok(channel)
  }

  /// Returns the trusted host endpoint for backend projection only.
  pub fn host_socket(&self) -> &Path {
    &self.host_socket
  }

  /// Returns the exact guest-to-host stream port selected by the supervisor.
  pub const fn guest_port(&self) -> u32 {
    self.guest_port
  }

  pub(crate) fn validate(&self) -> Result<(), ExecutionError> {
    if self.guest_port == 0 || self.guest_port == u32::MAX {
      return Err(ExecutionError::Invalid(
        "protected tool-action stream port is invalid".to_owned(),
      ));
    }
    #[cfg(unix)]
    {
      use std::os::unix::fs::FileTypeExt as _;
      let invalid =
        || ExecutionError::Invalid("protected tool-action stream requires a private owned socket".to_owned());
      let metadata = std::fs::symlink_metadata(&self.host_socket).map_err(|_| invalid())?;
      let parent = self.host_socket.parent().ok_or_else(invalid)?;
      if !self.host_socket.is_absolute()
        || !metadata.file_type().is_socket()
        || self.host_socket.canonicalize().map_err(|_| invalid())? != self.host_socket
      {
        return Err(invalid());
      }
      octacity_private_fs::validate_private_access(&self.host_socket).map_err(|_| invalid())?;
      octacity_private_fs::validate_trusted_owner(&self.host_socket).map_err(|_| invalid())?;
      octacity_private_fs::validate_private_access(parent).map_err(|_| invalid())?;
      octacity_private_fs::validate_trusted_directory_chain(parent).map_err(|_| invalid())?;
      Ok(())
    }
    #[cfg(not(unix))]
    {
      Err(ExecutionError::Unavailable(
        "protected tool-action streams are not qualified on this platform".to_owned(),
      ))
    }
  }
}
