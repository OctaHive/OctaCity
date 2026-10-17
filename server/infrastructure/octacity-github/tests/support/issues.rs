use octacity_github::GithubWorkProvider;
use octacity_work_source_protocol::{Request, decode_request};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{collections::VecDeque, fs, os::unix::fs::PermissionsExt as _, path::Path, sync::Arc, time::Duration};
use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  sync::{Mutex, Notify},
};
use tokio_rustls::{
  TlsAcceptor,
  rustls::{self, pki_types::PrivatePkcs8KeyDer},
};
#[path = "reporting.rs"]
#[allow(dead_code)]
pub mod reporting;

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
pub struct Forge {
  pub endpoint: String,
  pub certificate: String,
  pub requests: Arc<Mutex<Vec<String>>>,
  pub replies: Arc<Mutex<VecDeque<Reply>>>,
  pub repository: Arc<Mutex<Value>>,
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
        let reply = if repository {
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

pub struct Installation {
  pub root: tempfile::TempDir,
  pub selection: Vec<u8>,
}
impl Installation {
  pub fn new(forge: &Forge, selector: Value) -> Self {
    let root = tempfile::tempdir().unwrap();
    for path in [
      root.path().to_owned(),
      root.path().join("credentials"),
      root.path().join("selections"),
    ] {
      fs::create_dir_all(&path).unwrap();
      fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let config = format!(
      "config_version = 1\napi_base = {:?}\napi_version = \"fixture-v1\"\napi_root_certificates_pem = {:?}\ncredential_directory = \"credentials\"\nselection_directory = \"selections\"\n\
      [capabilities]\nsource_repositories = [\"installed-ticket-space\"]\n\
      [repositories.\"installed-ticket-space\"]\nrepository_id = \"1234\"\nowner = \"owner\"\nname = \"tickets\"\n",
      forge.endpoint,
      vec![forge.certificate.clone()]
    );
    private_file(&root.path().join("github-work.toml"), config.as_bytes());
    private_file(
      &root.path().join("credentials").join(hashed("private-source-grant")),
      br#"{"class":"work_source_read","token":"private-fixture-token","repositories":["installed-ticket-space"]}"#,
    );
    let selection = serde_json::to_vec(&selector).unwrap();
    private_file(
      &root
        .path()
        .join("selections")
        .join(hashed("arbitrary-installed-selector")),
      &selection,
    );
    Self { root, selection }
  }
  pub fn provider(&self) -> GithubWorkProvider {
    GithubWorkProvider::load(self.root.path()).unwrap()
  }
  pub fn request(&self) -> Request {
    let mut wire: Value = serde_json::from_slice(include_bytes!(
      "../../../../protocols/octacity-work-source-protocol/fixtures/scan-v1.json"
    ))
    .unwrap();
    let intent = &mut wire["message"]["command"]["payload"];
    intent["configuration_digest"] = json!(self.provider().configuration_digest());
    intent["resource"]["resource_id"] = json!("1234");
    intent["selection"]["sha256"] = json!(hex::encode(Sha256::digest(&self.selection)));
    intent["network_hosts"] = json!(["localhost", "127.0.0.1"]);
    intent["window"] = Value::Null;
    decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap()
  }
}
pub fn hashed(handle: &str) -> String {
  format!("{}.json", hex::encode(Sha256::digest(handle)))
}
pub fn private_file(path: &Path, bytes: &[u8]) {
  fs::write(path, bytes).unwrap();
  fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
pub fn selector() -> Value {
  json!({"state":"open","all_of":[],"any_of":[],"none_of":[]})
}
pub fn issue(id: u64, number: u64) -> Value {
  json!({"id":id,"number":number,"title":"Private task title","body":"Task\nAcceptance: explicit criteria",
    "updated_at":"2026-10-16T08:00:00Z","state":"open","labels":[],
    "comments_url":"https://forbidden.invalid/do-not-fetch", "unknown_future_field":true})
}
