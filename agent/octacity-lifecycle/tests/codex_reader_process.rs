//! Actual adapter process: MCP transport never reads Source directly.
#![cfg(unix)]
use serde_json::{Value, json};
use std::{
  io::{BufRead as _, Read as _, Write as _},
  os::unix::net::UnixListener,
  process::{Command, Stdio},
};

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, UnixListener) {
  let directory = tempfile::tempdir().unwrap();
  let socket = directory.path().join("reader.sock");
  let listener = UnixListener::bind(&socket).unwrap();
  let identity = |name| json!({"identity":name,"version":"1","sha256":"ab".repeat(32)});
  let recipe = json!({"tool":identity("operator-read"),"executable":identity("installed-reader"),
    "arguments":[],"paths":[],"network_hosts":[],"secret_profiles":[],"workload_identity_profiles":[],
    "descendants":1,"resources":{"cpu_millis":100,"memory_bytes":4096,"disk_bytes":4096,"process_count":2,"elapsed_millis":1000},
    "outputs":{"kinds":[],"max_artifact_count":0,"max_artifact_bytes":0,"max_report_count":0,"max_report_bytes":0}});
  let configuration = directory.path().join("reader.json");
  std::fs::write(
    &configuration,
    serde_json::to_vec(&json!({"socket":socket,"hook_tool_name":"mcp__qualified_fs__read_file","recipe":recipe}))
      .unwrap(),
  )
  .unwrap();
  (directory, configuration, listener)
}

#[test]
fn hook_then_mcp_serves_one_authorized_read_without_spending_bytes_in_the_hook() {
  let (_directory, configuration, listener) = fixture();
  let identity = |name| json!({"identity":name,"version":"1","sha256":"ab".repeat(32)});
  let server = std::thread::spawn(move || {
    // The channel permits one physical response, with no cached grant or
    // returned file bytes in the blocking authorization exchange.
    for (operation, response) in [("authorize", b"".as_slice()), ("read", b"facts".as_slice())] {
      let (mut stream, _) = listener.accept().unwrap();
      let mut length = [0; 4];
      stream.read_exact(&mut length).unwrap();
      let mut body = vec![0; u32::from_be_bytes(length) as usize];
      stream.read_exact(&mut body).unwrap();
      let request: Value = serde_json::from_slice(&body).unwrap();
      assert_eq!(request["schema_version"], 1);
      assert_eq!(request["operation"], operation);
      let proposal = &request["proposal"];
      assert_eq!(proposal["arguments"], json!(["/workspace/source/public/facts.txt"]));
      assert_eq!(
        proposal["paths"],
        json!([{"path":"/workspace/source/public/facts.txt","access":"read"}])
      );
      assert_eq!(proposal["tool"], identity("operator-read"));
      stream.write_all(&[0]).unwrap();
      stream.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
      stream.write_all(response).unwrap();
      let mut end = [0];
      let _ = stream.read(&mut end);
    }
  });
  let hook = invoke_hook(
    &configuration,
    json!({"hook_event_name":"PreToolUse","tool_name":"mcp__qualified_fs__read_file","tool_use_id":"call-1","tool_input":{"path":"/workspace/source/public/facts.txt"}}),
  );
  assert!(hook.status.success());
  assert!(hook.stderr.is_empty());
  assert_eq!(
    serde_json::from_slice::<Value>(&hook.stdout).unwrap(),
    json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}})
  );
  let mut child = Command::new(env!("CARGO_BIN_EXE_octacity-codex-reader"))
    .arg("mcp")
    .env_clear()
    .env("OCTACITY_CODEX_READER_CONFIG", configuration)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  let mut input = child.stdin.take().unwrap();
  for request in [
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"finite-client","version":"1"}}}),
    json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_file","arguments":{"path":"/workspace/source/public/facts.txt"}}}),
  ] {
    writeln!(input, "{request}").unwrap();
  }
  drop(input);
  let output = child.wait_with_output().unwrap();
  assert!(output.status.success());
  assert!(output.stderr.is_empty());
  let responses = output
    .stdout
    .as_slice()
    .lines()
    .map(|line| serde_json::from_str::<Value>(&line.unwrap()).unwrap())
    .collect::<Vec<_>>();
  assert_eq!(responses.len(), 3);
  assert_eq!(
    responses[0]["result"]["capabilities"],
    json!({"tools":{"listChanged":false}})
  );
  assert_eq!(responses[1]["result"]["tools"][0]["name"], "read_file");
  assert_eq!(responses[1]["result"]["tools"].as_array().unwrap().len(), 1);
  assert_eq!(
    responses[2],
    json!({"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"facts"}],"isError":false}})
  );
  assert!(!String::from_utf8_lossy(&output.stdout).contains("reader.sock"));
  server.join().unwrap();
}

fn invoke_hook(configuration: &std::path::Path, event: Value) -> std::process::Output {
  invoke_reader(configuration, &[], &event.to_string())
}

fn invoke_reader(configuration: &std::path::Path, arguments: &[&str], input: &str) -> std::process::Output {
  let mut child = Command::new(env!("CARGO_BIN_EXE_octacity-codex-reader"))
    .args(arguments)
    .env_clear()
    .env("OCTACITY_CODEX_READER_CONFIG", configuration)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
  child.wait_with_output().unwrap()
}

#[test]
fn a_successful_hook_cannot_bypass_denial_at_the_physical_read() {
  let (_directory, configuration, listener) = fixture();
  let server = std::thread::spawn(move || {
    for (operation, status) in [("authorize", 0), ("read", 1)] {
      let (mut peer, _) = listener.accept().unwrap();
      let mut length = [0; 4];
      peer.read_exact(&mut length).unwrap();
      let mut bytes = vec![0; u32::from_be_bytes(length) as usize];
      peer.read_exact(&mut bytes).unwrap();
      let request: Value = serde_json::from_slice(&bytes).unwrap();
      assert_eq!(request["operation"], operation);
      peer.write_all(&[status, 0, 0, 0, 0]).unwrap();
      let mut end = [0];
      let _ = peer.read(&mut end);
    }
  });
  let hook = invoke_hook(
    &configuration,
    json!({"hook_event_name":"PreToolUse","tool_name":"mcp__qualified_fs__read_file","tool_use_id":"call-1","tool_input":{"path":"/workspace/source/public/facts.txt"}}),
  );
  assert_eq!(
    serde_json::from_slice::<Value>(&hook.stdout).unwrap()["hookSpecificOutput"]["permissionDecision"],
    "allow"
  );
  let output = invoke_reader(
    &configuration,
    &["mcp"],
    &format!(
      "{}\n",
      json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"read_file","arguments":{"path":"/workspace/source/public/facts.txt"}}})
    ),
  );
  assert!(output.status.success());
  assert!(output.stderr.is_empty());
  assert_eq!(
    serde_json::from_slice::<Value>(&output.stdout).unwrap(),
    json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"Controlled read denied."}})
  );
  server.join().unwrap();
}

#[test]
fn actual_hook_denies_foreign_or_ambiguous_tools_before_any_channel_io() {
  let (_directory, configuration, listener) = fixture();
  listener.set_nonblocking(true).unwrap();
  for (name, input) in [
    ("Bash", json!({"command":"private-command-canary"})),
    (
      "mcp__foreign__read_file",
      json!({"path":"/workspace/source/private-path-canary"}),
    ),
    (
      "mcp__qualified_fs__read_file",
      json!({"path":"/workspace/source/private-path-canary","write":true}),
    ),
    (
      "mcp__qualified_fs__read_file",
      json!({"path":"/workspace/source/../protected/private-path-canary"}),
    ),
  ] {
    let output = invoke_hook(
      &configuration,
      json!({"hook_event_name":"PreToolUse","tool_name":name,"tool_use_id":"call-canary","tool_input":input}),
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
      serde_json::from_slice::<Value>(&output.stdout).unwrap(),
      json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Action denied by OctaCity policy."}})
    );
    assert!(matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("canary"));
  }
}

#[test]
fn actual_hook_requires_authorization_only_success_without_accepting_file_bytes() {
  let (_directory, configuration, listener) = fixture();
  let server = std::thread::spawn(move || {
    for (status, body) in [
      (0, b"".as_slice()),
      (1, b"".as_slice()),
      (0, b"private-read-canary!".as_slice()),
    ] {
      let (mut peer, _) = listener.accept().unwrap();
      let mut length = [0; 4];
      peer.read_exact(&mut length).unwrap();
      let mut bytes = vec![0; u32::from_be_bytes(length) as usize];
      peer.read_exact(&mut bytes).unwrap();
      let input: Value = serde_json::from_slice(&bytes).unwrap();
      assert_eq!(input["operation"], "authorize");
      assert_eq!(
        input["proposal"]["arguments"],
        json!(["/workspace/source/public/facts.txt"])
      );
      peer.write_all(&[status]).unwrap();
      peer.write_all(&(body.len() as u32).to_be_bytes()).unwrap();
      let _ = peer.write_all(body);
      let mut end = [0];
      let _ = peer.read(&mut end);
    }
  });
  for expected in ["allow", "deny", "deny"] {
    let output = invoke_hook(
      &configuration,
      json!({"hook_event_name":"PreToolUse","tool_name":"mcp__qualified_fs__read_file","tool_use_id":"call-1","tool_input":{"path":"/workspace/source/public/facts.txt"}}),
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
      serde_json::from_slice::<Value>(&output.stdout).unwrap()["hookSpecificOutput"]["permissionDecision"],
      expected
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-read-canary"));
  }
  server.join().unwrap();
}

#[test]
fn hook_authorization_may_finish_after_five_seconds_inside_the_official_window() {
  let (_directory, configuration, listener) = fixture();
  let server = std::thread::spawn(move || {
    let (mut peer, _) = listener.accept().unwrap();
    let mut length = [0; 4];
    peer.read_exact(&mut length).unwrap();
    let mut bytes = vec![0; u32::from_be_bytes(length) as usize];
    peer.read_exact(&mut bytes).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(6));
    let _ = peer.write_all(b"\0\0\0\0\0");
    let mut end = [0];
    let _ = peer.read(&mut end);
  });
  let output = invoke_hook(
    &configuration,
    json!({"hook_event_name":"PreToolUse","tool_name":"mcp__qualified_fs__read_file","tool_use_id":"call-1","tool_input":{"path":"/workspace/source/public/facts.txt"}}),
  );
  server.join().unwrap();
  assert!(output.status.success());
  assert!(output.stderr.is_empty());
  assert_eq!(
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["hookSpecificOutput"]["permissionDecision"],
    "allow"
  );
}
