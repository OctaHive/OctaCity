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
use octacity_changeset::{GitCaptureTool, GitChangeSetCapturer, GitChangeSetMaterializer};
use octacity_config::{
  AgentConfig, ChangeSetCaptureProvider, IsolationProviderConfig, OciEngineConfig, ValidatedConfig,
  ValidatedRuntimeConfig, VirtualizationProviderConfig,
};
use octacity_coordinator::{
  CacheSessionCoordinator, CoordinatorClient, HttpCoordinatorClient, HttpCoordinatorConfig, OutputUploadCoordinator,
  RetryPolicy,
};
use octacity_execution::{ExecutionArchitecture, ExecutionBackend, ExecutionOs, ExecutionPlatform, OciIsolation};
use octacity_execution_apple_vf::{APPLE_VF_PROVIDER_NAME, AppleVfEngine, AppleVfEngineConfig};
use octacity_execution_containerd::{CONTAINERD_ENGINE_NAME, ContainerdEngine, ContainerdEngineConfig};
use octacity_execution_host::{HOST_BACKEND_NAME, HostBackend, HostBackendConfig};
use octacity_execution_microsandbox::{MICROSANDBOX_ENGINE_NAME, MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_native::{LinuxNativeConfig, NATIVE_BACKEND_NAME, NativeBackend};
use octacity_execution_oci::{OciBackend, OciCapability, OciEngine};
use octacity_identity::FileWorkloadIdentityProvider;
use octacity_inventory::{AgentInventoryConfig, HostMonitor, build_inventory, host_platform};
use octacity_job::{
  ExecutionBackendRoute, JobExecutor, JobExecutorConfig, ProtectedInputStager, ProtectedInputStagerConfig,
};
use octacity_output::{OutputPublisher, PresignedOutputPublisher, PresignedOutputPublisherConfig};
use octacity_protocol::{
  AgentInventory, BackendHealth, BackendHealthStatus, EXECUTION_CONTRACT_V3, ExecutionCapabilityV2,
  ExecutionEnvironmentId, ExecutionMode, ExecutionProviderId, FactoryEnforcementCapabilityV3,
  FactoryExecutionCapabilityV3, FactoryImmutableReferenceV3, PlatformArchitecture, PlatformOs, RuntimeCapability,
  RuntimeMode, guarantees_for,
};
use octacity_runner::{
  ConfiguredExternalExecutable, RunnerInstallation, RunnerSupervisionPolicy, VerifiedExternalExecutable,
};
use octacity_source::{SourceMaterializer, SourcePluginRegistry};

use crate::maintenance::WorkspaceCapacityReservation;

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
  pub(crate) workspace_capacity_reservation: WorkspaceCapacityReservation,
  pub(crate) source_plugin_count: usize,
  pub(crate) tool_executables: BTreeMap<String, VerifiedExternalExecutable>,
}

impl Components {
  pub(crate) async fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
    let validated = AgentConfig::load(path)?.validate()?;
    let runner = RunnerInstallation::load(&validated.config.octa_release_root)?;
    let configured_executables = validated
      .config
      .tool_executables
      .iter()
      .map(|(product, executable)| ConfiguredExternalExecutable {
        product: product.clone(),
        version: executable.version.clone(),
        platform: executable.platform.clone(),
        executable: executable.path.clone(),
        sha256: executable.sha256.clone(),
      })
      .collect::<Vec<_>>();
    let tool_executables = runner.verify_external_executables(&configured_executables)?;
    let source_plugins = Arc::new(SourcePluginRegistry::discover(&validated.config.source_plugins_dir)?);
    let source_plugin_count = source_plugins.len();
    let ExecutorAssembly {
      executor,
      runtimes,
      executions,
      factory_executions,
      backend_health,
      cache,
    } = build_executor(
      &validated,
      runner.clone(),
      source_plugins.clone(),
      tool_executables.clone(),
    )
    .await?;
    let workspace_capacity_reservation = workspace_capacity_reservation(&runtimes, &executions);
    let virtualization_available = workspace_capacity_reservation == WorkspaceCapacityReservation::Required;
    let host = HostMonitor::new(
      validated.config.work_root.clone(),
      validated.config.state_root.clone(),
      virtualization_available,
    )?;
    let mut inventory = build_inventory(
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
    inventory.executions = executions;
    if validated.config.factory_permissions.is_some() {
      inventory.factory_executions = factory_executions;
      if !inventory.factory_executions.is_empty() {
        inventory.execution_contract.max = EXECUTION_CONTRACT_V3;
      }
    }
    inventory.validate()?;
    let retry = RetryPolicy {
      max_attempts: validated.config.retry_max_attempts,
      initial_delay: Duration::from_millis(validated.config.retry_initial_delay_milliseconds),
      max_delay: Duration::from_secs(validated.config.retry_max_delay_seconds),
    };
    let http_coordinator = Arc::new(HttpCoordinatorClient::new(HttpCoordinatorConfig {
      server_url: validated.config.server_url.clone(),
      credential_file: validated.config.credential_file.clone(),
      ca_certificate_file: validated.config.tls_ca_certificate_file.clone(),
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
        ca_certificate_file: validated.config.tls_ca_certificate_file.clone(),
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
      workspace_capacity_reservation,
      source_plugin_count,
      tool_executables,
    })
  }
}

struct ExecutorAssembly {
  executor: JobExecutor,
  runtimes: Vec<RuntimeCapability>,
  executions: Vec<ExecutionCapabilityV2>,
  factory_executions: Vec<FactoryExecutionCapabilityV3>,
  backend_health: Vec<BackendHealth>,
  cache: Arc<CacheSessionManager>,
}

#[derive(Default)]
struct BackendAssembly {
  legacy: BTreeMap<RuntimeMode, Arc<dyn ExecutionBackend>>,
  routes: Vec<ExecutionBackendRoute>,
  runtimes: Vec<RuntimeCapability>,
  executions: Vec<ExecutionCapabilityV2>,
  factory_executions: Vec<FactoryExecutionCapabilityV3>,
  health: Vec<BackendHealth>,
}

impl BackendAssembly {
  fn add_runtime(&mut self, capability: RuntimeCapability, backend: Arc<dyn ExecutionBackend>) {
    self.health.push(ready_backend(&capability.backend));
    self.legacy.insert(capability.mode, backend);
    self.runtimes.push(capability);
  }

  fn add_execution(
    &mut self,
    capability: ExecutionCapabilityV2,
    environment_identity: ExecutionEnvironmentId,
    backend: Arc<dyn ExecutionBackend>,
    factory_enforcement: Option<&'static [FactoryEnforcementCapabilityV3]>,
  ) -> Result<(), octacity_job::JobError> {
    let route = ExecutionBackendRoute::new(capability.clone(), environment_identity, backend)?;
    if let Some(capabilities) = factory_enforcement {
      self.factory_executions.push(FactoryExecutionCapabilityV3 {
        execution: capability.clone(),
        enforcement: capabilities.to_vec(),
      });
    }
    self.routes.push(match factory_enforcement {
      Some(capabilities) => route.with_factory_enforcement(capabilities.iter().copied())?,
      None => route,
    });
    self.health.push(ready_execution(&capability));
    self.executions.push(capability);
    Ok(())
  }

  async fn add_legacy_oci(
    &mut self,
    validated: &ValidatedConfig,
    runner: &RunnerInstallation,
    configured: &[OciEngineConfig],
  ) -> Result<(), Box<dyn std::error::Error>> {
    let mut engines: Vec<Arc<dyn OciEngine>> = Vec::new();
    for engine_config in configured {
      let (name, engine): (&str, Arc<dyn OciEngine>) = match engine_config {
        OciEngineConfig::Microsandbox {
          executable,
          libkrunfw,
          metrics_sample_interval_seconds,
        } => (
          MICROSANDBOX_ENGINE_NAME,
          Arc::new(microsandbox_engine(
            validated,
            runner,
            executable,
            libkrunfw,
            *metrics_sample_interval_seconds,
          )?),
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
          let engine = containerd_engine(
            validated,
            ContainerdAssemblyConfig {
              endpoint,
              namespace,
              snapshotter,
              runtime,
              registry_config_dir,
              pids_limit: *pids_limit,
              open_files_limit: *open_files_limit,
            },
          )?;
          engine.validate_connection().await?;
          (CONTAINERD_ENGINE_NAME, Arc::new(engine))
        }
      };
      self.runtimes.extend(
        engine
          .capabilities()
          .into_iter()
          .map(|capability| advertised_oci_capability(name, capability)),
      );
      self.health.push(ready_backend(name));
      engines.push(engine);
    }
    self
      .legacy
      .insert(RuntimeMode::Oci, Arc::new(OciBackend::new(engines)?));
    Ok(())
  }

  async fn add_isolation(
    &mut self,
    validated: &ValidatedConfig,
    runner: &RunnerInstallation,
    providers: &[IsolationProviderConfig],
  ) -> Result<(), Box<dyn std::error::Error>> {
    for provider in providers {
      let (provider_name, environment_identity, engine, isolation_error, factory_enforcement): (
        _,
        _,
        Arc<dyn OciEngine>,
        _,
        Option<&'static [FactoryEnforcementCapabilityV3]>,
      ) = match provider {
        IsolationProviderConfig::Containerd {
          environment_identity,
          endpoint,
          namespace,
          snapshotter,
          runtime,
          registry_config_dir,
          pids_limit,
          open_files_limit,
        } => {
          let engine = Arc::new(containerd_engine(
            validated,
            ContainerdAssemblyConfig {
              endpoint,
              namespace,
              snapshotter,
              runtime,
              registry_config_dir,
              pids_limit: *pids_limit,
              open_files_limit: *open_files_limit,
            },
          )?);
          engine.validate_connection().await?;
          (
            CONTAINERD_ENGINE_NAME,
            environment_identity,
            engine,
            "containerd isolation provider must enforce process isolation",
            Some(&FactoryEnforcementCapabilityV3::ALL),
          )
        }
        IsolationProviderConfig::AppleVf {
          environment_identity,
          executable,
          open_files_limit,
        } => {
          let engine = Arc::new(AppleVfEngine::new(AppleVfEngineConfig {
            agent_id: validated.config.agent_id.clone(),
            executable: executable.clone(),
            state_root: validated.config.state_root.clone(),
            work_root: validated.config.work_root.clone(),
            runner_platform: runner.capabilities.platform.clone(),
            max_workspace_bytes: validated.config.max_workspace_bytes,
            cleanup_timeout: Duration::from_secs(validated.config.cleanup_timeout_seconds),
            open_files_limit: *open_files_limit,
          })?);
          engine.validate_connection().await?;
          (
            APPLE_VF_PROVIDER_NAME,
            environment_identity,
            engine,
            "Apple VF isolation provider must implement the isolation contract",
            None,
          )
        }
      };
      let engine_capability = exactly_one_capability(provider_name, engine.capabilities())?;
      if engine_capability.isolation != OciIsolation::Process {
        return Err(isolation_error.into());
      }
      let capability = execution_capability(provider_name, ExecutionMode::Isolation, engine_capability.platform)?;
      let backend: Arc<dyn ExecutionBackend> = Arc::new(OciBackend::new(vec![engine])?);
      self.add_execution(capability, environment_identity.clone(), backend, factory_enforcement)?;
    }
    Ok(())
  }

  fn add_virtualization(
    &mut self,
    validated: &ValidatedConfig,
    runner: &RunnerInstallation,
    providers: &[VirtualizationProviderConfig],
  ) -> Result<(), Box<dyn std::error::Error>> {
    for provider in providers {
      let VirtualizationProviderConfig::Microsandbox {
        environment_identity,
        executable,
        libkrunfw,
        metrics_sample_interval_seconds,
      } = provider;
      let engine = Arc::new(microsandbox_engine(
        validated,
        runner,
        executable,
        libkrunfw,
        *metrics_sample_interval_seconds,
      )?);
      let engine_capability = exactly_one_capability(MICROSANDBOX_ENGINE_NAME, engine.capabilities())?;
      if engine_capability.isolation != OciIsolation::Hypervisor {
        return Err("Microsandbox virtualization provider must enforce hypervisor isolation".into());
      }
      let capability = execution_capability(
        MICROSANDBOX_ENGINE_NAME,
        ExecutionMode::Virtualization,
        engine_capability.platform,
      )?;
      let backend: Arc<dyn ExecutionBackend> = Arc::new(OciBackend::new(vec![engine])?);
      self.add_execution(
        capability,
        environment_identity.clone(),
        backend,
        Some(&FactoryEnforcementCapabilityV3::ALL),
      )?;
    }
    Ok(())
  }
}

async fn build_executor(
  validated: &ValidatedConfig,
  runner: RunnerInstallation,
  source_plugins: Arc<SourcePluginRegistry>,
  tool_executables: BTreeMap<String, VerifiedExternalExecutable>,
) -> Result<ExecutorAssembly, Box<dyn std::error::Error>> {
  #[cfg(unix)]
  if validated.runtimes.iter().any(|runtime| match runtime {
    ValidatedRuntimeConfig::Host { .. } => false,
    ValidatedRuntimeConfig::Native { .. } => true,
    ValidatedRuntimeConfig::Oci { engines } => engines
      .iter()
      .any(|engine| matches!(engine, OciEngineConfig::Containerd { .. })),
    ValidatedRuntimeConfig::Isolation { providers } => !providers.is_empty(),
    ValidatedRuntimeConfig::Virtualization { .. } => false,
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
  let mut assembly = BackendAssembly::default();
  for runtime in &validated.runtimes {
    match runtime {
      ValidatedRuntimeConfig::Host {
        environment_identity,
        environment,
      } => {
        let backend = Arc::new(HostBackend::new(HostBackendConfig {
          work_root: validated.config.work_root.clone(),
          runner_platform: runner.capabilities.platform.clone(),
          environment: environment.clone(),
          cleanup_timeout: Duration::from_secs(validated.config.cleanup_timeout_seconds),
          max_accounted_workspace_entries: validated.config.host_accounting_max_entries,
        })?);
        let platform = host_platform()?;
        let capability = ExecutionCapabilityV2 {
          provider: ExecutionProviderId::new(HOST_BACKEND_NAME).map_err(octacity_execution::ExecutionError::Invalid)?,
          mode: ExecutionMode::Host,
          host_platform: platform,
          target_platform: platform,
          guarantees: guarantees_for(ExecutionMode::Host),
          immutable_images: false,
        };
        assembly.add_execution(capability, environment_identity.clone(), backend, None)?;
      }
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
        assembly.add_runtime(
          RuntimeCapability {
            backend: NATIVE_BACKEND_NAME.to_owned(),
            mode: RuntimeMode::Native,
            platform: host_platform()?,
            isolation: None,
          },
          backend,
        );
      }
      ValidatedRuntimeConfig::Oci { engines } => assembly.add_legacy_oci(validated, &runner, engines).await?,
      ValidatedRuntimeConfig::Isolation { providers } => {
        assembly.add_isolation(validated, &runner, providers).await?;
      }
      ValidatedRuntimeConfig::Virtualization { providers } => {
        assembly.add_virtualization(validated, &runner, providers)?;
      }
    }
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
    assembly.legacy,
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
      external_executables: tool_executables,
      factory_permissions: validated.config.factory_permissions.clone(),
    },
  )?
  .with_execution_backends(assembly.routes)?
  .with_cache(cache.clone());
  let executor = if let Some(capture) = &validated.config.change_set_capture {
    let git = &capture.executable;
    let tool = GitCaptureTool {
      executable: git.path.clone(),
      identity: FactoryImmutableReferenceV3 {
        identity: "git".to_owned(),
        version: git.version.clone(),
        sha256: git.sha256.clone(),
      },
    };
    match capture.provider {
      ChangeSetCaptureProvider::Git => executor
        .with_change_set_capturer(Arc::new(GitChangeSetCapturer::new(tool.clone())?))
        .with_change_set_materializer(Arc::new(GitChangeSetMaterializer::new(tool)?)),
    }
  } else {
    executor
  };
  let executor = if validated.config.factory_permissions.is_some() {
    executor.with_protected_input_stager(ProtectedInputStager::new(ProtectedInputStagerConfig {
      allowed_origins: validated.config.allowed_upload_origins.clone(),
      download_timeout: Duration::from_secs(validated.config.upload_timeout_seconds),
    })?)
  } else {
    executor
  };
  Ok(ExecutorAssembly {
    executor,
    runtimes: assembly.runtimes,
    executions: assembly.executions,
    factory_executions: assembly.factory_executions,
    backend_health: assembly.health,
    cache,
  })
}

struct ContainerdAssemblyConfig<'a> {
  endpoint: &'a Path,
  namespace: &'a str,
  snapshotter: &'a str,
  runtime: &'a str,
  registry_config_dir: &'a Option<std::path::PathBuf>,
  pids_limit: u32,
  open_files_limit: u64,
}

fn microsandbox_engine(
  validated: &ValidatedConfig,
  runner: &RunnerInstallation,
  executable: &Path,
  libkrunfw: &Path,
  metrics_sample_interval_seconds: u64,
) -> Result<MicrosandboxEngine, octacity_execution::ExecutionError> {
  MicrosandboxEngine::new(MicrosandboxEngineConfig {
    agent_id: validated.config.agent_id.clone(),
    state_root: validated.config.state_root.clone(),
    work_root: validated.config.work_root.clone(),
    runner_platform: runner.capabilities.platform.clone(),
    executable: executable.to_owned(),
    libkrunfw: libkrunfw.to_owned(),
    cleanup_timeout: Duration::from_secs(validated.config.cleanup_timeout_seconds),
    metrics_sample_interval: Duration::from_secs(metrics_sample_interval_seconds),
  })
}

fn containerd_engine(
  validated: &ValidatedConfig,
  config: ContainerdAssemblyConfig<'_>,
) -> Result<ContainerdEngine, octacity_execution::ExecutionError> {
  ContainerdEngine::new(ContainerdEngineConfig {
    agent_id: validated.config.agent_id.clone(),
    endpoint: config.endpoint.to_owned(),
    namespace: config.namespace.to_owned(),
    snapshotter: config.snapshotter.to_owned(),
    runtime: config.runtime.to_owned(),
    registry_config_dir: config.registry_config_dir.clone(),
    state_root: validated.config.state_root.clone(),
    work_root: validated.config.work_root.clone(),
    max_workspace_bytes: validated.config.max_workspace_bytes,
    cleanup_timeout: Duration::from_secs(validated.config.cleanup_timeout_seconds),
    pids_limit: config.pids_limit,
    open_files_limit: config.open_files_limit,
  })
}

fn exactly_one_capability(
  provider: &str,
  capabilities: Vec<OciCapability>,
) -> Result<OciCapability, octacity_execution::ExecutionError> {
  let [capability]: [OciCapability; 1] = capabilities.try_into().map_err(|capabilities: Vec<_>| {
    octacity_execution::ExecutionError::Invalid(format!(
      "{provider} execution provider must advertise exactly one capability, got {}",
      capabilities.len()
    ))
  })?;
  Ok(capability)
}

fn execution_capability(
  provider: &str,
  mode: ExecutionMode,
  target_platform: ExecutionPlatform,
) -> Result<ExecutionCapabilityV2, Box<dyn std::error::Error>> {
  Ok(ExecutionCapabilityV2 {
    provider: ExecutionProviderId::new(provider).map_err(octacity_execution::ExecutionError::Invalid)?,
    mode,
    host_platform: host_platform()?,
    target_platform: protocol_platform(target_platform),
    guarantees: guarantees_for(mode),
    immutable_images: true,
  })
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
    execution: None,
    status: BackendHealthStatus::Ready,
    message: None,
  }
}

fn ready_execution(capability: &ExecutionCapabilityV2) -> BackendHealth {
  BackendHealth {
    backend: capability.provider.to_string(),
    execution: Some(capability.clone()),
    status: BackendHealthStatus::Ready,
    message: None,
  }
}

fn workspace_capacity_reservation(
  runtimes: &[RuntimeCapability],
  executions: &[ExecutionCapabilityV2],
) -> WorkspaceCapacityReservation {
  if runtimes
    .iter()
    .any(|capability| capability.isolation == Some(octacity_protocol::OciIsolation::Hypervisor))
    || executions
      .iter()
      .any(|capability| capability.mode == ExecutionMode::Virtualization)
  {
    WorkspaceCapacityReservation::Required
  } else {
    WorkspaceCapacityReservation::Preallocated
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
    agent_fixture(server_url, true)
  }

  /// Creates an inventory-only installation for daemon lifecycle tests that
  /// must not exercise a concrete execution provider.
  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  pub(crate) fn inventory_only_agent_fixture(server_url: &str) -> InstalledAgentFixture {
    agent_fixture(server_url, false)
  }

  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  fn agent_fixture(server_url: &str, include_microsandbox: bool) -> InstalledAgentFixture {
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

    let virtualization_provider = include_microsandbox.then(|| {
      format!(
        r#"
[[virtualization_providers]]
provider = "microsandbox"
environment_identity = "microsandbox-linux-guest-v1"
executable = "{}"
libkrunfw = "{}"
metrics_sample_interval_seconds = 1
"#,
        executable.display(),
        libkrunfw.display(),
      )
    });

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
enabled_runtime_modes = []
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
{}
"#,
        credential.display(),
        work_root.display(),
        state_root.display(),
        release_root.display(),
        source_plugins.display(),
        cache_root.display(),
        virtualization_provider.as_deref().unwrap_or_default(),
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
    assert_eq!(
      workspace_capacity_reservation(std::slice::from_ref(&process), &[]),
      WorkspaceCapacityReservation::Preallocated
    );

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
    assert_eq!(
      workspace_capacity_reservation(&[process, hypervisor], &[]),
      WorkspaceCapacityReservation::Required
    );
  }

  #[test]
  fn advertises_a_newly_validated_backend_as_ready() {
    assert_eq!(
      ready_backend("native"),
      BackendHealth {
        backend: "native".to_owned(),
        execution: None,
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
    assert!(components.tool_executables.is_empty());
    assert_eq!(
      components.workspace_capacity_reservation,
      WorkspaceCapacityReservation::Required
    );
    assert!(components.inventory.runtimes.is_empty());
    assert_eq!(components.inventory.executions.len(), 1);
    let capability = &components.inventory.executions[0];
    assert_eq!(capability.provider.as_str(), MICROSANDBOX_ENGINE_NAME);
    assert_eq!(capability.mode, ExecutionMode::Virtualization);
    assert_eq!(capability.guarantees, guarantees_for(ExecutionMode::Virtualization));
    assert_eq!(capability.host_platform, host_platform().unwrap());
    assert_eq!(capability.target_platform.os, PlatformOs::Linux);
    assert!(components.host.capacity().virtualization_available);
    assert_eq!(components.backend_health, vec![ready_execution(capability)]);
  }
}
