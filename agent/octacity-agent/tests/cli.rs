//! Black-box checks for the agent CLI contract and structured logging flags.

use std::process::Command;

#[test]
fn prints_the_agent_version() {
  let output = Command::new(env!("CARGO_BIN_EXE_octacity-agent"))
    .arg("--version")
    .output()
    .unwrap();

  assert!(output.status.success());
  assert_eq!(String::from_utf8(output.stdout).unwrap(), "octacity-agent 0.1.0\n");
  assert!(output.stderr.is_empty());
}

#[test]
fn rejects_an_unknown_command() {
  let output = Command::new(env!("CARGO_BIN_EXE_octacity-agent"))
    .arg("serve")
    .output()
    .unwrap();

  assert!(!output.status.success());
  assert!(
    String::from_utf8(output.stderr)
      .unwrap()
      .contains("Usage: octacity-agent")
  );
}

#[test]
fn accepts_structured_logging_options() {
  let output = Command::new(env!("CARGO_BIN_EXE_octacity-agent"))
    .args(["--log-format", "json", "--log-filter", "debug", "--help"])
    .output()
    .unwrap();

  assert!(output.status.success());
  let help = String::from_utf8(output.stdout).unwrap();
  assert!(help.contains("--log-format"));
  assert!(help.contains("--log-filter"));
}
