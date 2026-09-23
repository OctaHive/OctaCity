//! Process entry point for the read-only Git VCS adapter.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
  match octacity_vcs_git::serve().await {
    Ok(()) => ExitCode::SUCCESS,
    Err(_) => ExitCode::FAILURE,
  }
}
