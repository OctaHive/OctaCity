//! OctaCity agent command line and diagnostics subscriber.
//!
//! Component construction and the long-lived worker loop remain separate so
//! CLI and logging concerns cannot leak into lease or job state machines.

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand, ValueEnum};
use tracing::{debug, error};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

mod app;
mod composition;

#[derive(Debug, Parser)]
#[command(version, about = "OctaCity self-hosted build agent")]
struct Cli {
  /// Tracing filter, using the same syntax as RUST_LOG.
  #[arg(long, global = true, value_name = "DIRECTIVES")]
  log_filter: Option<String>,

  /// Log representation written to stderr.
  #[arg(long, global = true, value_enum, default_value_t = LogFormat::Compact)]
  log_format: LogFormat,

  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// Validate an agent installation without contacting the server.
  Validate {
    /// Path to the agent TOML configuration.
    config: PathBuf,
  },
  /// Run the long-lived one-job agent worker.
  Run {
    /// Path to the agent TOML configuration.
    config: PathBuf,
  },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum LogFormat {
  Compact,
  Json,
}

#[tokio::main]
async fn main() -> ExitCode {
  let cli = Cli::parse();
  if let Err(error) = init_tracing(cli.log_filter.as_deref(), cli.log_format) {
    eprintln!("octacity-agent: failed to initialize tracing: {error}");
    return ExitCode::FAILURE;
  }
  debug!(command = ?cli.command, "parsed command line");
  let result = match cli.command {
    Command::Validate { config } => app::validate(config).await,
    Command::Run { config } => app::run(config).await,
  };
  match result {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      error!(error = %error, "agent command failed");
      ExitCode::FAILURE
    }
  }
}

fn init_tracing(filter: Option<&str>, format: LogFormat) -> Result<(), Box<dyn std::error::Error>> {
  let filter = match filter {
    Some(filter) => EnvFilter::try_new(filter)?,
    None => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
  };
  let registry = tracing_subscriber::registry().with(filter);
  match format {
    LogFormat::Compact => registry.with(tracing_subscriber::fmt::layer().compact()).try_init()?,
    LogFormat::Json => registry.with(tracing_subscriber::fmt::layer().json()).try_init()?,
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_run_and_structured_logging_options() {
    let cli = Cli::try_parse_from([
      "octacity-agent",
      "--log-filter",
      "octacity_agent=debug",
      "--log-format",
      "json",
      "run",
      "/etc/octacity/agent.toml",
    ])
    .unwrap();
    assert_eq!(cli.log_filter.as_deref(), Some("octacity_agent=debug"));
    assert_eq!(cli.log_format, LogFormat::Json);
    assert!(matches!(cli.command, Command::Run { .. }));
  }

  #[test]
  fn rejects_an_invalid_tracing_filter() {
    assert!(init_tracing(Some("[invalid"), LogFormat::Compact).is_err());
  }
}
