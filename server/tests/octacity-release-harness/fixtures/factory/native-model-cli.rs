//! Finite qualification only: forward the real CLI, changing its model peer and
//! trust for the test-owned HTTPS peer. MCP argv and credentials stay untouched.
use std::{env, os::unix::process::CommandExt as _, process::Command};

fn main() {
  let executable = env!("OCTACITY_FINITE_CODEX_PATH");
  let mut arguments: Vec<_> = env::args().skip(1).collect();
  if arguments == ["--version"] {
    let output = Command::new(executable).arg("--version").output().expect("native version probe");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "codex-cli 0.161.0");
    println!("codex-cli 0.161.0");
    return;
  }
  if arguments.iter().any(|argument| argument == "exec") {
    let provider = format!(
      "--config=model_providers.finite={{name=\"Finite model\",base_url=\"{}\",wire_api=\"responses\",env_key=\"OPENAI_API_KEY\",supports_websockets=false,request_max_retries=0}}",
      env!("OCTACITY_FINITE_MODEL_URL")
    );
    assert_eq!(arguments.pop().as_deref(), Some("-"));
    arguments.extend(["--config=model_provider=\"finite\"".into(), provider, "-".into()]);
  }
  let _error = Command::new(executable)
    .args(arguments)
    .env("SSL_CERT_FILE", "/octacity/protected/skills/review/ca.pem")
    .exec();
  eprintln!("qualified native CLI is unavailable");
  std::process::exit(64);
}
