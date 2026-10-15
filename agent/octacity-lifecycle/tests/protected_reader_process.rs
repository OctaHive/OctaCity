//! Worker wire behavior through its actual standalone process entrypoint.
//! These host tests do not qualify VM isolation or resource enforcement.
#![cfg(unix)]

use std::{
  io::Write as _,
  process::{Command, Stdio},
};

fn exchange(root: &std::path::Path, request: &[u8]) -> std::process::Output {
  let mut frame = Vec::from((request.len() as u32).to_be_bytes());
  frame.extend_from_slice(request);
  exchange_frame(root, &frame)
}

fn exchange_frame(root: &std::path::Path, frame: &[u8]) -> std::process::Output {
  let mut child = Command::new(env!("CARGO_BIN_EXE_octacity-protected-reader"))
    .current_dir(root)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  let mut input = child.stdin.take().unwrap();
  input.write_all(frame).unwrap();
  drop(input);
  child.wait_with_output().unwrap()
}

#[test]
fn malformed_worker_frames_return_only_a_bounded_refusal() {
  let directory = tempfile::tempdir().unwrap();
  for frame in [
    b"\0\0\0\0".as_slice(),
    b"\xff\xff\xff\xff".as_slice(),
    b"\0\0\0".as_slice(),
    b"\0\0\0\x05{}".as_slice(),
  ] {
    let result = exchange_frame(directory.path(), frame);
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    assert_eq!(result.stdout, b"\x01\0\0\0\0");
  }
}

#[test]
fn an_empty_worker_read_remains_distinct_from_refusal() {
  let directory = tempfile::tempdir().unwrap();
  std::fs::write(directory.path().join("empty.txt"), []).unwrap();
  let result = exchange(
    directory.path(),
    br#"{"schema_version":1,"relative_path":"empty.txt","max_bytes":1}"#,
  );
  assert!(result.status.success());
  assert!(result.stderr.is_empty());
  assert_eq!(result.stdout, b"\0\0\0\0\0");
}

#[test]
fn worker_instruction_debug_redacts_its_private_path() {
  let request = octacity_protocol::ProtectedReadWorkerRequestV1 {
    schema_version: 1,
    relative_path: "private-source-path-canary.txt".to_owned(),
    max_bytes: 5,
  };
  assert!(!format!("{request:?}").contains("private-source-path-canary"));
}

#[test]
fn installed_reader_process_reads_only_a_bounded_relative_file_from_its_trusted_root() {
  use std::os::unix::fs::PermissionsExt as _;
  let directory = tempfile::tempdir().unwrap();
  let source = directory.path().join("source");
  std::fs::create_dir(&source).unwrap();
  std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o700)).unwrap();
  std::fs::write(source.join("facts.txt"), b"facts").unwrap();
  std::fs::write(directory.path().join("outside-secret.txt"), b"outside-private-canary").unwrap();

  let result = exchange(
    &source,
    br#"{"schema_version":1,"relative_path":"facts.txt","max_bytes":5}"#,
  );
  assert!(result.status.success());
  assert!(result.stderr.is_empty());
  assert_eq!(result.stdout, b"\0\0\0\0\x05facts");

  for request in [
    br#"{"schema_version":1,"relative_path":"../outside-secret.txt","max_bytes":64}"#.as_slice(),
    br#"{"schema_version":1,"relative_path":"facts.txt","max_bytes":4}"#.as_slice(),
    br#"{"schema_version":1,"relative_path":"facts.txt","max_bytes":4194305}"#.as_slice(),
    br#"{"schema_version":1,"relative_path":"facts.txt","max_bytes":5,"source_root":".."}"#.as_slice(),
  ] {
    let result = exchange(&source, request);
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    assert_eq!(result.stdout, b"\x01\0\0\0\0", "refusals reveal no file bytes or paths");
  }
}
