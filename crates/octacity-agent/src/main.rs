use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand, ValueEnum};
use octacity_agent::{config::AgentConfig, runner_installation::RunnerInstallation};
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

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
  match run(cli.command).await {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      error!(error = %error, "agent command failed");
      ExitCode::FAILURE
    }
  }
}

async fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
  match command {
    Command::Validate { config } => {
      info!(config = %config.display(), "validating agent configuration");
      let validated = AgentConfig::load(&config)?.validate()?;
      let runner = RunnerInstallation::load(&validated.config.octa_release_root).await?;
      println!(
        "agent '{}' configuration is valid (Octa {}, {} signing key(s), {} backend(s), {} source plugin(s))",
        validated.config.agent_id,
        runner.capabilities.octa_version,
        validated.signing_keys.len(),
        validated.config.enabled_execution_backends.len(),
        validated.source_plugins.len()
      );
      info!(
        agent_id = %validated.config.agent_id,
        signing_keys = validated.signing_keys.len(),
        execution_backends = validated.config.enabled_execution_backends.len(),
        source_plugins = validated.source_plugins.len(),
        "agent configuration is valid"
      );
      Ok(())
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
  fn parses_structured_logging_options() {
    let cli = Cli::try_parse_from([
      "octacity-agent",
      "--log-filter",
      "octacity_agent=debug",
      "--log-format",
      "json",
      "validate",
      "/etc/octacity/agent.toml",
    ])
    .unwrap();
    assert_eq!(cli.log_filter.as_deref(), Some("octacity_agent=debug"));
    assert_eq!(cli.log_format, LogFormat::Json);
    assert!(matches!(cli.command, Command::Validate { .. }));
  }

  #[test]
  fn rejects_an_invalid_tracing_filter() {
    assert!(init_tracing(Some("[invalid"), LogFormat::Compact).is_err());
  }

  #[tokio::test]
  async fn validate_reports_configuration_errors() {
    let error = run(Command::Validate {
      config: PathBuf::from("/path/that/does/not/exist"),
    })
    .await
    .unwrap_err();
    assert!(error.to_string().contains("failed to inspect configuration"));
  }
}
