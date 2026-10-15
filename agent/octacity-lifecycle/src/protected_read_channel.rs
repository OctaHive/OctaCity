//! Owned Unix listener for the neutral protected-reader channel.

use std::{
  os::unix::fs::{MetadataExt as _, PermissionsExt as _},
  path::PathBuf,
  sync::Arc,
};

use octacity_execution::{ExecutionError, FactoryToolChannel};
use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  net::UnixListener,
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use super::ProtectedReadDispatcher;

#[path = "protected_read_execution.rs"]
mod execution;

/// One sequential, bounded reader listener owned by the execution supervisor.
/// Each connection supplies one versioned authorization/read request. Responses contain a
/// status byte (0 success, 1 refusal), a big-endian u32 length, and bounded bytes.
/// The peer owns normal connection closure; cancellation or the shared operation
/// deadline also closes it. This listener does not enforce helper process limits.
pub struct ProtectedReadChannel {
  endpoint: FactoryToolChannel,
  cancellation: CancellationToken,
  task: Option<JoinHandle<()>>,
  socket: OwnedSocket,
}

impl ProtectedReadChannel {
  /// Binds a new socket in an existing canonical, private trusted directory.
  /// Existing endpoints are never replaced. A Tokio runtime must be active.
  pub fn bind(
    dispatcher: Arc<ProtectedReadDispatcher>,
    path: PathBuf,
    guest_port: u32,
  ) -> Result<Self, ExecutionError> {
    let parent = path.parent().ok_or_else(invalid_endpoint)?;
    if !parent.is_absolute()
      || parent.canonicalize().map_err(|_| invalid_endpoint())? != parent
      || octacity_private_fs::validate_private_access(parent).is_err()
      || octacity_private_fs::validate_trusted_directory_chain(parent).is_err()
    {
      return Err(invalid_endpoint());
    }
    let runtime = tokio::runtime::Handle::try_current()
      .map_err(|_| unavailable("private reader requires an active async runtime"))?;
    let listener = UnixListener::bind(&path).map_err(|_| unavailable("private reader endpoint could not be bound"))?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| invalid_endpoint())?;
    let socket = OwnedSocket {
      path: path.clone(),
      identity: (metadata.dev(), metadata.ino()),
      owned_parent: None,
      removed: false,
    };
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|_| invalid_endpoint())?;
    let endpoint = FactoryToolChannel::new(path, guest_port)?;
    let (duration, cancellation) = dispatcher.channel_scope();
    let scope = cancellation.clone();
    let task = runtime.spawn(async move {
      loop {
        let peer = tokio::select! {
          biased;
          () = scope.cancelled() => break,
          result = listener.accept() => match result {
            Ok((peer, _)) => peer,
            Err(_) => break,
          },
        };
        let serve = async {
          let mut peer = peer;
          let result = dispatcher.read_channel(&mut peer).await;
          if scope.is_cancelled() {
            return;
          }
          let (status, bytes) = match result {
            Ok(bytes) => (0, bytes),
            Err(_) => (1, Vec::new()),
          };
          let Ok(length) = u32::try_from(bytes.len()) else {
            return;
          };
          if peer.write_u8(status).await.is_err()
            || peer.write_u32(length).await.is_err()
            || peer.write_all(&bytes).await.is_err()
          {
            return;
          }
          // Keep the endpoint alive until peer closure or the operation bound;
          // immediate host close can lose queued bytes in the vsock bridge.
          let mut end = [0; 1];
          let _ = peer.read(&mut end).await;
        };
        tokio::select! {
          biased;
          () = scope.cancelled() => break,
          _ = tokio::time::timeout(duration, serve) => {},
        }
      }
    });
    Ok(Self {
      endpoint,
      cancellation,
      task: Some(task),
      socket,
    })
  }

  /// Transfers removal of the freshly created private directory to this owner.
  /// Only composition inside this crate may transfer that directory authority.
  pub(crate) fn with_owned_parent(mut self) -> Self {
    self.socket.owned_parent = self.socket.path.parent().map(PathBuf::from);
    self
  }

  /// Returns trusted backend projection metadata without transferring ownership.
  pub const fn endpoint(&self) -> &FactoryToolChannel {
    &self.endpoint
  }

  /// Stops the listener and current exchange, then removes its owned endpoint.
  pub async fn shutdown(mut self) -> Result<(), ExecutionError> {
    self.cancellation.cancel();
    if let Some(task) = self.task.take() {
      task.abort();
      if task.await.is_err_and(|error| !error.is_cancelled()) {
        return Err(unavailable("private reader task failed during cleanup"));
      }
    }
    self.socket.remove()
  }
}

impl Drop for ProtectedReadChannel {
  fn drop(&mut self) {
    self.cancellation.cancel();
    if let Some(task) = &self.task {
      task.abort();
    }
  }
}

struct OwnedSocket {
  path: PathBuf,
  identity: (u64, u64),
  owned_parent: Option<PathBuf>,
  removed: bool,
}

impl OwnedSocket {
  fn remove(&mut self) -> Result<(), ExecutionError> {
    if self.removed {
      return Ok(());
    }
    match std::fs::symlink_metadata(&self.path) {
      Ok(metadata) if (metadata.dev(), metadata.ino()) == self.identity => {
        std::fs::remove_file(&self.path).map_err(|_| unavailable("private reader endpoint cleanup failed"))?;
      }
      Ok(_) => return Err(unavailable("private reader endpoint identity changed")),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
      Err(_) => return Err(unavailable("private reader endpoint cleanup failed")),
    }
    if let Some(parent) = &self.owned_parent {
      std::fs::remove_dir(parent).map_err(|_| unavailable("private reader directory cleanup failed"))?;
    }
    self.removed = true;
    Ok(())
  }
}

impl Drop for OwnedSocket {
  fn drop(&mut self) {
    let _ = self.remove();
  }
}

fn invalid_endpoint() -> ExecutionError {
  ExecutionError::Invalid("private reader requires a canonical private owned endpoint".to_owned())
}

fn unavailable(message: &str) -> ExecutionError {
  ExecutionError::Unavailable(message.to_owned())
}
