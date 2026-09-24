use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand, ValueEnum};
use octacity_server::{ServerConfig, ServerRuntime, rebuild_log_search};
use octacity_server_domain::ProjectId;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

#[derive(Debug, Parser)]
#[command(version, about = "OctaCity build coordination server")]
struct Cli {
  /// Tracing filter using the same syntax as RUST_LOG.
  #[arg(long, global = true, value_name = "DIRECTIVES")]
  log_filter: Option<String>,
  /// Structured log representation written to stderr.
  #[arg(long, global = true, value_enum, default_value_t = LogFormat::Json)]
  log_format: LogFormat,
  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// Validate configuration without binding a socket.
  Validate {
    /// Path to the server TOML configuration.
    config: PathBuf,
  },
  /// Run the server until SIGINT or SIGTERM.
  Run {
    /// Path to the server TOML configuration.
    config: PathBuf,
  },
  /// Rebuild one Project's derived log-search projection from committed chunks.
  RebuildLogSearch {
    /// Path to the server TOML configuration.
    config: PathBuf,
    /// Project whose search projection must be rebuilt.
    #[arg(long)]
    project: ProjectId,
  },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum LogFormat {
  Compact,
  Json,
}

fn main() -> ExitCode {
  let cli = Cli::parse();
  if let Err(error) = init_tracing(cli.log_filter.as_deref(), cli.log_format) {
    eprintln!("octacity-server: failed to initialize tracing: {error}");
    return ExitCode::FAILURE;
  }
  let result = match cli.command {
    Command::Validate { config } => validate(config),
    Command::Run { config } => run_async(run(config)),
    Command::RebuildLogSearch { config, project } => run_async(rebuild(config, project)),
  };
  match result {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      error!(%error, "server command failed");
      ExitCode::FAILURE
    }
  }
}

async fn rebuild(path: PathBuf, project_id: ProjectId) -> Result<(), Box<dyn std::error::Error>> {
  let config = ServerConfig::load(&path)?;
  let summary = rebuild_log_search(&config, project_id).await?;
  info!(
    project_id = %summary.project_id,
    queued = summary.queued,
    committed_through = ?summary.committed_through.map(|position| position.get()),
    "Build-log search rebuild queued"
  );
  Ok(())
}

fn validate(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
  let config = ServerConfig::load(&path)?;
  println!(
    "server configuration is valid (management listener {}, security trusted_network_unauthenticated, external acknowledgement {}, agent listener {}, cache listener {})",
    config.management_bind(),
    config.unauthenticated_management_acknowledged(),
    config
      .agent_bind()
      .map_or_else(|| "disabled".to_owned(), |address| address.to_string()),
    config
      .cache_bind()
      .map_or_else(|| "disabled".to_owned(), |address| address.to_string()),
  );
  Ok(())
}

async fn run(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
  let config = ServerConfig::load(&path)?;
  let mut runtime = ServerRuntime::start(config).await?;
  info!(
    management_addr = %runtime.management_addr(),
    agent_addr = ?runtime.agent_addr(),
    cache_addr = ?runtime.cache_addr(),
    management_security_mode = "trusted_network_unauthenticated",
    "server started"
  );
  tokio::select! {
    signal = shutdown_signal() => {
      signal?;
      info!("shutdown signal received");
      runtime.shutdown().await?;
    }
    listener = runtime.wait() => listener?,
  }
  Ok(())
}

fn run_async<F>(future: F) -> Result<(), Box<dyn std::error::Error>>
where
  F: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
  tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?
    .block_on(future)
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

async fn shutdown_signal() -> std::io::Result<()> {
  #[cfg(unix)]
  {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
      result = tokio::signal::ctrl_c() => result,
      signal = terminate.recv() => signal.map(|_| ()).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::BrokenPipe, "SIGTERM stream closed")
      }),
    }
  }
  #[cfg(not(unix))]
  {
    tokio::signal::ctrl_c().await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_run_with_structured_logging_options() {
    let cli = Cli::try_parse_from([
      "octacity-server",
      "--log-filter",
      "octacity_server=debug",
      "--log-format",
      "json",
      "run",
      "server.toml",
    ])
    .unwrap();
    assert_eq!(cli.log_filter.as_deref(), Some("octacity_server=debug"));
    assert_eq!(cli.log_format, LogFormat::Json);
    assert!(matches!(cli.command, Command::Run { .. }));
  }

  #[test]
  fn rejects_an_invalid_tracing_filter() {
    assert!(init_tracing(Some("[invalid"), LogFormat::Json).is_err());
  }

  #[test]
  fn parses_a_project_scoped_log_search_rebuild() {
    let cli = Cli::try_parse_from([
      "octacity-server",
      "rebuild-log-search",
      "server.toml",
      "--project",
      "00000000-0000-0000-0000-000000000001",
    ])
    .unwrap();
    assert!(matches!(cli.command, Command::RebuildLogSearch { .. }));
  }
}
