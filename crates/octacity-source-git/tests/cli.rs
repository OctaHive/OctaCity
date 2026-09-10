//! Black-box checks for the Git source-plugin process interface.

use std::{
  io::Write as _,
  process::{Command, Stdio},
};

#[test]
fn reports_protocol_capabilities() {
  let output = Command::new(env!("CARGO_BIN_EXE_octacity-source-git"))
    .arg("capabilities")
    .output()
    .unwrap();

  assert!(output.status.success());
  assert_eq!(
    serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
    serde_json::json!({
      "type": "hello",
      "protocol_version": 1,
      "plugin_name": "git",
      "plugin_version": "0.1.0"
    })
  );
  assert!(output.stderr.is_empty());
}

#[test]
fn rejects_an_unknown_command() {
  let output = Command::new(env!("CARGO_BIN_EXE_octacity-source-git"))
    .arg("clone")
    .output()
    .unwrap();

  assert!(!output.status.success());
  assert!(output.stderr.is_empty());
  let message: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(message["type"], "error");
  assert!(
    message["message"]
      .as_str()
      .unwrap()
      .contains("usage: octacity-source-git")
  );
}

#[test]
fn rejects_cancel_as_the_first_protocol_command_without_hanging() {
  let mut child = Command::new(env!("CARGO_BIN_EXE_octacity-source-git"))
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  child
    .stdin
    .take()
    .unwrap()
    .write_all(b"{\"type\":\"cancel\",\"request_id\":\"request-1\"}\n")
    .unwrap();
  let output = child.wait_with_output().unwrap();

  assert!(!output.status.success());
  let frames = String::from_utf8(output.stdout).unwrap();
  assert!(frames.contains("\"type\":\"hello\""));
  assert!(frames.contains("the first source-plugin command must be materialize"));
}
