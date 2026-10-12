#![cfg(unix)]
use octacity_delivery_protocol::{FailureClass, Outcome, decode_request};
use octacity_github::GithubPlugin;
use sha2::{Digest as _, Sha256};
use std::{fs, os::unix::fs::PermissionsExt as _};

#[tokio::test]
async fn a_read_only_credential_cannot_be_reclassified_as_delivery_authority() {
  let root = tempfile::tempdir().unwrap();
  fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
  for dir in ["credentials", "inputs"] {
    fs::create_dir(root.path().join(dir)).unwrap();
    fs::set_permissions(root.path().join(dir), fs::Permissions::from_mode(0o700)).unwrap();
  }
  let git = root.path().join("verified-git");
  let executable = b"#!/bin/sh\nexit 91\n";
  fs::write(&git, executable).unwrap();
  fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
  let mut config = format!(
    "git_path = {:?}\ngit_sha256 = {:?}\n",
    git.to_str().unwrap(),
    hex::encode(Sha256::digest(executable))
  );
  config.push_str(
    r#"config_version = 1
api_base = "https://forge.invalid/api/"
api_version = "2026-03-10"
credential_directory = "credentials"
input_directory = "inputs"
[repositories."repository:accepted-project"]
repository_id = "repository-1"
owner = "owner"
name = "repository"
remote = "https://forge.invalid/owner/repository.git"
"#,
  );
  fs::write(root.path().join("github-plugin.toml"), config).unwrap();
  fs::set_permissions(
    root.path().join("github-plugin.toml"),
    fs::Permissions::from_mode(0o600),
  )
  .unwrap();
  let handle = "protected-delivery-grant";
  let path = root
    .path()
    .join("credentials")
    .join(format!("{}.json", hex::encode(Sha256::digest(handle))));
  fs::write(
    &path,
    r#"{"class":"vcs_read","token":"never-disclose-token","repositories":["repository:accepted-project"]}"#,
  )
  .unwrap();
  fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
  let mut request = decode_request(include_bytes!(
    "../../../protocols/octacity-delivery-protocol/fixtures/publish-v1.json"
  ))
  .unwrap();
  let plugin = GithubPlugin::load(root.path()).unwrap();
  if let octacity_delivery_protocol::Command::Publish(intent) = &mut request.command {
    intent.configuration_digest = plugin.configuration_digest().to_owned();
  }
  let result = plugin
    .execute(&request, tokio_util::sync::CancellationToken::new())
    .await;
  assert!(matches!(result, Outcome::Failure(ref failure) if failure.class == FailureClass::Permanent));
  let rendered = format!("{result:?}");
  assert!(!rendered.contains("never-disclose-token"));
  assert!(!rendered.contains("forge.invalid"));
}
