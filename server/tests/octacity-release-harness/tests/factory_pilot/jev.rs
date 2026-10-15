//! Deterministic JEV wire fixture, used only at the installed HTTPS adapter boundary.
use serde_json::{Value, json};
use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
  time::Duration,
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpListener,
  sync::oneshot,
  task::{JoinHandle, JoinSet},
};
use tokio_rustls::{
  TlsAcceptor,
  rustls::{self, pki_types::PrivatePkcs8KeyDer},
};

const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy)]
pub enum JevResponseMode {
  Success,
  DropAfterAccept,
}

#[derive(Clone, Debug)]
pub struct CapturedJevRequest {
  pub method: String,
  pub path: String,
  pub idempotency_key: Option<String>,
  pub credential_matched: bool,
  pub body: Value,
}

pub struct JevHttpsFixture {
  address: std::net::SocketAddr,
  root_pem: String,
  captured: Arc<Mutex<Vec<CapturedJevRequest>>>,
  stop: Option<oneshot::Sender<()>>,
  task: Option<JoinHandle<()>>,
}
impl JevHttpsFixture {
  pub async fn start(model_version: &str, expected_api_key: &str) -> Self {
    Self::start_with_mode(model_version, expected_api_key, JevResponseMode::Success).await
  }
  pub async fn start_with_mode(model_version: &str, expected_api_key: &str, mode: JevResponseMode) -> Self {
    let mut ca = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    ca.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca.distinguished_name
      .push(rcgen::DnType::CommonName, "OctaCity JEV fixture root");
    ca.not_before = (std::time::SystemTime::now() - Duration::from_secs(3600)).into();
    ca.not_after = (std::time::SystemTime::now() + Duration::from_secs(172800)).into();
    ca.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign, rcgen::KeyUsagePurpose::CrlSign];
    let issuer = rcgen::CertifiedIssuer::self_signed(ca, rcgen::KeyPair::generate().unwrap()).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let mut leaf = rcgen::CertificateParams::new(vec!["localhost".to_owned()]).unwrap();
    leaf.distinguished_name.push(rcgen::DnType::CommonName, "localhost");
    leaf.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    leaf.not_before = (std::time::SystemTime::now() - Duration::from_secs(3600)).into();
    leaf.not_after = (std::time::SystemTime::now() + Duration::from_secs(86400)).into();
    let leaf = leaf.signed_by(&key, &issuer).unwrap();
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
    let address = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let records = captured.clone();
    let model_version = model_version.to_owned();
    let authorization = format!("Bearer {expected_api_key}");
    let (stop, mut stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
      let mut connections = JoinSet::new();
      loop {
        tokio::select! {
          _ = &mut stopped => break,
          Some(completed) = connections.join_next() => { completed.unwrap(); }
          accepted = listener.accept() => {
            let (socket, _) = accepted.unwrap();
            let acceptor = acceptor.clone();
            let records = records.clone();
            let model_version = model_version.clone();
            let authorization = authorization.clone();
            connections.spawn(async move {
              // Rejected trust and hostname checks never become HTTP requests.
              let Ok(Ok(mut tls)) = tokio::time::timeout(Duration::from_secs(3), acceptor.accept(socket)).await else { return; };
              let Ok(Some(request)) = tokio::time::timeout(Duration::from_secs(3), read_request(&mut tls, &authorization)).await else { return; };
              let index = {
                let mut captured = records.lock().unwrap();
                captured.push(request.clone());
                captured.len()
              };
              let (status, body) = if !request.credential_matched {
                (401, json!({"error":"fixture credential invalid"}))
              } else if request.method != "POST" || request.path != "/api/v1/systemone/" {
                (404, json!({"error":"fixture route missing"}))
              } else {
                if matches!(mode, JevResponseMode::DropAfterAccept) {
                  // Acceptance is recorded before the connection disappears.
                  // No status lookup is invented for a lost opaque JEV id.
                  let _ = tls.shutdown().await;
                  return;
                }
                (200, response(&request.body, &model_version, index))
              };
              let bytes = serde_json::to_vec(&body).unwrap();
              let header = format!("HTTP/1.1 {status} fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len());
              if tls.write_all(header.as_bytes()).await.is_ok() { let _ = tls.write_all(&bytes).await; }
              let _ = tls.shutdown().await;
            });
          }
        }
      }
      connections.abort_all();
      while connections.join_next().await.is_some() {}
    });
    Self {
      address,
      root_pem: issuer.pem(),
      captured,
      stop: Some(stop),
      task: Some(task),
    }
  }
  pub fn endpoint(&self) -> String {
    format!("https://localhost:{}/api/v1/systemone/", self.address.port())
  }
  pub fn root_certificate_pem(&self) -> &str {
    &self.root_pem
  }
  pub fn connector_configuration(&self, capability: &impl serde::Serialize) -> Value {
    json!({
      "version":1,"endpoint":self.endpoint(),"capability":capability,
      "credit_cost_micro_units":7,"timeout_milliseconds":5000,
      "root_certificates_pem":[self.root_certificate_pem()],
    })
  }
  pub fn captured(&self) -> Vec<CapturedJevRequest> {
    self.captured.lock().unwrap().clone()
  }
  pub async fn shutdown(mut self) {
    let _ = self.stop.take().unwrap().send(());
    let mut task = self.task.take().unwrap();
    match tokio::time::timeout(Duration::from_secs(5), &mut task).await {
      Ok(result) => result.unwrap(),
      Err(_) => {
        task.abort();
        let _ = task.await;
        panic!("JEV fixture must stop all owned connections within its cleanup bound");
      }
    }
  }
}
impl Drop for JevHttpsFixture {
  fn drop(&mut self) {
    if let Some(stop) = self.stop.take() {
      let _ = stop.send(());
    }
    if let Some(task) = self.task.take() {
      task.abort();
    }
  }
}

async fn read_request<S: tokio::io::AsyncRead + Unpin>(
  stream: &mut S,
  authorization: &str,
) -> Option<CapturedJevRequest> {
  let mut bytes = Vec::new();
  let header_end = loop {
    if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
      break index + 4;
    }
    if bytes.len() > MAX_HEADER_BYTES {
      return None;
    }
    let mut part = [0_u8; 4096];
    let count = stream.read(&mut part).await.ok()?;
    if count == 0 {
      return None;
    }
    bytes.extend_from_slice(&part[..count]);
  };
  if header_end > MAX_HEADER_BYTES {
    return None;
  }
  let header = std::str::from_utf8(&bytes[..header_end]).ok()?;
  let mut lines = header.split("\r\n");
  let mut route = lines.next()?.split_whitespace();
  let method = route.next()?.to_owned();
  let path = route.next()?.to_owned();
  let fields = lines
    .filter_map(|line| line.split_once(':'))
    .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
    .collect::<BTreeMap<_, _>>();
  let length = fields
    .get("content-length")
    .map_or(Some(0), |value| value.parse::<usize>().ok())?;
  if length > MAX_REQUEST_BYTES {
    return None;
  }
  while bytes.len() < header_end + length {
    let mut part = [0_u8; 4096];
    let count = stream.read(&mut part).await.ok()?;
    if count == 0 {
      return None;
    }
    bytes.extend_from_slice(&part[..count]);
  }
  Some(CapturedJevRequest {
    method,
    path,
    idempotency_key: fields.get("idempotency-key").cloned(),
    credential_matched: fields.get("authorization").is_some_and(|value| value == authorization),
    body: if length == 0 {
      Value::Null
    } else {
      serde_json::from_slice(&bytes[header_end..header_end + length]).ok()?
    },
  })
}
fn response(request: &Value, model_version: &str, index: usize) -> Value {
  let answers = request["questions"]
    .as_object()
    .unwrap()
    .iter()
    .map(|(name, question)| {
      assert_eq!(question["type"], "choice");
      let choices = question["criteria"].as_object().unwrap();
      let selected = choices.keys().next_back().unwrap();
      let probabilities = choices
        .keys()
        .map(|key| (key.clone(), if key == selected { json!(1.0) } else { json!(0.0) }))
        .collect::<serde_json::Map<_, _>>();
      (
        name.clone(),
        json!({"type":"choice","choice":selected,"probabilities":probabilities}),
      )
    })
    .collect::<serde_json::Map<_, _>>();
  json!({"id":format!("dec_fixture{index}"),"model":request["model"],"model_version":model_version,"provider":"jev-ai.org","answers":answers,
    "usage":{"input_tokens":8,"output_tokens":2,"charged_tokens":8,"charged_credits":0,"wallet":"tokens"},"latency_ms":7})
}
