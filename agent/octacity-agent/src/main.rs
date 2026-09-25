//! OctaCity agent command line and diagnostics subscriber.
//!
//! Component construction and the long-lived worker loop remain separate so
//! CLI and logging concerns cannot leak into lease or job state machines.

use std::{future::Future, path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand, ValueEnum};
use octacity_observability::TraceEvent;
use tracing::{debug, error};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

mod app;
mod composition;
mod maintenance;
#[cfg(windows)]
mod windows_service;

#[derive(Debug, Parser)]
#[command(version, about = "OctaCity self-hosted build agent")]
struct Cli {
  /// Tracing filter, using the same syntax as RUST_LOG.
  #[arg(long, global = true, value_name = "DIRECTIVES")]
  log_filter: Option<String>,

  /// Log representation written by foreground commands to stderr.
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
  /// Run under the Windows Service Control Manager.
  #[cfg(windows)]
  #[command(hide = true)]
  Service {
    /// SCM registration name used by the service dispatcher.
    #[arg(long)]
    service_name: String,
    /// Path to the agent TOML configuration.
    config: PathBuf,
  },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum LogFormat {
  Compact,
  Json,
}

fn main() -> ExitCode {
  let cli = Cli::parse();
  if let Err(error) = init_tracing(cli.log_filter.as_deref(), cli.log_format, &cli.command) {
    eprintln!("octacity-agent: failed to initialize tracing: {error}");
    return ExitCode::FAILURE;
  }
  debug!(command = ?cli.command, "parsed command line");
  let result = match cli.command {
    Command::Validate { config } => run_async_command(app::validate(config)),
    Command::Run { config } => run_async_command(app::run(config)),
    #[cfg(windows)]
    Command::Service { service_name, config } => windows_service::dispatch(service_name, config),
  };
  match result {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      error!(event.name = TraceEvent::AgentCommandFailed.as_str(), error = %error, "agent command failed");
      ExitCode::FAILURE
    }
  }
}

/// Runs a foreground asynchronous command on its only Tokio runtime.
///
/// The Windows SCM command deliberately bypasses this function because its
/// callback thread owns a separate long-lived worker runtime. Keeping `main`
/// synchronous prevents an idle outer runtime from surviving for the complete
/// service lifetime.
fn run_async_command<F>(future: F) -> Result<(), Box<dyn std::error::Error>>
where
  F: Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
  tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?
    .block_on(future)
}

fn init_tracing(filter: Option<&str>, format: LogFormat, command: &Command) -> Result<(), Box<dyn std::error::Error>> {
  let filter = match filter {
    Some(filter) => EnvFilter::try_new(filter)?,
    None => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
  };
  #[cfg(windows)]
  if let Command::Service { service_name, .. } = command {
    let native = tracing_layer_win_eventlog::EventLogLayer::new(service_name)?;
    tracing_subscriber::registry().with(filter).with(native).try_init()?;
    return Ok(());
  }
  #[cfg(not(windows))]
  let _ = command;

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
    assert!(
      init_tracing(
        Some("[invalid"),
        LogFormat::Compact,
        &Command::Validate {
          config: PathBuf::from("agent.toml"),
        },
      )
      .is_err()
    );
  }

  #[test]
  fn foreground_commands_use_one_explicit_multithread_runtime() {
    run_async_command(async {
      assert_eq!(
        tokio::runtime::Handle::current().runtime_flavor(),
        tokio::runtime::RuntimeFlavor::MultiThread
      );
      Ok(())
    })
    .unwrap();
  }

  #[cfg(windows)]
  #[test]
  fn parses_the_private_windows_service_entrypoint() {
    let cli = Cli::try_parse_from([
      "octacity-agent",
      "service",
      "--service-name",
      "OctaCityAgent",
      r"C:\ProgramData\OctaCity\agent.toml",
    ])
    .unwrap();
    assert!(matches!(
      cli.command,
      Command::Service { service_name, config }
        if service_name == "OctaCityAgent"
          && config == std::path::Path::new(r"C:\ProgramData\OctaCity\agent.toml")
    ));
  }
}
