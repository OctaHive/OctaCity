//! Deterministic Codex CLI boundary fixture for the released Agent slice.
//!
//! It implements only the two interactions required by the released Codex
//! plugin: a compatibility probe and one JSON event stream. The fixture is an
//! operator-selected executable, not part of an Agent or server release.

use std::{
  io::{Read as _, Write as _},
  path::Path,
  process::{Command, Stdio},
  thread,
  time::{Duration, Instant},
};

const MODE_ENV: &str = "OCTA_CODEX_FIXTURE_MODE";
const PUBLIC_ENV: &str = "OCTA_CODEX_FIXTURE_PUBLIC";
const SECRET_ENV: &str = "OCTA_CODEX_FIXTURE_SECRET";
const UNMAPPED_ENV: &str = "OCTA_CODEX_FIXTURE_UNMAPPED";
const EXPECTED_PUBLIC: &str = "release-public-canary";
const EXPECTED_SECRET: &str = "release-secret-canary-must-not-appear";
const OVERSIZED_EVENT_BYTES: usize = 1024 * 1024 + 1;
const DESCENDANT_STARTUP_DELAY: Duration = Duration::from_secs(2);
const DESCENDANT_READY_TIMEOUT: Duration = Duration::from_secs(15);

fn main() {
  let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
  if arguments
    .first()
    .is_some_and(|argument| argument == "--fixture-descendant")
  {
    let heartbeat = arguments.get(1).expect("fixture descendant requires a heartbeat path");
    // Keep the release contract deterministic while exercising startup that
    // is slower than an unloaded local process spawn.
    thread::sleep(DESCENDANT_STARTUP_DELAY);
    run_heartbeat(Path::new(heartbeat));
  }
  if arguments == ["--version"] {
    println!("codex-cli 0.161.0");
    return;
  }

  let mut prompt = String::new();
  std::io::stdin()
    .read_to_string(&mut prompt)
    .expect("fixture prompt must be readable");
  assert!(!prompt.trim().is_empty(), "fixture requires a non-empty prompt");

  assert_eq!(std::env::var(PUBLIC_ENV).as_deref(), Ok(EXPECTED_PUBLIC));
  assert!(
    std::env::var_os(UNMAPPED_ENV).is_none(),
    "unmapped variables must not enter Codex"
  );
  let secret = std::env::var(SECRET_ENV).expect("fixture secret must be explicitly mapped");
  assert_eq!(secret, EXPECTED_SECRET);

  match std::env::var(MODE_ENV).as_deref() {
    Ok("redaction") => emit_redaction(&secret),
    Ok("overflow") => emit_overflow(&secret),
    Ok("cancel") => run_cancellation_fixture(),
    _ => panic!("fixture mode must be explicitly mapped"),
  }
}

fn emit_event(event: &str) {
  println!("{event}");
  std::io::stdout().flush().expect("fixture events must flush");
}

fn emit_redaction(secret: &str) {
  emit_event(r#"{"type":"turn.started"}"#);
  emit_event(
    &serde_json::json!({
      "type": "item.completed",
      "item": {"type": "agent_message", "text": format!("credential={secret}")}
    })
    .to_string(),
  );
  eprintln!("fixture diagnostic credential={secret}");
  emit_event(
    r#"{"type":"turn.completed","result":{"outcome":"completed","files":1},"thread_id":"release-fixture-thread","turn_id":"release-fixture-turn","usage":{"input_tokens":3,"output_tokens":5}}"#,
  );
}

fn emit_overflow(secret: &str) {
  let ready = Path::new(".octa/codex-fixture-overflow-descendant-ready");
  std::fs::create_dir_all(ready.parent().expect("ready marker must have a parent"))
    .expect("fixture control directory must be writable");
  spawn_descendant(ready);
  wait_for_descendant(ready, "overflow");
  emit_event(r#"{"type":"turn.started"}"#);
  emit_event(
    &serde_json::json!({
      "type": "item.completed",
      "item": {"type": "agent_message", "text": format!("partial credential={secret}")}
    })
    .to_string(),
  );
  eprintln!("overflow diagnostic credential={secret}");
  let mut stdout = std::io::stdout().lock();
  stdout
    .write_all(b"{\"type\":\"future.additive\",\"padding\":\"")
    .expect("oversized event prefix must be writable");
  stdout
    .write_all(&vec![b'x'; OVERSIZED_EVENT_BYTES])
    .expect("oversized event body must be writable");
  stdout
    .write_all(b"\"}\n")
    .expect("oversized event suffix must be writable");
  stdout.flush().expect("oversized event must flush");
}

fn run_cancellation_fixture() -> ! {
  let ready = Path::new(".octa/codex-fixture-descendant-ready");
  std::fs::create_dir_all(ready.parent().expect("ready marker must have a parent"))
    .expect("fixture control directory must be writable");
  spawn_descendant(ready);
  wait_for_descendant(ready, "cancellation");
  emit_event(r#"{"type":"turn.started"}"#);
  run_heartbeat(Path::new(".octa/codex-fixture-parent-heartbeat"));
}

fn wait_for_descendant(ready: &Path, scenario: &str) {
  let deadline = Instant::now() + DESCENDANT_READY_TIMEOUT;
  while !ready.exists() {
    assert!(
      Instant::now() < deadline,
      "{scenario} fixture descendant did not become ready"
    );
    thread::sleep(Duration::from_millis(10));
  }
}

fn run_heartbeat(path: &Path) -> ! {
  let mut counter = 0_u64;
  loop {
    std::fs::write(path, counter.to_string()).expect("heartbeat must be writable");
    counter = counter.wrapping_add(1);
    thread::sleep(Duration::from_millis(10));
  }
}

#[expect(
  clippy::zombie_processes,
  reason = "the cancellation fixture deliberately leaves its child for the runner process owner"
)]
fn spawn_descendant(heartbeat: &Path) {
  Command::new(std::env::current_exe().expect("fixture executable path must be available"))
    .arg("--fixture-descendant")
    .arg(heartbeat)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .expect("fixture descendant must start");
}
