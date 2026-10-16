use std::sync::{Arc, Mutex};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpListener,
  task::JoinHandle,
};
use tokio_rustls::{
  TlsAcceptor,
  rustls::{self, pki_types::PrivatePkcs8KeyDer},
};

pub const KEY: &str = "sk-proj-openai-https-private-canary";

pub struct Fixture {
  pub endpoint: String,
  pub root: String,
  pub captured: Arc<Mutex<Vec<(bool, serde_json::Value)>>>,
  task: JoinHandle<()>,
}

impl Fixture {
  pub async fn start(response: serde_json::Value) -> Self {
    Self::reply(Some(response), 200, 0).await
  }
  pub async fn reply(response: Option<serde_json::Value>, status: u16, delay_millis: u64) -> Self {
    let mut ca = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    ca.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    // Distinguish the issuer from the leaf: identical default names make
    // native chain builders classify the leaf as self-signed.
    ca.distinguished_name
      .push(rcgen::DnType::CommonName, "OctaCity OpenAI fixture root");
    ca.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign];
    let issuer = rcgen::CertifiedIssuer::self_signed(ca, rcgen::KeyPair::generate().unwrap()).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    // Match the bound address directly. A dual-stack localhost lookup can
    // otherwise consume the failure test's deadline on IPv6-to-IPv4 fallback.
    let leaf = rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned()])
      .unwrap()
      .signed_by(&key, &issuer)
      .unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
      .with_safe_default_protocol_versions()
      .unwrap()
      .with_no_client_auth()
      .with_single_cert(
        vec![leaf.der().clone(), issuer.der().clone()],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
      )
      .unwrap();
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let endpoint = format!(
      "https://127.0.0.1:{}/v1/decisions",
      listener.local_addr().unwrap().port()
    );
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let records = captured.clone();
    let task = tokio::spawn(async move {
      loop {
        let (socket, _) = listener.accept().await.unwrap();
        let Ok(mut stream) = acceptor.accept(socket).await else {
          continue;
        };
        let mut bytes = Vec::new();
        let (head, length) = loop {
          let mut buffer = [0u8; 4096];
          let n = stream.read(&mut buffer).await.unwrap();
          if n == 0 {
            return;
          }
          bytes.extend_from_slice(&buffer[..n]);
          assert!(bytes.len() <= 65536);
          if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..i]);
            let length: usize = header
              .lines()
              .find_map(|l| {
                l.to_ascii_lowercase()
                  .strip_prefix("content-length: ")
                  .map(str::to_owned)
              })
              .unwrap()
              .parse()
              .unwrap();
            break (i + 4, length);
          }
        };
        while bytes.len() < head + length {
          let mut buffer = [0u8; 4096];
          let n = stream.read(&mut buffer).await.unwrap();
          assert!(n > 0 && bytes.len() + n <= 65536);
          bytes.extend_from_slice(&buffer[..n]);
        }
        let headers = String::from_utf8_lossy(&bytes[..head]);
        assert!(headers.starts_with("POST /v1/decisions HTTP/1.1\r\n"));
        let matched = headers.lines().any(|l| l == format!("authorization: Bearer {KEY}"));
        records
          .lock()
          .unwrap()
          .push((matched, serde_json::from_slice(&bytes[head..head + length]).unwrap()));
        let Some(response) = &response else {
          continue;
        };
        tokio::time::sleep(std::time::Duration::from_millis(delay_millis)).await;
        let body = serde_json::to_vec(response).unwrap();
        let header = format!(
          "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
          body.len()
        );
        stream.write_all(header.as_bytes()).await.unwrap();
        stream.write_all(&body).await.unwrap();
        let _ = stream.shutdown().await;
      }
    });
    Self {
      endpoint,
      root: issuer.pem(),
      captured,
      task,
    }
  }
  pub async fn stop(self) {
    self.task.abort();
    let _ = self.task.await;
  }
}
