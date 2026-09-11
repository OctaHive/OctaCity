//! Thin command-line entry point for configuring diagnostics and invoking the
//! agent's startup validation routines.
//!
//! Long-lived coordination is intentionally absent until durable event and
//! completion delivery exists in phase 5. This binary is the composition
//! root: focused component crates contain reusable behavior and never depend
//! on the agent.

use std::{collections::BTreeMap, path::PathBuf, process::ExitCode, sync::Arc, time::Duration};

use clap::{Parser, Subcommand, ValueEnum};
use octacity_config::{AgentConfig, OciEngineConfig, ValidatedConfig, ValidatedRuntimeConfig};
use octacity_coordinator::{HttpCoordinatorClient, HttpCoordinatorConfig, RetryPolicy};
use octacity_execution::{ExecutionArchitecture, ExecutionBackend, ExecutionOs, ExecutionPlatform, OciIsolation};
use octacity_execution_containerd::{CONTAINERD_ENGINE_NAME, ContainerdEngine, ContainerdEngineConfig};
use octacity_execution_microsandbox::{MICROSANDBOX_ENGINE_NAME, MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_native::{LinuxNativeConfig, NATIVE_BACKEND_NAME, NativeBackend};
use octacity_execution_oci::{OciBackend, OciCapability, OciEngine};
use octacity_inventory::{HostMonitor, build_inventory, host_platform};
use octacity_job::{JobExecutor, JobExecutorConfig};
use octacity_protocol::{
  BackendHealth, BackendHealthStatus, PlatformArchitecture, PlatformOs, RuntimeCapability, RuntimeMode,
};
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
      let (_executor, runtimes, backend_health) =
        build_executor(&validated, runner.clone(), source_plugins.clone()).await?;
      let virtualization_available = runtimes
        .iter()
        .any(|capability| capability.isolation == Some(octacity_protocol::OciIsolation::Hypervisor));
      let mut host = HostMonitor::new(
        validated.config.work_root.clone(),
        validated.config.state_root.clone(),
        virtualization_available,
      )?;
      let inventory = build_inventory(
        agent_id.clone(),
        env!("CARGO_PKG_VERSION").to_owned(),
        validated.config.labels.clone(),
        runtimes,
        &runner,
        &source_plugins,
        host.capacity().clone(),
      )?;
      let _snapshot = host.snapshot(None, backend_health)?;
      let _coordinator = HttpCoordinatorClient::new(HttpCoordinatorConfig {
        server_url: validated.config.server_url.clone(),
        credential_file: validated.config.credential_file.clone(),
        request_timeout: Duration::from_secs(validated.config.coordinator_request_timeout_seconds),
        max_body_bytes: validated.config.coordinator_max_body_bytes,
        retry: RetryPolicy {
          max_attempts: validated.config.retry_max_attempts,
          initial_delay: Duration::from_millis(validated.config.retry_initial_delay_milliseconds),
          max_delay: Duration::from_secs(validated.config.retry_max_delay_seconds),
        },
      })?;
      println!(
        "agent '{}' configuration is valid (Octa {}, {} signing key(s), {} runtime mode(s), {} runtime route(s), {} source plugin(s))",
        agent_id,
        octa_version,
        signing_key_count,
        runtime_count,
        inventory.runtimes.len(),
        source_plugin_count
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
  validated: &ValidatedConfig,
  runner: RunnerInstallation,
  source_plugins: Arc<SourcePluginRegistry>,
) -> Result<(JobExecutor, Vec<RuntimeCapability>, Vec<BackendHealth>), Box<dyn std::error::Error>> {
  let mut backends: BTreeMap<RuntimeMode, Arc<dyn ExecutionBackend>> = BTreeMap::new();
  let mut runtime_capabilities = Vec::new();
  let mut backend_health = Vec::new();
  for runtime in &validated.runtimes {
    let (mode, backend): (RuntimeMode, Arc<dyn ExecutionBackend>) = match runtime {
      ValidatedRuntimeConfig::Native {
        cgroup_root,
        bubblewrap,
        readonly_paths,
        pids_limit,
        environment,
      } => {
        let backend = Arc::new(NativeBackend::new(LinuxNativeConfig {
          cgroup_root: cgroup_root.clone(),
          work_root: validated.config.work_root.clone(),
          bubblewrap: bubblewrap.clone(),
          readonly_paths: readonly_paths.clone(),
          runner_platform: runner.capabilities.platform.clone(),
          max_workspace_bytes: validated.config.max_workspace_bytes,
          cleanup_timeout: Duration::from_secs(validated.config.cleanup_timeout_seconds),
          pids_limit: *pids_limit,
          environment: environment.clone(),
        })?);
        runtime_capabilities.push(RuntimeCapability {
          backend: NATIVE_BACKEND_NAME.to_owned(),
          mode: RuntimeMode::Native,
          platform: host_platform()?,
          isolation: None,
        });
        backend_health.push(ready_backend(NATIVE_BACKEND_NAME));
        (RuntimeMode::Native, backend)
      }
      ValidatedRuntimeConfig::Oci {
        engines: configured_engines,
      } => {
        let mut engines: Vec<Arc<dyn OciEngine>> = Vec::new();
        for configured in configured_engines {
          let (name, engine): (&str, Arc<dyn OciEngine>) = match configured {
            OciEngineConfig::Microsandbox {
              executable,
              libkrunfw,
              metrics_sample_interval_seconds,
            } => (
              MICROSANDBOX_ENGINE_NAME,
              Arc::new(MicrosandboxEngine::new(MicrosandboxEngineConfig {
                agent_id: validated.config.agent_id.clone(),
                state_root: validated.config.state_root.clone(),
                work_root: validated.config.work_root.clone(),
                runner_platform: runner.capabilities.platform.clone(),
                executable: executable.clone(),
                libkrunfw: libkrunfw.clone(),
                cleanup_timeout: Duration::from_secs(validated.config.cleanup_timeout_seconds),
                metrics_sample_interval: Duration::from_secs(*metrics_sample_interval_seconds),
              })?),
            ),
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
              (CONTAINERD_ENGINE_NAME, Arc::new(engine))
            }
          };
          runtime_capabilities.extend(
            engine
              .capabilities()
              .into_iter()
              .map(|capability| advertised_oci_capability(name, capability)),
          );
          backend_health.push(ready_backend(name));
          engines.push(engine);
        }
        (RuntimeMode::Oci, Arc::new(OciBackend::new(engines)?))
      }
    };
    backends.insert(mode, backend);
  }
  let source: Arc<dyn SourceMaterializer> = source_plugins;
  let executor = JobExecutor::new(
    validated.signing_keys.clone(),
    runner,
    source,
    backends,
    JobExecutorConfig {
      work_root: validated.config.work_root.clone(),
      max_workspace_bytes: validated.config.max_workspace_bytes,
      cancellation_grace: Duration::from_secs(validated.config.graceful_cancel_timeout_seconds),
      runner_supervision: RunnerSupervisionPolicy {
        hello_timeout: Duration::from_secs(validated.config.runner_hello_timeout_seconds),
        resource_sample_interval: Duration::from_secs(validated.config.resource_sample_interval_seconds),
        resource_sample_timeout: Duration::from_secs(validated.config.resource_sample_timeout_seconds),
        max_accounting_failures: validated.config.max_accounting_failures,
      },
    },
  )?;
  Ok((executor, runtime_capabilities, backend_health))
}

fn advertised_oci_capability(backend: &str, capability: OciCapability) -> RuntimeCapability {
  RuntimeCapability {
    backend: backend.to_owned(),
    mode: RuntimeMode::Oci,
    platform: protocol_platform(capability.platform),
    isolation: Some(match capability.isolation {
      OciIsolation::Process => octacity_protocol::OciIsolation::Process,
      OciIsolation::Hypervisor => octacity_protocol::OciIsolation::Hypervisor,
    }),
  }
}

fn protocol_platform(platform: ExecutionPlatform) -> octacity_protocol::PlatformSpec {
  octacity_protocol::PlatformSpec {
    os: match platform.os {
      ExecutionOs::Linux => PlatformOs::Linux,
      ExecutionOs::Windows => PlatformOs::Windows,
      ExecutionOs::Macos => PlatformOs::Macos,
    },
    architecture: match platform.architecture {
      ExecutionArchitecture::Amd64 => PlatformArchitecture::Amd64,
      ExecutionArchitecture::Arm64 => PlatformArchitecture::Arm64,
    },
  }
}

fn ready_backend(backend: &str) -> BackendHealth {
  BackendHealth {
    backend: backend.to_owned(),
    status: BackendHealthStatus::Ready,
    message: None,
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
