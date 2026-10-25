//! Native fixture pin required by the Agent's external-executable verification.
//! The finite JSON program is embedded; no repository script is loaded at runtime.
use std::{env, os::unix::process::CommandExt as _, process::Command};

fn main() {
  let program = include_str!("pilot-cli.mjs").split_once('\n').unwrap().1;
  // Build-time selection is part of the immutable binary pin, never task data.
  let node = option_env!("OCTACITY_FACTORY_PILOT_NODE").unwrap_or("/usr/local/bin/node");
  let arguments: Vec<_> = env::args_os().skip(1).collect();
  if arguments.len() == 2 && arguments[0] == "--mcp" {
    let _error = Command::new(node).arg(&arguments[1]).exec();
    eprintln!("finite MCP fixture runtime is unavailable");
    std::process::exit(64);
  }
  let _error = Command::new(node)
    .args(["--input-type=module", "--eval"])
    .arg(format!("process.argv.splice(1, 0, 'finite-cli');\n{program}"))
    .arg("--")
    .args(env::args_os().skip(1))
    .exec();
  eprintln!("finite model fixture runtime is unavailable");
  std::process::exit(64);
}
