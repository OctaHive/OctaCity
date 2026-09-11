//! Thin command-line entry point for configuring diagnostics and invoking the
//! agent's startup validation routines.
//!
//! Long-lived coordination is intentionally absent until the server-agent
//! transport is implemented. This binary is the composition root: focused
//! component crates contain reusable behavior and never depend on the agent.

use std::{collections::BTreeMap, path::PathBuf, process::ExitCode, sync::Arc, time::Duration};

use clap::{Parser, Subcommand, ValueEnum};
use octacity_config::{AgentConfig, OciEngineConfig, ValidatedConfig, ValidatedRuntimeConfig};
use octacity_execution::ExecutionBackend;
use octacity_execution_containerd::{ContainerdEngine, ContainerdEngineConfig};
use octacity_execution_microsandbox::{MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_native::{LinuxNativeConfig, NativeBackend};
use octacity_execution_oci::{OciBackend, OciEngine};
use octacity_job::{JobExecutor, JobExecutorConfig};
use octacity_protocol::RuntimeMode;
use octacity_runner::RunnerInstallation;
use octacity_runner::RunnerSupervisionPolicy;
use octacity_source::{SourceMaterializer, SourcePluginRegistry};
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
      let runner = RunnerInstallation::load(&validated.config.octa_release_root)?;
      let octa_version = runner.capabilities.octa_version.clone();
      let source_plugins = Arc::new(SourcePluginRegistry::discover(&validated.config.source_plugins_dir)?);
      let source_plugin_count = source_plugins.len();
      let agent_id = validated.config.agent_id.clone();
      let signing_key_count = validated.signing_keys.len();
      let runtime_count = validated.config.enabled_runtime_modes.len();
      let _executor = build_executor(validated, runner, source_plugins).await?;
      println!(
        "agent '{}' configuration is valid (Octa {}, {} signing key(s), {} runtime mode(s), {} source plugin(s))",
        agent_id, octa_version, signing_key_count, runtime_count, source_plugin_count
      );
      info!(
        agent_id = %agent_id,
        signing_keys = signing_key_count,
        runtime_modes = runtime_count,
        source_plugins = source_plugin_count,
        "agent configuration is valid"
      );
      Ok(())
    }
  }
}

async fn build_executor(
  validated: ValidatedConfig,
  runner: RunnerInstallation,
  source_plugins: Arc<SourcePluginRegistry>,
) -> Result<JobExecutor, Box<dyn std::error::Error>> {
  let mut backends: BTreeMap<RuntimeMode, Arc<dyn ExecutionBackend>> = BTreeMap::new();
  for runtime in &validated.runtimes {
    let (mode, backend): (RuntimeMode, Arc<dyn ExecutionBackend>) = match runtime {
      ValidatedRuntimeConfig::Native {
        cgroup_root,
        bubblewrap,
        readonly_paths,
        pids_limit,
        environment,
      } => (
        RuntimeMode::Native,
        Arc::new(NativeBackend::new(LinuxNativeConfig {
          cgroup_root: cgroup_root.clone(),
          work_root: validated.config.work_root.clone(),
          bubblewrap: bubblewrap.clone(),
          readonly_paths: readonly_paths.clone(),
          runner_platform: runner.capabilities.platform.clone(),
          max_workspace_bytes: validated.config.max_workspace_bytes,
          cleanup_timeout: Duration::from_secs(validated.config.cleanup_timeout_seconds),
          pids_limit: *pids_limit,
          environment: environment.clone(),
        })?),
      ),
      ValidatedRuntimeConfig::Oci {
        engines: configured_engines,
      } => {
        let mut engines: Vec<Arc<dyn OciEngine>> = Vec::new();
        for configured in configured_engines {
          let engine: Arc<dyn OciEngine> = match configured {
            OciEngineConfig::Microsandbox {
              executable,
              libkrunfw,
              metrics_sample_interval_seconds,
            } => Arc::new(MicrosandboxEngine::new(MicrosandboxEngineConfig {
              agent_id: validated.config.agent_id.clone(),
              state_root: validated.config.state_root.clone(),
              work_root: validated.config.work_root.clone(),
              runner_platform: runner.capabilities.platform.clone(),
              executable: executable.clone(),
              libkrunfw: libkrunfw.clone(),
              cleanup_timeout: Duration::from_secs(validated.config.cleanup_timeout_seconds),
              metrics_sample_interval: Duration::from_secs(*metrics_sample_interval_seconds),
            })?),
            OciEngineConfig::Containerd {
              endpoint,
              namespace,
              snapshotter,
              runtime,
              registry_config_dir,
              pids_limit,
              open_files_limit,
            } => {
              let engine = ContainerdEngine::new(ContainerdEngineConfig {
                agent_id: validated.config.agent_id.clone(),
                endpoint: endpoint.clone(),
                namespace: namespace.clone(),
                snapshotter: snapshotter.clone(),
                runtime: runtime.clone(),
                registry_config_dir: registry_config_dir.clone(),
                state_root: validated.config.state_root.clone(),
                work_root: validated.config.work_root.clone(),
                max_workspace_bytes: validated.config.max_workspace_bytes,
                cleanup_timeout: Duration::from_secs(validated.config.cleanup_timeout_seconds),
                pids_limit: *pids_limit,
                open_files_limit: *open_files_limit,
              })?;
              engine.validate_connection().await?;
              Arc::new(engine)
            }
          };
          engines.push(engine);
        }
        (RuntimeMode::Oci, Arc::new(OciBackend::new(engines)?))
      }
    };
    backends.insert(mode, backend);
  }
  let source: Arc<dyn SourceMaterializer> = source_plugins;
  Ok(JobExecutor::new(
    validated.signing_keys,
    runner,
    source,
    backends,
    JobExecutorConfig {
      work_root: validated.config.work_root,
      max_workspace_bytes: validated.config.max_workspace_bytes,
      cancellation_grace: Duration::from_secs(validated.config.graceful_cancel_timeout_seconds),
      runner_supervision: RunnerSupervisionPolicy {
        hello_timeout: Duration::from_secs(validated.config.runner_hello_timeout_seconds),
        resource_sample_interval: Duration::from_secs(validated.config.resource_sample_interval_seconds),
        resource_sample_timeout: Duration::from_secs(validated.config.resource_sample_timeout_seconds),
        max_accounting_failures: validated.config.max_accounting_failures,
      },
    },
  )?)
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
