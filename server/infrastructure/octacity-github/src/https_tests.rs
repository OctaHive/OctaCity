//! Operator-installed API trust over real TLS; no private transport mutation.
use super::*;
use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio_rustls::{
  TlsAcceptor,
  rustls::{self, pki_types::PrivatePkcs8KeyDer},
};

struct Forge {
  endpoint: String,
  root: String,
  requests: Arc<AtomicUsize>,
  task: tokio::task::JoinHandle<()>,
}
impl Forge {
  async fn start(intent: &DeliveryIntent) -> Self {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign, rcgen::KeyUsagePurpose::CrlSign];
    params.not_before = (std::time::SystemTime::now() - Duration::from_secs(3600)).into();
    params.not_after = (std::time::SystemTime::now() + Duration::from_secs(172800)).into();
    let issuer = rcgen::CertifiedIssuer::self_signed(params, rcgen::KeyPair::generate().unwrap()).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_owned()]).unwrap();
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    params.not_before = (std::time::SystemTime::now() - Duration::from_secs(3600)).into();
    params.not_after = (std::time::SystemTime::now() + Duration::from_secs(86400)).into();
    let leaf = params.signed_by(&key, &issuer).unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
      .with_safe_default_protocol_versions()
      .unwrap()
      .with_no_client_auth()
      .with_single_cert(
        vec![leaf.der().clone(), issuer.der().clone()],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
      )
      .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("https://localhost:{}/", listener.local_addr().unwrap().port());
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let requests = Arc::new(AtomicUsize::new(0));
    let captured = requests.clone();
    let intent = intent.clone();
    let task = tokio::spawn(async move {
      loop {
        let (socket, _) = listener.accept().await.unwrap();
        let Ok(Ok(mut socket)) = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(socket)).await else {
          continue;
        };
        let request = tokio::time::timeout(Duration::from_secs(10), async {
          let mut bytes = vec![];
          while !bytes.ends_with(b"\r\n\r\n") {
            if bytes.len() >= 16 * 1024 {
              return None;
            }
            let mut byte = [0];
            if socket.read_exact(&mut byte).await.is_err() {
              return None;
            }
            bytes.push(byte[0]);
          }
          String::from_utf8(bytes).ok()
        })
        .await;
        let Ok(Some(request)) = request else {
          continue;
        };
        assert!(request.starts_with("GET "), "observation must remain read-only");
        assert!(
          request
            .to_ascii_lowercase()
            .contains("authorization: bearer private-fixture-token")
        );
        captured.fetch_add(1, Ordering::SeqCst);
        let path = request.split_whitespace().nth(1).unwrap();
        let body = if path == "/repos/owner/repository/git/ref/heads/main" {
          serde_json::json!({"ref":"refs/heads/main","object":{"type":"commit","sha":intent.base_revision}})
        } else if path == "/repos/owner/repository/git/ref/heads/review/accepted-candidate" {
          serde_json::json!({"ref":intent.target.candidate_reference,"object":{"type":"commit","sha":intent.candidate_revision}})
        } else {
          assert!(path.starts_with("/repos/owner/repository/pulls?"));
          serde_json::json!([{"number":23,"state":"open","merged_at":null,"title":intent.review.title,
            "head":{"sha":intent.candidate_revision,"ref":"review/accepted-candidate","repo":{"full_name":"owner/repository"}},
            "base":{"sha":intent.base_revision,"ref":"main","repo":{"full_name":"owner/repository"}},
            "body":format!("{}\n\n{}",intent.review.summary,api::marker(&intent))}])
        };
        let body = serde_json::to_vec(&body).unwrap();
        socket
          .write_all(
            format!(
              "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
              body.len()
            )
            .as_bytes(),
          )
          .await
          .unwrap();
        socket.write_all(&body).await.unwrap();
        let _ = socket.shutdown().await;
      }
    });
    Self {
      endpoint,
      root: issuer.pem(),
      requests,
      task,
    }
  }
}
impl Drop for Forge {
  fn drop(&mut self) {
    self.task.abort();
  }
}

fn configure(root: &std::path::Path, endpoint: &str, roots: Vec<String>) -> Result<GithubPlugin, GithubConfigError> {
  let path = root.join("github-plugin.toml");
  let mut document: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
  document["api_base"] = toml::Value::String(endpoint.to_owned());
  document.as_table_mut().unwrap().insert(
    "api_root_certificates_pem".into(),
    toml::Value::Array(roots.into_iter().map(toml::Value::String).collect()),
  );
  write_private(&path, toml::to_string(&document).unwrap().as_bytes());
  GithubPlugin::load(root)
}

async fn observe(plugin: &GithubPlugin, mut intent: DeliveryIntent) -> Outcome {
  intent.configuration_digest = plugin.configuration_digest().to_owned();
  intent.network_hosts.push("localhost".to_owned());
  plugin
    .execute(
      &Request {
        protocol_version: 1,
        request_id: "private-api-trust".into(),
        command: Command::Observe(intent),
      },
      CancellationToken::new(),
    )
    .await
}

#[tokio::test]
async fn an_installed_private_api_root_observes_the_exact_review_over_https() {
  let (root, _, intent) = fixture();
  let forge = Forge::start(&intent).await;
  let plugin = configure(root.path(), &forge.endpoint, vec![forge.root.clone()])
    .expect("one operator-installed API root is a supported configuration");
  let outcome = observe(&plugin, intent).await;
  let Outcome::Observed(receipt) = outcome else {
    panic!("trusted HTTPS must observe the exact review: {outcome:?}");
  };
  assert_eq!(receipt.remote_head, "2".repeat(40));
  assert_eq!(receipt.review.unwrap().identity, "23");
  assert_eq!(forge.requests.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn untrusted_api_and_hostname_mismatch_fail_before_http() {
  let (root, _, intent) = fixture();
  let forge = Forge::start(&intent).await;
  let untrusted = configure(root.path(), &forge.endpoint, vec![]).unwrap();
  assert!(matches!(observe(&untrusted, intent.clone()).await, Outcome::Failure(_)));
  assert_eq!(forge.requests.load(Ordering::SeqCst), 0);
  let wrong_host = forge.endpoint.replace("localhost", "127.0.0.1");
  let wrong_host = configure(root.path(), &wrong_host, vec![forge.root.clone()]).unwrap();
  assert!(matches!(observe(&wrong_host, intent).await, Outcome::Failure(_)));
  assert_eq!(forge.requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn private_api_trust_is_bounded_and_part_of_the_frozen_provider_identity() {
  let (root, _, intent) = fixture();
  let forge = Forge::start(&intent).await;
  let original = configure(root.path(), &forge.endpoint, vec![]).unwrap();
  let trusted = configure(root.path(), &forge.endpoint, vec![forge.root.clone()]).unwrap();
  assert_ne!(original.configuration_digest(), trusted.configuration_digest());
  let mut stale = intent;
  stale.configuration_digest = original.configuration_digest().to_owned();
  assert!(matches!(
    trusted
      .execute(
        &Request {
          protocol_version: 1,
          request_id: "stale-api-trust".into(),
          command: Command::Observe(stale)
        },
        CancellationToken::new()
      )
      .await,
    Outcome::Failure(Failure {
      class: FailureClass::Conflict,
      ..
    })
  ));
  assert_eq!(forge.requests.load(Ordering::SeqCst), 0);
  for roots in [
    vec!["not a certificate".to_owned()],
    vec!["x".repeat(16 * 1024 + 1)],
    vec![forge.root.clone(); 9],
    vec![format!("{}{}", forge.root, forge.root)],
  ] {
    assert!(configure(root.path(), &forge.endpoint, roots).is_err());
  }
}
