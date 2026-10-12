//! Single GitHub provider process entry point.
#[tokio::main]
async fn main() -> std::process::ExitCode {
  match octacity_github::serve().await {
    Ok(()) => std::process::ExitCode::SUCCESS,
    Err(_) => std::process::ExitCode::FAILURE,
  }
}
