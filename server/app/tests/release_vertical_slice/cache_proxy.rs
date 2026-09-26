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
  pub(super) async fn start(upstream: SocketAddr, directory: &Path) -> Self {
    let certified = rcgen::generate_simple_self_signed(["127.0.0.1".to_owned()]).unwrap();
    let ca_certificate = private_file(directory, "cache-ca.pem", &certified.cert.pem());
    let key = PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let config = rustls::ServerConfig::builder()
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
      origin: format!("https://{address}"),
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
