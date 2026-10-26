//! Protected local proxy transport; HTTP headers never establish its identity.
use super::{ListenerTaskResult, ServerRuntimeError};
use crate::config::LocalIntegrationManagement;
use axum::{
  extract::{ConnectInfo, Request},
  middleware::{self, Next},
};
use octacity_server_api_rest::integrations::ProtectedIntegrationRequest;
use octacity_server_application::{
  ManagementActor, ManagementIngress, ManagementRequestAttributes, ManagementRequestContext, ManagementRequestId,
  ManagementSecurityScope,
};
use std::{
  os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
  path::PathBuf,
};
use tokio::{net::UnixListener, task::JoinSet};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct PeerUid(Option<u32>);
impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, UnixListener>> for PeerUid {
  fn connect_info(stream: axum::serve::IncomingStream<'_, UnixListener>) -> Self {
    Self(stream.io().peer_cred().ok().map(|peer| peer.uid()))
  }
}

pub(super) struct LocalListener {
  listener: UnixListener,
  config: LocalIntegrationManagement,
  owned: OwnedSocket,
}
struct OwnedSocket {
  path: PathBuf,
  device: u64,
  inode: u64,
}
impl Drop for OwnedSocket {
  fn drop(&mut self) {
    if let Ok(meta) = std::fs::symlink_metadata(&self.path)
      && meta.dev() == self.device
      && meta.ino() == self.inode
    {
      let _ = std::fs::remove_file(&self.path);
    }
  }
}
pub(super) async fn bind(config: &LocalIntegrationManagement) -> Result<LocalListener, ServerRuntimeError> {
  let create = async {
    let parent = config
      .socket_path
      .parent()
      .ok_or_else(|| std::io::Error::other("missing socket parent"))?;
    let meta = std::fs::symlink_metadata(parent)?;
    if !meta.is_dir() || meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o022 != 0 {
      return Err(std::io::Error::other(
        "socket directory must be owned and not writable by other identities",
      ));
    }
    if let Ok(meta) = std::fs::symlink_metadata(&config.socket_path) {
      if !meta.file_type().is_socket() || meta.uid() != rustix::process::geteuid().as_raw() {
        return Err(std::io::Error::other("existing management path is not an owned socket"));
      }
      match tokio::time::timeout(
        std::time::Duration::from_millis(config.operation_timeout_milliseconds),
        tokio::net::UnixStream::connect(&config.socket_path),
      )
      .await
      {
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
          let current = std::fs::symlink_metadata(&config.socket_path)?;
          if current.dev() != meta.dev() || current.ino() != meta.ino() {
            return Err(std::io::Error::other("management socket changed during recovery"));
          }
          std::fs::remove_file(&config.socket_path)?;
        }
        _ => {
          return Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "management socket is active or cannot be verified",
          ));
        }
      }
    }
    let listener = UnixListener::bind(&config.socket_path)?;
    let meta = std::fs::symlink_metadata(&config.socket_path)?;
    let owned = OwnedSocket {
      path: config.socket_path.clone(),
      device: meta.dev(),
      inode: meta.ino(),
    };
    // Only the server may alter the directory. Connection rights do not grant
    // management authority: each accepted stream must have the selected OS UID.
    std::fs::set_permissions(&config.socket_path, std::fs::Permissions::from_mode(0o666))?;
    Ok(LocalListener {
      listener,
      config: config.clone(),
      owned,
    })
  };
  create.await.map_err(|source| ServerRuntimeError::InspectListener {
    ingress: "local-integration-management",
    source,
  })
}
impl LocalListener {
  pub(super) fn spawn(
    self,
    tasks: &mut JoinSet<ListenerTaskResult>,
    router: axum::Router,
    cancellation: CancellationToken,
  ) {
    let config = self.config;
    let router = router.layer(middleware::from_fn(
      move |ConnectInfo(peer): ConnectInfo<PeerUid>, mut request: Request, next: Next| {
        let config = config.clone();
        async move {
          if peer.0 == Some(config.peer_uid) {
            let context = request_context(&config.principal);
            request
              .extensions_mut()
              .insert(ProtectedIntegrationRequest::new(context).expect("verified local transport"));
          }
          next.run(request).await
        }
      },
    ));
    tasks.spawn(async move {
      let _owned = self.owned;
      axum::serve(self.listener, router.into_make_service_with_connect_info::<PeerUid>())
        .with_graceful_shutdown(cancellation.cancelled_owned())
        .await
        .map_err(|source| ("local-integration-management", source))?;
      Ok("local-integration-management")
    });
  }
}

// The worker and protected ingress share the same accepted installation-owner partition.
pub(super) fn request_context(principal: &str) -> ManagementRequestContext {
  ManagementRequestContext::new(
    ManagementActor::authenticated(principal).expect("validated principal"),
    ManagementSecurityScope::new(format!(
      "local-proxy:{}",
      octacity_server_factory::FactoryDigest::content_sha256(principal.as_bytes())
    ))
    .expect("bounded principal"),
    ManagementRequestId::new(uuid::Uuid::new_v4()).expect("generated identity"),
    ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
  )
  .expect("verified local proxy")
}

#[cfg(test)]
mod tests {
  use super::*;
  #[tokio::test]
  async fn restart_reclaims_only_an_owned_inactive_socket_and_preserves_other_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("management.sock");
    let config: LocalIntegrationManagement = toml::from_str(&format!(
      "socket_path={:?}\npeer_uid=101\nprincipal='test-proxy'",
      path.to_str().unwrap()
    ))
    .unwrap();
    let stale = std::os::unix::net::UnixListener::bind(&path).unwrap();
    drop(stale);
    let listener = bind(&config)
      .await
      .expect("restart must recover its own inactive socket");
    assert!(bind(&config).await.is_err(), "a live listener must never be replaced");
    drop(listener);
    assert!(!path.exists(), "graceful ownership cleanup removes the socket");
    std::fs::write(&path, b"retained unrelated file").unwrap();
    assert!(bind(&config).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"retained unrelated file");
  }
}
