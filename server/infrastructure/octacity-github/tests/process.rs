use std::{
  io::Write as _,
  process::{Command, Stdio},
};

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
