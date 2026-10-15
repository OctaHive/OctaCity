//! Hermetic provider fixture: actual bare Git refs and atomically retained reviews.
//! Git transport itself remains the separately installed local-mirror adapter.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
  collections::BTreeMap,
  fs,
  io::Write as _,
  path::{Path, PathBuf},
  process::Command,
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  time::Duration,
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpListener,
  sync::{Notify, oneshot, watch},
  task::{JoinHandle, JoinSet},
};
use tokio_rustls::{
  TlsAcceptor,
  rustls::{self, pki_types::PrivatePkcs8KeyDer},
};

#[derive(Clone, Copy)]
pub enum ForgeResponseMode {
  Respond,
  DropReviewResponse,
  HoldReviewResponse,
}
struct State {
  repository: PathBuf,
  reviews: PathBuf,
  posts: AtomicUsize,
  versioned_requests: AtomicUsize,
  serial: Mutex<()>,
  accepted: Notify,
  held_observation: Notify,
  release: watch::Sender<bool>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Review {
  title: String,
  body: String,
  head: String,
  base: String,
  draft: bool,
}
struct Request {
  method: String,
  path: String,
  authorized: bool,
  exact_api_version: bool,
  body: Value,
}
enum Response {
  Reply(u16, Value),
  Close,
  Hold,
}

pub struct GithubForge {
  endpoint: String,
  root: String,
  state: Arc<State>,
  stop: Option<oneshot::Sender<()>>,
  task: Option<JoinHandle<()>>,
}
impl GithubForge {
  pub async fn start(directory: &Path, repository: &Path, mode: ForgeResponseMode) -> Self {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign, rcgen::KeyUsagePurpose::CrlSign];
    params.not_before = (std::time::SystemTime::now() - Duration::from_secs(3600)).into();
    params.not_after = (std::time::SystemTime::now() + Duration::from_secs(172800)).into();
    let issuer = rcgen::CertifiedIssuer::self_signed(params, rcgen::KeyPair::generate().unwrap()).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let mut params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    params.not_before = (std::time::SystemTime::now() - Duration::from_secs(3600)).into();
    params.not_after = (std::time::SystemTime::now() + Duration::from_secs(86400)).into();
    let leaf = params.signed_by(&key, &issuer).unwrap();
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
      .with_safe_default_protocol_versions()
      .unwrap()
      .with_no_client_auth()
      .with_single_cert(
        vec![leaf.der().clone(), issuer.der().clone()],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
      )
      .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(tls));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("https://localhost:{}/", listener.local_addr().unwrap().port());
    let state = Arc::new(State {
      repository: repository.canonicalize().unwrap(),
      reviews: directory.canonicalize().unwrap().join("review.json"),
      posts: AtomicUsize::new(0),
      versioned_requests: AtomicUsize::new(0),
      serial: Mutex::new(()),
      accepted: Notify::new(),
      held_observation: Notify::new(),
      release: watch::channel(false).0,
    });
    let shared = state.clone();
    let (stop, mut stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
      let mut connections = JoinSet::new();
      loop {
        tokio::select! {
          _ = &mut stopped => break,
          Some(completed) = connections.join_next() => { completed.unwrap(); }
          accepted = listener.accept() => {
            let (socket, _) = accepted.unwrap();
            if connections.len() >= 32 { continue; }
            let acceptor = acceptor.clone();
            let state = shared.clone();
            connections.spawn(async move {
              let Ok(Ok(mut socket)) = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(socket)).await else { return; };
              let Ok(Some(request)) = tokio::time::timeout(Duration::from_secs(10), read_request(&mut socket)).await else { return; };
              let response = state.respond(request, mode);
              if matches!(response, Response::Hold) {
                let mut release = state.release.subscribe();
                tokio::time::timeout(Duration::from_secs(60), async {
                  let released = *release.borrow_and_update();
                  if !released {
                    release.changed().await.unwrap();
                  }
                }).await.expect("owned accepted-response barrier was not released");
              }
              if let Response::Reply(status, value) = response {
                let body = serde_json::to_vec(&value).unwrap();
                let header = format!("HTTP/1.1 {status} fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = tokio::time::timeout(Duration::from_secs(10), async {
                  socket.write_all(header.as_bytes()).await?;
                  socket.write_all(&body).await
                }).await;
              }
              let _ = socket.shutdown().await;
            });
          }
        }
      }
      connections.abort_all();
      while connections.join_next().await.is_some() {}
    });
    Self {
      endpoint,
      root: issuer.pem(),
      state,
      stop: Some(stop),
      task: Some(task),
    }
  }
  pub fn endpoint(&self) -> &str {
    &self.endpoint
  }
  pub fn root_certificate_pem(&self) -> &str {
    &self.root
  }
  pub fn has_review(&self) -> bool {
    let _serial = self.state.serial.lock().unwrap();
    self.state.retained().is_some()
  }
  pub async fn wait_for_review(&self) {
    while !self.has_review() {
      self.state.accepted.notified().await;
    }
  }
  pub async fn wait_for_held_observation(&self) {
    self.state.held_observation.notified().await;
  }
  pub fn release_response(&self) {
    self.state.release.send_replace(true);
  }
  pub fn versioned_requests(&self) -> usize {
    self.state.versioned_requests.load(Ordering::SeqCst)
  }
  pub fn review_posts(&self) -> usize {
    self.state.posts.load(Ordering::SeqCst)
  }
  pub async fn stop(mut self) {
    let _ = self.stop.take().unwrap().send(());
    let mut task = self.task.take().unwrap();
    match tokio::time::timeout(Duration::from_secs(5), &mut task).await {
      Ok(result) => result.unwrap(),
      Err(_) => {
        task.abort();
        let _ = task.await;
        panic!("owned forge exceeded shutdown bound");
      }
    }
  }
}
impl Drop for GithubForge {
  fn drop(&mut self) {
    if let Some(stop) = self.stop.take() {
      let _ = stop.send(());
    }
    if let Some(task) = self.task.take() {
      task.abort();
    }
  }
}
impl State {
  fn revision(&self, branch: &str) -> Option<String> {
    if branch.len() > 256 || branch.is_empty() || branch.chars().any(char::is_control) {
      return None;
    }
    let reference = format!("refs/heads/{branch}");
    let git = |args: &[&str]| {
      Command::new("git")
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .arg("--git-dir")
        .arg(&self.repository)
        .args(args)
        .output()
        .ok()
    };
    if !git(&["check-ref-format", &reference])?.status.success() {
      return None;
    }
    let result = git(&["rev-parse", "--verify", &format!("{reference}^{{commit}}")])?;
    if !result.status.success() {
      return None;
    }
    let revision = String::from_utf8(result.stdout).ok()?.trim().to_owned();
    (revision.len() == 40 && revision.bytes().all(|ch| ch.is_ascii_hexdigit())).then_some(revision)
  }
  fn retained(&self) -> Option<Review> {
    match fs::metadata(&self.reviews) {
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
      Ok(meta) => {
        assert!(meta.is_file() && meta.len() <= 65536);
        Some(serde_json::from_slice(&fs::read(&self.reviews).unwrap()).unwrap())
      }
      Err(_) => panic!("test-owned review state unavailable"),
    }
  }
  fn view(&self, review: &Review) -> Option<Value> {
    Some(
      json!({"number":1,"state":"open","merged_at":null,"title":review.title,"body":review.body,"draft":review.draft,
      "head":{"sha":self.revision(&review.head)?,"ref":review.head,"repo":{"full_name":"owner/repository"}},
      "base":{"sha":self.revision(&review.base)?,"ref":review.base,"repo":{"full_name":"owner/repository"}}}),
    )
  }
  fn respond(&self, request: Request, mode: ForgeResponseMode) -> Response {
    let reply = |status, value| Response::Reply(status, value);
    if request.exact_api_version {
      self.versioned_requests.fetch_add(1, Ordering::SeqCst);
    }
    if !request.authorized {
      return reply(401, json!({"error":"fixture authorization rejected"}));
    }
    let url = url::Url::parse(&format!("https://fixture.invalid{}", request.path)).unwrap();
    let Some(route) = url.path().strip_prefix("/repos/owner/repository/") else {
      return reply(404, json!({}));
    };
    let _serial = self.serial.lock().unwrap();
    if request.method == "GET" {
      if let Some(branch) = route.strip_prefix("git/ref/heads/") {
        return self.revision(branch).map_or_else(
          || reply(404, json!({})),
          |sha| {
            reply(
              200,
              json!({"ref":format!("refs/heads/{branch}"),"object":{"type":"commit","sha":sha}}),
            )
          },
        );
      }
      if route == "pulls" {
        if matches!(mode, ForgeResponseMode::HoldReviewResponse) && !*self.release.borrow() && self.retained().is_some()
        {
          self.held_observation.notify_one();
          return Response::Hold;
        }
        let mut selected = Vec::new();
        if let Some(review) = self.retained() {
          let query: BTreeMap<_, _> = url.query_pairs().collect();
          if query
            .get("head")
            .is_none_or(|head| head == &format!("owner:{}", review.head))
            && query.get("base").is_none_or(|base| base == &review.base)
            && let Some(value) = self.view(&review)
          {
            selected.push(value);
          }
        }
        return reply(200, json!(selected));
      }
    }
    if request.method == "POST" && route == "pulls" {
      self.posts.fetch_add(1, Ordering::SeqCst);
      let Ok(review) = serde_json::from_value::<Review>(request.body) else {
        return reply(422, json!({}));
      };
      if self.retained().is_some() {
        return reply(409, json!({}));
      }
      let Some(view) = self.view(&review) else {
        return reply(422, json!({}));
      };
      let parent = self.reviews.parent().unwrap();
      let mut temporary = tempfile::NamedTempFile::new_in(parent).unwrap();
      serde_json::to_writer(&mut temporary, &review).unwrap();
      temporary.flush().unwrap();
      temporary.as_file().sync_all().unwrap();
      temporary.persist_noclobber(&self.reviews).unwrap();
      fs::File::open(parent).unwrap().sync_all().unwrap();
      self.accepted.notify_one();
      return match mode {
        ForgeResponseMode::Respond => reply(201, view),
        ForgeResponseMode::DropReviewResponse => Response::Close,
        ForgeResponseMode::HoldReviewResponse => Response::Hold,
      };
    }
    reply(404, json!({}))
  }
}
async fn read_request<S: tokio::io::AsyncRead + Unpin>(socket: &mut S) -> Option<Request> {
  let mut bytes = Vec::new();
  let header_end = loop {
    if let Some(index) = bytes.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
      break index + 4;
    }
    if bytes.len() > 16384 {
      return None;
    }
    let mut part = [0; 4096];
    let count = socket.read(&mut part).await.ok()?;
    if count == 0 {
      return None;
    }
    bytes.extend_from_slice(&part[..count]);
  };
  if header_end > 16384 {
    return None;
  }
  let header = std::str::from_utf8(&bytes[..header_end]).ok()?;
  let mut lines = header.split("\r\n");
  let mut route = lines.next()?.split_whitespace();
  let method = route.next()?.to_owned();
  let path = route.next()?.to_owned();
  let mut fields = BTreeMap::new();
  for line in lines.filter(|line| !line.is_empty()) {
    let (name, value) = line.split_once(':')?;
    if fields.insert(name.to_ascii_lowercase(), value.trim()).is_some() {
      return None;
    }
  }
  if fields.contains_key("transfer-encoding") {
    return None;
  }
  let length = fields
    .get("content-length")
    .map_or(Some(0), |value| value.parse::<usize>().ok())?;
  if length > 65536 {
    return None;
  }
  let authorized = fields.get("authorization") == Some(&"Bearer private-delivery-token");
  let exact_api_version = fields.get("x-github-api-version") == Some(&"2022-11-28");
  while bytes.len() < header_end + length {
    let mut part = [0; 4096];
    let count = socket.read(&mut part).await.ok()?;
    if count == 0 {
      return None;
    }
    bytes.extend_from_slice(&part[..count]);
  }
  Some(Request {
    method,
    path,
    authorized,
    exact_api_version,
    body: if length == 0 {
      Value::Null
    } else {
      serde_json::from_slice(&bytes[header_end..header_end + length]).ok()?
    },
  })
}
