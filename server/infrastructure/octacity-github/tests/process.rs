use std::{
  io::Write as _,
  process::{Command, Stdio},
};

fn work_exchange(frame: &serde_json::Value) -> std::process::Output {
  let root = tempfile::tempdir().unwrap();
  let mut child = Command::new(env!("CARGO_BIN_EXE_octacity-github"))
    .current_dir(root.path())
    .env_clear()
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  let mut bytes = serde_json::to_vec(frame).unwrap();
  bytes.push(b'\n');
  child.stdin.take().unwrap().write_all(&bytes).unwrap();
  child.wait_with_output().unwrap()
}

#[test]
fn the_same_provider_process_understands_independent_work_role_cancellations() {
  for (family, target, identity) in [
    ("source", "target_scan_id", "scan_id"),
    ("reporter", "target_projection_id", "projection_id"),
  ] {
    let frame = serde_json::json!({"family":family,"message":{"protocol_version":1,"request_id":"cancel-work",
      "command":{"operation":"cancel","payload":{target:"a".repeat(64)}}}});
    let output = work_exchange(&frame);
    assert!(
      output.status.success(),
      "the process must understand {family} independently of Delivery"
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
      response,
      serde_json::json!({"family":family,"message":{"protocol_version":1,"request_id":"cancel-work",
      "outcome":{"status":"acknowledged","payload":{identity:"a".repeat(64)}}}})
    );
    assert!(output.stderr.is_empty());
  }
}

#[test]
fn missing_work_installations_are_explicitly_unsupported_without_credentials_or_delivery_fallback() {
  for fixture in [
    include_bytes!("../../../protocols/octacity-work-source-protocol/fixtures/scan-v1.json").as_slice(),
    include_bytes!("../../../protocols/octacity-work-source-protocol/fixtures/project-v1.json").as_slice(),
  ] {
    let wire: serde_json::Value = serde_json::from_slice(fixture).unwrap();
    let request = octacity_work_source_protocol::decode_request(fixture).unwrap();
    let output = work_exchange(&wire);
    assert!(output.status.success());
    let response = octacity_work_source_protocol::decode_response(&output.stdout, &request).unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["message"]["outcome"]["status"], "failure");
    assert_eq!(response["message"]["outcome"]["payload"]["class"], "unsupported");
    assert!(output.stderr.is_empty());
    let mut substituted = wire;
    substituted["message"]["command"]["payload"]["resource"]["credential"]["class"] =
      serde_json::json!("delivery_write");
    let rejected = work_exchange(&substituted);
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    assert!(rejected.stderr.is_empty());
  }
}

#[test]
fn the_provider_process_correlates_a_bounded_delivery_cancellation_without_credentials() {
  let root = tempfile::tempdir().unwrap();
  let mut child = Command::new(env!("CARGO_BIN_EXE_octacity-github"))
    .current_dir(root.path())
    .env_clear()
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  let frame = serde_json::json!({"protocol_version":1,"request_id":"cancel-original",
    "command":{"operation":"cancel","payload":{"target_delivery_id":"a".repeat(64)}}});
  let mut bytes = serde_json::to_vec(&frame).unwrap();
  bytes.push(b'\n');
  child.stdin.take().unwrap().write_all(&bytes).unwrap();
  let output = child.wait_with_output().unwrap();
  assert!(output.status.success());
  let response: serde_json::Value =
    serde_json::from_slice(&output.stdout).expect("the process must produce its one correlated result");
  assert_eq!(
    response,
    serde_json::json!({"protocol_version":1,"request_id":"cancel-original",
    "outcome":{"status":"acknowledged","payload":{"delivery_id":"a".repeat(64)}}})
  );
  assert!(output.stderr.is_empty());
}
