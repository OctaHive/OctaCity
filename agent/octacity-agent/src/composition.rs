//! Constructs concrete adapters once at the executable boundary.
//!
//! The executable is the only crate allowed to know every concrete adapter. It
//! validates operator configuration, discovers installed source plugins,
//! constructs enabled execution engines, and then exposes them through the
//! narrow interfaces consumed by orchestration. Keeping this wiring here makes
//! dependencies point from policy toward implementation only at the outermost
//! layer; coordinator, job, and lifecycle crates never select adapters.

use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

use octacity_cache_session::{CacheSessionManager, CacheSessionManagerConfig};
use octacity_config::{AgentConfig, OciEngineConfig, ValidatedConfig, ValidatedRuntimeConfig};
use octacity_coordinator::{
  CacheSessionCoordinator, CoordinatorClient, HttpCoordinatorClient, HttpCoordinatorConfig, OutputUploadCoordinator,
  RetryPolicy,
};
use octacity_execution::{ExecutionArchitecture, ExecutionBackend, ExecutionOs, ExecutionPlatform, OciIsolation};
use octacity_execution_containerd::{CONTAINERD_ENGINE_NAME, ContainerdEngine, ContainerdEngineConfig};
use octacity_execution_microsandbox::{MICROSANDBOX_ENGINE_NAME, MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_native::{LinuxNativeConfig, NATIVE_BACKEND_NAME, NativeBackend};
use octacity_execution_oci::{OciBackend, OciCapability, OciEngine};
use octacity_identity::FileWorkloadIdentityProvider;
use octacity_inventory::{AgentInventoryConfig, HostMonitor, build_inventory, host_platform};
use octacity_job::{JobExecutor, JobExecutorConfig};
use octacity_output::{OutputPublisher, PresignedOutputPublisher, PresignedOutputPublisherConfig};
use octacity_protocol::{
  AgentInventory, BackendHealth, BackendHealthStatus, PlatformArchitecture, PlatformOs, RuntimeCapability, RuntimeMode,
};
use octacity_runner::{RunnerInstallation, RunnerSupervisionPolicy};
use octacity_source::{SourceMaterializer, SourcePluginRegistry};

pub(crate) struct Components {
  pub(crate) validated: ValidatedConfig,
  pub(crate) runner: RunnerInstallation,
  pub(crate) executor: Arc<JobExecutor>,
  pub(crate) coordinator: Arc<dyn CoordinatorClient>,
  pub(crate) cache_coordinator: Arc<dyn CacheSessionCoordinator>,
  pub(crate) cache: Arc<CacheSessionManager>,
  pub(crate) outputs: Arc<dyn OutputPublisher>,
  pub(crate) inventory: AgentInventory,
  pub(crate) host: HostMonitor,
  pub(crate) backend_health: Vec<BackendHealth>,
  pub(crate) source_plugin_count: usize,
}

impl Components {
  pub(crate) async fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
    let validated = AgentConfig::load(path)?.validate()?;
    let runner = RunnerInstallation::load(&validated.config.octa_release_root)?;
    let source_plugins = Arc::new(SourcePluginRegistry::discover(&validated.config.source_plugins_dir)?);
    let source_plugin_count = source_plugins.len();
    let (executor, runtimes, backend_health, cache) =
      build_executor(&validated, runner.clone(), source_plugins.clone()).await?;
    let virtualization_available = runtimes
      .iter()
      .any(|capability| capability.isolation == Some(octacity_protocol::OciIsolation::Hypervisor));
    let host = HostMonitor::new(
      validated.config.work_root.clone(),
      validated.config.state_root.clone(),
      virtualization_available,
    )?;
    let inventory = build_inventory(
      AgentInventoryConfig {
        agent_id: validated.config.agent_id.clone(),
        agent_version: env!("CARGO_PKG_VERSION").to_owned(),
        labels: validated.config.labels.clone(),
        remote_cache_configured: !validated.config.cache.allowed_remote_origins.is_empty(),
      },
      runtimes,
      &runner,
      &source_plugins,
      host.capacity().clone(),
    )?;
    let retry = RetryPolicy {
      max_attempts: validated.config.retry_max_attempts,
      initial_delay: Duration::from_millis(validated.config.retry_initial_delay_milliseconds),
      max_delay: Duration::from_secs(validated.config.retry_max_delay_seconds),
    };
    let http_coordinator = Arc::new(HttpCoordinatorClient::new(HttpCoordinatorConfig {
      server_url: validated.config.server_url.clone(),
      credential_file: validated.config.credential_file.clone(),
      request_timeout: Duration::from_secs(validated.config.coordinator_request_timeout_seconds),
      max_body_bytes: validated.config.coordinator_max_body_bytes,
      retry: retry.clone(),
    })?);
    let coordinator: Arc<dyn CoordinatorClient> = http_coordinator.clone();
    let cache_coordinator: Arc<dyn CacheSessionCoordinator> = http_coordinator.clone();
    let output_coordinator: Arc<dyn OutputUploadCoordinator> = http_coordinator;
    let outputs: Arc<dyn OutputPublisher> = Arc::new(PresignedOutputPublisher::new(
      output_coordinator,
      PresignedOutputPublisherConfig {
        allowed_origins: validated.config.allowed_upload_origins.clone(),
        max_archive_entries: validated.config.max_archive_entries,
        upload_timeout: Duration::from_secs(validated.config.upload_timeout_seconds),
        retry,
      },
    )?);
    Ok(Self {
      validated,
      runner,
      executor: Arc::new(executor),
      coordinator,
      cache_coordinator,
      cache,
      outputs,
      inventory,
      host,
      backend_health,
      source_plugin_count,
    })
  }
}

async fn build_executor(
  validated: &ValidatedConfig,
  runner: RunnerInstallation,
  source_plugins: Arc<SourcePluginRegistry>,
) -> Result<
  (
    JobExecutor,
    Vec<RuntimeCapability>,
    Vec<BackendHealth>,
    Arc<CacheSessionManager>,
  ),
  Box<dyn std::error::Error>,
> {
  #[cfg(unix)]
  if validated.runtimes.iter().any(|runtime| match runtime {
    ValidatedRuntimeConfig::Native { .. } => true,
    ValidatedRuntimeConfig::Oci { engines } => engines
      .iter()
      .any(|engine| matches!(engine, OciEngineConfig::Containerd { .. })),
  }) {
    let aggregate_max_bytes = validated
      .config
      .cache
      .capacity
      .max_bytes
      .checked_mul(u64::try_from(validated.config.cache.max_scopes)?)
      .ok_or("validated aggregate cache capacity overflowed")?;
    octacity_execution::validate_process_cache_capacity_root(&validated.config.cache.root, aggregate_max_bytes)?;
  }
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
      ValidatedRuntimeConfig::Oci { engines: configured } => {
        let mut engines: Vec<Arc<dyn OciEngine>> = Vec::new();
        for engine_config in configured {
          let (name, engine): (&str, Arc<dyn OciEngine>) = match engine_config {
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
  let identity = Arc::new(FileWorkloadIdentityProvider::new(
    validated.config.workload_identity_profiles.clone(),
  ));
  let cache = Arc::new(CacheSessionManager::new(CacheSessionManagerConfig {
    root: validated.config.cache.root.clone(),
    capacity: validated.config.cache.capacity,
    max_scopes: validated.config.cache.max_scopes,
    allow_read: validated.config.cache.allow_read,
    allow_write: validated.config.cache.allow_write,
    allowed_origins: validated.config.cache.allowed_remote_origins.clone(),
    ca_certificate_file: validated.config.cache.ca_certificate_file.clone(),
    native_identities: validated.config.cache.native_environment_identities.clone(),
    request_timeout_seconds: validated.config.cache.request_timeout_seconds,
    max_parallel_transfers: validated.config.cache.max_parallel_transfers,
  })?);
  let executor = JobExecutor::new(
    runner,
    source,
    identity,
    backends,
    JobExecutorConfig {
      work_root: validated.config.work_root.clone(),
      max_workspace_bytes: validated.config.max_workspace_bytes,
      allow_unrestricted_network: validated.config.allow_unrestricted_network,
      allowed_network_hosts: validated.config.allowed_network_hosts.clone(),
      max_output_limits: validated.config.max_output_limits.clone(),
      cancellation_grace: Duration::from_secs(validated.config.graceful_cancel_timeout_seconds),
      runner_supervision: RunnerSupervisionPolicy {
        hello_timeout: Duration::from_secs(validated.config.runner_hello_timeout_seconds),
        resource_sample_interval: Duration::from_secs(validated.config.resource_sample_interval_seconds),
        resource_sample_timeout: Duration::from_secs(validated.config.resource_sample_timeout_seconds),
        max_accounting_failures: validated.config.max_accounting_failures,
      },
    },
  )?
  .with_cache(cache.clone());
  Ok((executor, runtime_capabilities, backend_health, cache))
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

#[cfg(test)]
pub(crate) mod tests {
  use super::*;

  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  use std::{fs, path::PathBuf};

  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  pub(crate) struct InstalledAgentFixture {
    pub(crate) _temporary: tempfile::TempDir,
    pub(crate) config: PathBuf,
  }

  /// Creates a complete but inert installation. The Microsandbox adapter is
  /// validated and inventoried, but no VM is started by configuration loading.
  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  pub(crate) fn installed_agent_fixture(server_url: &str) -> InstalledAgentFixture {
    use std::os::unix::fs::PermissionsExt as _;

    let temporary = tempfile::tempdir().unwrap();
    let directory = |name: &str| {
      let path = temporary.path().join(name);
      fs::create_dir(&path).unwrap();
      path
    };
    let work_root = directory("work");
    let state_root = directory("state");
    let release_root = directory("octa");
    let source_plugins = directory("sources");
    let cache_root = temporary.path().join("cache");
    fs::create_dir(&cache_root).unwrap();
    fs::set_permissions(&cache_root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(release_root.join("plugins")).unwrap();
    fs::write(release_root.join("Octa.lock"), "version: 1\nplugins: {}\n").unwrap();

    let runner = release_root.join("octa-runner");
    fs::write(&runner, "inert runner fixture").unwrap();
    fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
    let runner_platform = if cfg!(target_arch = "x86_64") {
      "linux-x86_64"
    } else {
      "linux-aarch64"
    };
    fs::write(
      release_root.join("octa-runner-capabilities.json"),
      format!(
        "{{\"type\":\"capabilities\",\"octa_version\":\"0.3.0\",\"runner_protocols\":[3],\"event_schemas\":[4],\"plugin_protocols\":[2],\"octafile_versions\":[1],\"platform\":\"{runner_platform}\",\"features\":[\"{cache_feature}\",\"{cache_http_feature}\"]}}",
        cache_feature = octacity_protocol::CACHE_FEATURE_V1,
        cache_http_feature = octacity_protocol::CACHE_HTTP_FEATURE_V1,
      ),
    )
    .unwrap();

    let credential = temporary.path().join("credential");
    fs::write(&credential, "fixture-credential").unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    let executable = temporary.path().join("msb");
    fs::write(&executable, "inert microsandbox fixture").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let libkrunfw = temporary.path().join("libkrunfw.so");
    fs::write(&libkrunfw, "inert firmware fixture").unwrap();

    let config = temporary.path().join("agent.toml");
    fs::write(
      &config,
      format!(
        r#"agent_id = "agent-coverage"
server_url = "{server_url}"
credential_file = "{}"
work_root = "{}"
state_root = "{}"
octa_release_root = "{}"
source_plugins_dir = "{}"
workload_identity_profiles = {{}}
cache = {{ root = "{}", capacity = {{ max_bytes = 1048576, high_watermark_bytes = 943718, low_watermark_bytes = 838860 }}, max_scopes = 4, allow_read = true, allow_write = true, allowed_remote_origins = ["https://cache.example"], native_environment_identities = {{}}, request_timeout_seconds = 10, max_parallel_transfers = 2 }}
maintenance = {{ work_reserve_bytes = 1, state_reserve_bytes = 1, cache_reserve_bytes = 1, disk_check_interval_seconds = 1 }}
enabled_runtime_modes = ["oci"]
allow_native_execution = false
native_linux_pids_limit = 0
allow_unrestricted_network = false
allowed_network_hosts = []
allowed_upload_origins = ["https://objects.example"]
max_archive_entries = 1000
upload_timeout_seconds = 1
max_output_limits = {{ artifact_count = 1, artifact_bytes = 1, report_count = 1, report_bytes = 1, single_output_bytes = 1 }}
max_workspace_bytes = 1048576
max_spool_bytes = 1048576
max_spool_records = 32
event_batch_max_bytes = 16384
event_batch_max_records = 8
event_channel_capacity = 8
poll_timeout_seconds = 1
coordinator_request_timeout_seconds = 1
coordinator_max_body_bytes = 1048576
retry_initial_delay_milliseconds = 1
retry_max_delay_seconds = 1
retry_max_attempts = 1
heartbeat_interval_seconds = 1
lease_safety_margin_seconds = 2
graceful_cancel_timeout_seconds = 1
cleanup_timeout_seconds = 1
runner_hello_timeout_seconds = 1
resource_sample_interval_seconds = 1
resource_sample_timeout_seconds = 1
max_accounting_failures = 1

[server_signing_keys]
primary = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo="

[[oci_engines]]
engine = "microsandbox"
executable = "{}"
libkrunfw = "{}"
metrics_sample_interval_seconds = 1
"#,
        credential.display(),
        work_root.display(),
        state_root.display(),
        release_root.display(),
        source_plugins.display(),
        cache_root.display(),
        executable.display(),
        libkrunfw.display(),
      ),
    )
    .unwrap();
    InstalledAgentFixture {
      _temporary: temporary,
      config,
    }
  }

  /// Serializes tests that construct Microsandbox because its library keeps
  /// one process-global path configuration until the owning graph is dropped.
  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  pub(crate) async fn component_graph_guard() -> tokio::sync::OwnedMutexGuard<()> {
    static GUARD: std::sync::OnceLock<Arc<tokio::sync::Mutex<()>>> = std::sync::OnceLock::new();
    GUARD
      .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
      .clone()
      .lock_owned()
      .await
  }

  #[test]
  fn translates_execution_capabilities_without_changing_isolation() {
    let process = advertised_oci_capability(
      "containerd",
      OciCapability {
        platform: ExecutionPlatform {
          os: ExecutionOs::Windows,
          architecture: ExecutionArchitecture::Amd64,
        },
        isolation: OciIsolation::Process,
      },
    );
    assert_eq!(process.backend, "containerd");
    assert_eq!(process.platform.os, PlatformOs::Windows);
    assert_eq!(process.platform.architecture, PlatformArchitecture::Amd64);
    assert_eq!(process.isolation, Some(octacity_protocol::OciIsolation::Process));

    let hypervisor = advertised_oci_capability(
      "microsandbox",
      OciCapability {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Arm64,
        },
        isolation: OciIsolation::Hypervisor,
      },
    );
    assert_eq!(hypervisor.platform.os, PlatformOs::Linux);
    assert_eq!(hypervisor.platform.architecture, PlatformArchitecture::Arm64);
    assert_eq!(hypervisor.isolation, Some(octacity_protocol::OciIsolation::Hypervisor));
  }

  #[test]
  fn advertises_a_newly_validated_backend_as_ready() {
    assert_eq!(
      ready_backend("native"),
      BackendHealth {
        backend: "native".to_owned(),
        status: BackendHealthStatus::Ready,
        message: None,
      }
    );
  }

  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  #[tokio::test]
  async fn loads_a_complete_microsandbox_component_graph() {
    let _guard = component_graph_guard().await;
    let fixture = installed_agent_fixture("https://coordinator.example");
    let components = Components::load(&fixture.config).await.unwrap();

    assert_eq!(components.source_plugin_count, 0);
    assert_eq!(components.inventory.runtimes.len(), 1);
    assert_eq!(components.inventory.runtimes[0].backend, MICROSANDBOX_ENGINE_NAME);
    assert_eq!(
      components.inventory.runtimes[0].isolation,
      Some(octacity_protocol::OciIsolation::Hypervisor)
    );
    assert!(components.host.capacity().virtualization_available);
    assert_eq!(components.backend_health, vec![ready_backend(MICROSANDBOX_ENGINE_NAME)]);
  }
}
