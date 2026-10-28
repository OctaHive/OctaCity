use octacity_github::GithubWorkProvider;
use octacity_work_source_protocol::{Request, decode_request};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{fs, os::unix::fs::PermissionsExt as _, path::Path};
#[path = "reporting.rs"]
#[allow(dead_code)]
pub mod reporting;

#[path = "https_peer.rs"]
mod https_peer;
pub use https_peer::{Forge, Reply};

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

/// Exercise the real provider protocol without coupling its tests to host executable hashing.
pub async fn source_process(installation: &Installation, request: &Request) -> tokio::process::Child {
  use tokio::io::AsyncWriteExt as _;
  let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_octacity-github"))
    .current_dir(installation.root.path())
    .env_clear()
    .kill_on_drop(true)
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .unwrap();
  let mut bytes = serde_json::to_vec(request).unwrap();
  bytes.push(b'\n');
  child.stdin.as_mut().unwrap().write_all(&bytes).await.unwrap();
  child
}
