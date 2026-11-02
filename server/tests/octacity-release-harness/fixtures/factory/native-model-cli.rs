//! Finite qualification only: forward the real CLI, changing its model peer and
//! trust for the test-owned HTTPS peer. MCP argv and credentials stay untouched.
use std::{env, fs, os::unix::process::CommandExt as _, process::Command};

fn main() {
  let executable = env!("OCTACITY_FINITE_CODEX_PATH");
  let mut arguments: Vec<_> = env::args().skip(1).collect();
  if arguments == ["--version"] {
    let output = Command::new(executable)
      .arg("--version")
      .output()
      .expect("native version probe");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "codex-cli 0.161.0");
    println!("codex-cli 0.161.0");
    return;
  }
  if arguments.iter().any(|argument| argument == "exec") {
    let base = env!("OCTACITY_FINITE_MODEL_URL");
    let base = if option_env!("OCTACITY_FINITE_OBSERVE_PROCESS_LIMIT") == Some("1") {
      let limit = observe_process_limit();
      format!("{base}/process-limit/{limit}")
    } else {
      base.to_owned()
    };
    let provider = format!(
      "--config=model_providers.finite={{name=\"Finite model\",base_url=\"{base}\",wire_api=\"responses\",env_key=\"OPENAI_API_KEY\",supports_websockets=false,request_max_retries=0}}",
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

fn observe_process_limit() -> u32 {
  let limits = fs::read_to_string("/proc/self/limits").expect("read guest process limits");
  let mut processes = limits
    .lines()
    .find_map(|line| line.strip_prefix("Max processes"))
    .expect("guest process limit")
    .split_whitespace();
  let soft: u32 = processes.next().unwrap().parse().expect("finite guest soft limit");
  let hard: u32 = processes.next().unwrap().parse().expect("finite guest hard limit");
  assert_eq!(soft, hard);
  let raised = Command::new("/bin/bash")
    .args(["-c", "ulimit -Hu \"$1\"", "qualification", &(hard + 1).to_string()])
    .output()
    .expect("attempt to raise the guest process limit");
  assert!(!raised.status.success(), "guest must not be able to raise its limit");
  soft
}
