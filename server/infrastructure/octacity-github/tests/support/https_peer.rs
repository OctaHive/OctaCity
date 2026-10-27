//! Shared hermetic HTTPS GitHub peer for adapter and packaged integration journeys.
//! The fixture is the provider boundary; callers still execute the real adapter.
use super::reporting;
use serde_json::{Value, json};
use std::{
  collections::{BTreeMap, VecDeque},
  sync::Arc,
  time::Duration,
};
use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  sync::{Mutex, Notify},
};
use tokio_rustls::{
  TlsAcceptor,
  rustls::{self, pki_types::PrivatePkcs8KeyDer},
};

pub struct Reply {
  pub status: u16,
  pub headers: Vec<(String, String)>,
  pub body: Vec<u8>,
  pub chunked: bool,
  pub stall: bool,
  pub disconnect: bool,
}
impl Reply {
  pub fn json(value: Value) -> Self {
    Self {
      status: 200,
      headers: vec![],
      body: serde_json::to_vec(&value).unwrap(),
      chunked: false,
      stall: false,
      disconnect: false,
    }
  }
}
pub struct NativeResource {
  pub metadata: Value,
  pub issues: Vec<Value>,
}
pub struct Forge {
  pub endpoint: String,
  pub certificate: String,
  pub requests: Arc<Mutex<Vec<String>>>,
  pub replies: Arc<Mutex<VecDeque<Reply>>>,
  pub repository: Arc<Mutex<Value>>,
  #[allow(dead_code)]
  pub resources: Arc<Mutex<BTreeMap<String, NativeResource>>>,
  // Connection diagnostics override metadata failures; listing fixtures use the default.
  #[allow(dead_code)]
  pub repository_reply: Arc<Mutex<Option<Reply>>>,
  pub changed: Arc<Notify>,
  #[allow(dead_code)]
  pub reporting: Arc<Mutex<Option<reporting::Issue>>>,
  task: tokio::task::JoinHandle<()>,
}
impl Forge {
  pub async fn start() -> Self {
    let now = std::time::SystemTime::now();
    let mut ca = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    ca.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign, rcgen::KeyUsagePurpose::CrlSign];
    ca.not_before = (now - Duration::from_secs(3600)).into();
    ca.not_after = (now + Duration::from_secs(172800)).into();
    let ca = rcgen::CertifiedIssuer::self_signed(ca, rcgen::KeyPair::generate().unwrap()).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let mut leaf = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
    leaf.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    leaf.not_before = (now - Duration::from_secs(3600)).into();
    leaf.not_after = (now + Duration::from_secs(86400)).into();
    let leaf = leaf.signed_by(&key, &ca).unwrap();
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
      .with_safe_default_protocol_versions()
      .unwrap()
      .with_no_client_auth()
      .with_single_cert(
        vec![leaf.der().clone(), ca.der().clone()],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
      )
      .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("https://localhost:{}/api/v3/", listener.local_addr().unwrap().port());
    let acceptor = TlsAcceptor::from(Arc::new(tls));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let replies = Arc::new(Mutex::new(VecDeque::<Reply>::new()));
    let repository = Arc::new(Mutex::new(json!({"id":1234})));
    let metadata = repository.clone();
    let resources = Arc::new(Mutex::new(BTreeMap::<String, NativeResource>::new()));
    let containers = resources.clone();
    let repository_reply = Arc::new(Mutex::new(None::<Reply>));
    let metadata_reply = repository_reply.clone();
    let changed = Arc::new(Notify::new());
    let reporting = Arc::new(Mutex::new(None::<reporting::Issue>));
    let effects = reporting.clone();
    let (captured, queued, signal) = (requests.clone(), replies.clone(), changed.clone());
    let task = tokio::spawn(async move {
      loop {
        let (stream, _) = listener.accept().await.unwrap();
        let Ok(Ok(mut stream)) = tokio::time::timeout(Duration::from_secs(3), acceptor.accept(stream)).await else {
          continue;
        };
        let request = tokio::time::timeout(Duration::from_secs(3), async {
          let mut bytes = Vec::new();
          while !bytes.ends_with(b"\r\n\r\n") {
            if bytes.len() >= 16 * 1024 {
              return None;
            }
            let mut byte = [0];
            stream.read_exact(&mut byte).await.ok()?;
            bytes.push(byte[0]);
          }
          let headers = String::from_utf8(bytes.clone()).ok()?;
          let length = headers
            .lines()
            .find_map(|line| {
              let (name, value) = line.split_once(':')?;
              name
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
            })
            .unwrap_or(0);
          if length > 64 * 1024 {
            return None;
          }
          let mut body = vec![0; length];
          stream.read_exact(&mut body).await.ok()?;
          bytes.extend(body);
          String::from_utf8(bytes).ok()
        })
        .await;
        let Ok(Some(request)) = request else { continue };
        let repository = request.starts_with("GET /api/v3/repos/owner/tickets HTTP/");
        captured.lock().await.push(request.clone());
        signal.notify_one();
        let additional = containers.lock().await.iter().find_map(|(locator, resource)| {
          let root = format!("/api/v3/repos/{locator}");
          if request.starts_with(&format!("GET {root} HTTP/")) {
            Some(Reply::json(resource.metadata.clone()))
          } else if request.starts_with(&format!("GET {root}/issues?")) {
            Some(Reply::json(json!(resource.issues)))
          } else {
            None
          }
        });
        let reply = if let Some(reply) = additional {
          reply
        } else if repository {
          let override_reply = metadata_reply.lock().await.take();
          match override_reply {
            Some(reply) => reply,
            None => Reply::json(metadata.lock().await.clone()),
          }
        } else if let Some(issue) = effects.lock().await.as_mut() {
          issue.respond(&request)
        } else {
          queued.lock().await.pop_front().unwrap_or_else(|| Reply {
            status: 404,
            ..Reply::json(json!({"message":"private-provider-error"}))
          })
        };
        if reply.disconnect {
          continue;
        }
        if reply.stall {
          std::future::pending::<()>().await;
        }
        let mut headers = format!(
          "HTTP/1.1 {} fixture\r\nContent-Type: application/json\r\nConnection: close\r\n",
          reply.status
        );
        let declared = reply
          .headers
          .iter()
          .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));
        for (name, value) in reply.headers {
          headers.push_str(&format!("{name}: {value}\r\n"));
        }
        let body = if reply.chunked {
          headers.push_str("Transfer-Encoding: chunked\r\n\r\n");
          let mut body = format!("{:x}\r\n", reply.body.len()).into_bytes();
          body.extend(reply.body);
          body.extend(b"\r\n0\r\n\r\n");
          body
        } else {
          if !declared {
            headers.push_str(&format!("Content-Length: {}\r\n", reply.body.len()));
          }
          headers.push_str("\r\n");
          reply.body
        };
        if stream.write_all(headers.as_bytes()).await.is_ok() {
          let _ = stream.write_all(&body).await;
        }
        let _ = stream.shutdown().await;
      }
    });
    Self {
      endpoint,
      certificate: ca.pem(),
      requests,
      replies,
      repository,
      resources,
      repository_reply,
      changed,
      reporting,
      task,
    }
  }
  pub async fn wait_for_requests(&self, count: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
      loop {
        let next = self.changed.notified();
        if self.requests.lock().await.len() >= count {
          return;
        }
        next.await;
      }
    })
    .await
    .expect("provider request reached the controlled HTTPS peer");
  }
}
impl Drop for Forge {
  fn drop(&mut self) {
    self.task.abort();
  }
}
