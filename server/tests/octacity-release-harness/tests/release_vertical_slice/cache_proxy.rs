//! TLS facade used to exercise the released Agent's remote-cache trust path.

use std::{
  net::SocketAddr,
  path::{Path, PathBuf},
  sync::Arc,
};

use rustls::pki_types::PrivatePkcs8KeyDer;
use tokio::{
  io::copy_bidirectional,
  net::{TcpListener, TcpStream},
  task::{JoinHandle, JoinSet},
};
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;

use super::private_file;

pub(super) struct TlsCacheProxy {
  pub(super) origin: String,
  pub(super) ca_certificate: PathBuf,
  stop: CancellationToken,
  task: JoinHandle<()>,
}

impl TlsCacheProxy {
  pub(super) async fn start(upstream: SocketAddr, directory: &Path, advertised_host: &str) -> Self {
    let certified = rcgen::generate_simple_self_signed([advertised_host.to_owned()]).unwrap();
    let ca_certificate = private_file(directory, "cache-ca.pem", &certified.cert.pem());
    let key = PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
      .with_safe_default_protocol_versions()
      .unwrap()
      .with_no_client_auth()
      .with_single_cert(vec![certified.cert.der().clone()], key.into())
      .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let stop = CancellationToken::new();
    let task_stop = stop.clone();
    let task = tokio::spawn(async move {
      let mut connections = JoinSet::new();
      loop {
        let accepted = tokio::select! {
          () = task_stop.cancelled() => break,
          accepted = listener.accept() => accepted,
          Some(_) = connections.join_next(), if !connections.is_empty() => continue,
        };
        let Ok((socket, _)) = accepted else { break };
        let acceptor = acceptor.clone();
        connections.spawn(async move {
          let Ok(mut client) = acceptor.accept(socket).await else {
            return;
          };
          let Ok(mut server) = TcpStream::connect(upstream).await else {
            return;
          };
          let _ = copy_bidirectional(&mut client, &mut server).await;
        });
      }
      connections.abort_all();
      while connections.join_next().await.is_some() {}
    });
    Self {
      origin: advertised_origin(advertised_host, address.port()),
      ca_certificate,
      stop,
      task,
    }
  }

  pub(super) async fn shutdown(self) {
    self.stop.cancel();
    self.task.await.unwrap();
  }
}

fn advertised_origin(host: &str, port: u16) -> String {
  format!("https://{host}:{port}")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn cache_proxy_supports_tls_with_its_declared_ca() {
    let directory = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = tokio::spawn(async move {
      let app = axum::Router::new().route("/probe", axum::routing::get(|| async { "cache-contract" }));
      axum::serve(listener, app).await.unwrap();
    });
    let proxy = TlsCacheProxy::start(address, directory.path(), "127.0.0.1").await;
    let url = format!("{}/probe", proxy.origin);
    let certificate = reqwest::Certificate::from_pem(&std::fs::read(&proxy.ca_certificate).unwrap()).unwrap();
    let trusted = reqwest::Client::builder()
      .no_proxy()
      .add_root_certificate(certificate)
      .build()
      .unwrap();
    let response = trusted.get(&url).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(response.text().await.unwrap(), "cache-contract");
    let untrusted = reqwest::Client::builder().no_proxy().build().unwrap();
    assert!(untrusted.get(&url).send().await.is_err());
    proxy.shutdown().await;
    upstream.abort();
    let _ = upstream.await;
  }

  #[test]
  fn advertised_origin_preserves_the_guest_visible_host() {
    assert_eq!(
      advertised_origin("host.microsandbox.internal", 8443),
      "https://host.microsandbox.internal:8443"
    );
  }
}
