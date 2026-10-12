#![cfg(unix)]
use octacity_delivery_protocol::{Command, Outcome, decode_request};
use octacity_server_delivery::{DeliveryAdapterRegistry, HostFailureClass};
use sha2::{Digest as _, Sha256};
use std::{fs, os::unix::fs::PermissionsExt as _, time::Duration};
use tokio_util::sync::CancellationToken;

fn installed(script: &str) -> (tempfile::TempDir, String) {
  let root = tempfile::tempdir().unwrap();
  let dir = root.path().join("provider-plugin");
  fs::create_dir(&dir).unwrap();
  let executable = dir.join("plugin");
  fs::write(&executable, script).unwrap();
  fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
  let digest = hex::encode(Sha256::digest(script.as_bytes()));
  // The common registry accepts TOML installation metadata, not caller code.
  let toml = format!(
    "manifest_version = 1\nadapter_id = \"provider-plugin\"\nexecutable = \"plugin\"\nexecutable_sha256 = {digest:?}\n[protocol]\nmin = 1\nmax = 1\n[capabilities]\nobserve = true\npublish = true\ncancel = true\nidempotency = \"observe_before_retry\"\ncredential_class = \"delivery_write\"\nconfiguration_digest = {:?}\n",
    "e".repeat(64)
  );
  fs::write(dir.join("adapter.toml"), toml).unwrap();
  (root, digest)
}

#[tokio::test]
async fn verifies_the_profile_before_starting_the_process_and_correlates_observation() {
  let (root, digest) = installed(
    "#!/bin/sh\nIFS= read -r frame || exit 91\nprintf '%s\\n' '{\"protocol_version\":1,\"request_id\":\"request-1\",\"outcome\":{\"status\":\"absent\"}}'\n",
  );
  let registry = DeliveryAdapterRegistry::discover(root.path()).unwrap();
  let adapter = registry.resolve("provider-plugin", &digest).unwrap();
  let mut request = decode_request(include_bytes!(
    "../../../protocols/octacity-delivery-protocol/fixtures/publish-v1.json"
  ))
  .unwrap();
  let Command::Publish(intent) = request.command else {
    unreachable!()
  };
  request.command = Command::Observe(intent);
  assert_eq!(
    adapter
      .execute(
        request.clone(),
        Duration::from_secs(1),
        Duration::from_millis(50),
        CancellationToken::new()
      )
      .await
      .unwrap(),
    Outcome::Absent
  );
  let Command::Observe(intent) = &mut request.command else {
    unreachable!()
  };
  intent.configuration_digest = "f".repeat(64);
  assert_eq!(
    adapter
      .execute(
        request,
        Duration::from_secs(1),
        Duration::from_millis(50),
        CancellationToken::new()
      )
      .await
      .unwrap_err()
      .class(),
    HostFailureClass::InvalidRequest
  );
}
