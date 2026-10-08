//! Real execution-backend contract tests for a provisioned host.
//!
//! These tests are ignored in the portable workspace suite because they need
//! an operator-created cgroup/filesystem boundary or microVM runtime. Running
//! any test explicitly is strict: missing configuration or backend support is
//! a failure, never a runtime skip.

use std::{
  collections::BTreeMap,
  env,
  path::{Path, PathBuf},
  sync::Arc,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use octacity_cache_session::{CacheSessionManager, CacheSessionManagerConfig};
use octacity_execution::{ExecutionBackend, LocalCacheCapacity};
use octacity_execution_apple_vf::{APPLE_VF_PROVIDER_NAME, AppleVfEngine, AppleVfEngineConfig};
use octacity_execution_containerd::{ContainerdEngine, ContainerdEngineConfig};
use octacity_execution_host::{HOST_BACKEND_NAME, HostBackend, HostBackendConfig};
use octacity_execution_microsandbox::{MICROSANDBOX_ENGINE_NAME, MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_native::{LinuxNativeConfig, NativeBackend};
use octacity_execution_oci::{OciBackend, OciEngine};
use octacity_identity::FileWorkloadIdentityProvider;
use octacity_job::{
  ExecuteJobRequest, ExecutionBackendRoute, FactoryPreflightError, JobError, JobExecutor, JobExecutorConfig,
  ProtectedInputStager, ProtectedInputStagerConfig,
};
use octacity_protocol::{
  AGENT_PROTOCOL_VERSION, ArtifactTransferCapability, BeginCacheSessionResponse, CachePolicy, EXECUTION_CONTRACT_V2,
  EXECUTION_CONTRACT_V3, ExecutionCapabilityV2, ExecutionEnvironmentId, ExecutionMode, ExecutionProviderId,
  ExecutionSpec, ExecutionTargetV2, FactoryEnforcementCapabilityV3, FactoryImmutableReferenceV3, FactoryMountModeV3,
  FactoryMountPermissionV3, FactoryOutputPermissionsV3, FactoryPermissionSetV3, FactoryResourceLimitsV3, JobSpecV1,
  JobSpecV2, JobSpecV3, ManagedOctaExecutionV3, NetworkPolicy, OciIsolation, OctaSpec, OutputLimits,
  PlatformArchitecture, PlatformOs, PlatformSpec, ProtectedInputManifestV3, ProtectedInputTransferV3, ProtectedInputV3,
  RemoteCacheGrant, RuntimeMode, RuntimeSpec, RuntimeSpecV2, RuntimeTarget, SourceSpec, VerifiedJobSpec,
  guarantees_for,
};
use octacity_runner::{RunStatus, RunnerInstallation, RunnerStreamItem, RunnerSupervisionPolicy};
use octacity_source::{MaterializedSource, SourceError, SourceMaterializationRequest, SourceMaterializer};
use sha2::{Digest as _, Sha256};
use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  sync::mpsc,
};
use tokio_util::sync::CancellationToken;

const FIXTURE_OCTAFILE: &str =
  "version: 1\n\ntasks:\n  contract:\n    shell: echo octacity-backend-contract\n  wait:\n    shell: sleep 60\n";
const SOURCE_DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn expected_microsandbox_runner_platform() -> &'static str {
  if cfg!(target_arch = "aarch64") {
    "linux-aarch64"
  } else {
    "linux-x86_64"
  }
}

struct FixtureSource {
  oci_probe: Option<OciProbe>,
}

#[derive(Clone)]
enum OciProbe {
  DisabledNetwork,
  RestrictedNetwork { allowed_host: String, denied_host: String },
}

#[async_trait]
impl SourceMaterializer for FixtureSource {
  fn verify(&self, _requirement: &SourceSpec) -> Result<(), SourceError> {
    Ok(())
  }

  async fn materialize(
    &self,
    requirement: &SourceSpec,
    request: SourceMaterializationRequest,
    _operation_timeout: Duration,
    _cancellation_grace: Duration,
    _cancellation: CancellationToken,
  ) -> Result<MaterializedSource, SourceError> {
    let octafile = match &self.oci_probe {
      Some(OciProbe::RestrictedNetwork {
        allowed_host,
        denied_host,
      }) => format!(
        "version: 1\n\ntasks:\n  contract:\n    shell: |\n      test \"$(cat /run/octa-identity)\" = backend-contract-identity\n      ! printf tamper >> /run/octa-identity\n      command -v curl > /dev/null\n      curl --fail --silent --show-error --max-time 10 https://{allowed_host}/ > /dev/null\n      ! curl --fail --silent --show-error --max-time 3 https://{denied_host}/ > /dev/null 2>&1\n      echo octacity-backend-contract\n  wait:\n    shell: sleep 60\n"
      ),
      Some(OciProbe::DisabledNetwork) => {
        "version: 1\n\ntasks:\n  contract:\n    shell: |\n      test \"$(cat /run/octa-identity)\" = backend-contract-identity\n      ! printf tamper >> /run/octa-identity\n      command -v curl > /dev/null\n      ! curl --fail --silent --show-error --max-time 3 https://example.com/ > /dev/null 2>&1\n      echo octacity-backend-contract\n  wait:\n    shell: sleep 60\n"
          .to_owned()
      }
      None => FIXTURE_OCTAFILE.to_owned(),
    };
    std::fs::write(request.destination.join("Octafile.yml"), octafile).map_err(|source| {
      SourceError::Host(octacity_source::SourceHostError::Io {
        plugin: "backend-contract-fixture".to_owned(),
        source,
      })
    })?;
    Ok(MaterializedSource {
      revision: requirement.revision.clone(),
      provenance: BTreeMap::from([("fixture".to_owned(), "backend-contract-v1".to_owned())]),
      progress: Vec::new(),
      diagnostics: Vec::new(),
    })
  }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a delegated cgroup v2 root and quota-mounted work root; see docs/testing/backend-contracts.md"]
async fn native_backend_satisfies_the_real_runner_contract() {
  let work_root = required_path("OCTACITY_CONTRACT_NATIVE_WORK_ROOT");
  let cgroup_root = required_path("OCTACITY_CONTRACT_NATIVE_CGROUP_ROOT");
  let bubblewrap = required_absolute_path("OCTACITY_CONTRACT_NATIVE_BWRAP");
  let workspace_bytes = required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES");
  let path = required_string("OCTACITY_CONTRACT_NATIVE_PATH");
  let backend = Arc::new(
    NativeBackend::new(LinuxNativeConfig {
      cgroup_root,
      work_root: work_root.clone(),
      bubblewrap,
      readonly_paths: Vec::new(),
      runner_platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
      max_workspace_bytes: workspace_bytes,
      cleanup_timeout: Duration::from_secs(10),
      pids_limit: 4096,
      environment: BTreeMap::from([("PATH".to_owned(), path)]),
    })
    .expect("Native backend configuration must be usable"),
  );
  run_contract(
    RuntimeTarget::Native {
      platform: linux_platform(),
    },
    work_root,
    workspace_bytes,
    backend,
    None,
    true,
  )
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a released Octa host bundle; exercised on Linux, macOS, and Windows in backend-contract CI"]
async fn host_backend_satisfies_the_real_runner_contract() {
  let work_root = required_path("OCTACITY_CONTRACT_HOST_WORK_ROOT");
  let workspace_bytes = required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES");
  let release_root = required_path("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT");
  let runner = RunnerInstallation::load(&release_root).expect("the configured Octa release must be valid");
  let platform = host_platform();
  let capability = ExecutionCapabilityV2 {
    provider: ExecutionProviderId::new(HOST_BACKEND_NAME).unwrap(),
    mode: ExecutionMode::Host,
    host_platform: platform,
    target_platform: platform,
    guarantees: guarantees_for(ExecutionMode::Host),
    immutable_images: false,
  };
  let backend = Arc::new(
    HostBackend::new(HostBackendConfig {
      work_root: work_root.clone(),
      runner_platform: runner.capabilities.platform.clone(),
      environment: host_environment(),
      cleanup_timeout: Duration::from_secs(10),
      max_accounted_workspace_entries: 100_000,
    })
    .expect("Host backend configuration must be usable"),
  );
  let executor = JobExecutor::new(
    runner.clone(),
    Arc::new(FixtureSource { oci_probe: None }),
    Arc::new(FileWorkloadIdentityProvider::default()),
    BTreeMap::new(),
    executor_config(work_root, workspace_bytes, true, Vec::new()),
  )
  .expect("job executor configuration must be valid")
  .with_execution_backends([ExecutionBackendRoute::new(
    capability,
    ExecutionEnvironmentId::new(required_string("OCTACITY_CONTRACT_HOST_ENVIRONMENT_ID")).unwrap(),
    backend,
  )
  .unwrap()])
  .expect("Host execution route must be valid");
  executor.cleanup_orphans().await.expect("pre-test cleanup must succeed");
  let now = unix_now();
  let spec = host_specification(platform, workspace_bytes, now, &runner);
  exercise_contract(
    &executor,
    spec.into(),
    None,
    Some((HOST_BACKEND_NAME, ExecutionMode::Host)),
    "Host",
  )
  .await;
  executor
    .cleanup_orphans()
    .await
    .expect("post-test cleanup must succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Linux/KVM, Apple Silicon macOS, or Windows/WHP and a provisioned Microsandbox image; see docs/testing/backend-contracts.md"]
async fn microsandbox_backend_satisfies_the_real_runner_contract() {
  let work_root = required_path("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT");
  let state_root = required_path("OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT");
  let workspace_bytes = required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES");
  let image = required_string("OCTACITY_CONTRACT_MICROSANDBOX_IMAGE");
  let engine: Arc<dyn OciEngine> = Arc::new(
    MicrosandboxEngine::new(MicrosandboxEngineConfig {
      agent_id: "backend-contract".to_owned(),
      state_root,
      work_root: work_root.clone(),
      runner_platform: expected_microsandbox_runner_platform().to_owned(),
      executable: required_absolute_path("OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE"),
      libkrunfw: required_absolute_path("OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW"),
      cleanup_timeout: Duration::from_secs(10),
      metrics_sample_interval: Duration::from_secs(1),
    })
    .expect("Microsandbox backend configuration must be usable"),
  );
  run_oci_v2_contract(OciV2Contract {
    engine,
    provider: MICROSANDBOX_ENGINE_NAME,
    mode: ExecutionMode::Virtualization,
    environment_identity_variable: "OCTACITY_CONTRACT_MICROSANDBOX_ENVIRONMENT_IDENTITY",
    work_root,
    workspace_bytes,
    image,
    host_platform: host_platform(),
    target_platform: linux_platform(),
    probe: OciProbe::RestrictedNetwork {
      allowed_host: required_network_host("OCTACITY_CONTRACT_MICROSANDBOX_ALLOWED_HOST"),
      denied_host: required_network_host("OCTACITY_CONTRACT_MICROSANDBOX_DENIED_HOST"),
    },
    factory_enforcement: Some(&FactoryEnforcementCapabilityV3::ALL),
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Linux containerd, runc/crun, cgroup v2, and a quota-mounted work root; see docs/testing/backend-contracts.md"]
async fn containerd_isolation_provider_satisfies_the_real_runner_contract() {
  let work_root = required_path("OCTACITY_CONTRACT_CONTAINERD_WORK_ROOT");
  let state_root = required_path("OCTACITY_CONTRACT_CONTAINERD_STATE_ROOT");
  let workspace_bytes = required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES");
  let image = required_string("OCTACITY_CONTRACT_CONTAINERD_IMAGE");
  let registry_config_dir = env::var_os("OCTACITY_CONTRACT_CONTAINERD_REGISTRY_CONFIG_DIR").map(PathBuf::from);
  let engine: Arc<dyn OciEngine> = Arc::new(
    ContainerdEngine::new(ContainerdEngineConfig {
      agent_id: "backend-contract".to_owned(),
      endpoint: required_absolute_path("OCTACITY_CONTRACT_CONTAINERD_ENDPOINT"),
      namespace: required_string("OCTACITY_CONTRACT_CONTAINERD_NAMESPACE"),
      snapshotter: required_string("OCTACITY_CONTRACT_CONTAINERD_SNAPSHOTTER"),
      runtime: required_string("OCTACITY_CONTRACT_CONTAINERD_RUNTIME"),
      registry_config_dir,
      state_root,
      work_root: work_root.clone(),
      max_workspace_bytes: workspace_bytes,
      cleanup_timeout: Duration::from_secs(10),
      pids_limit: 4096,
      open_files_limit: 65536,
    })
    .expect("containerd engine configuration must be usable"),
  );
  let platform = linux_platform();
  run_oci_v2_contract(OciV2Contract {
    engine,
    provider: "containerd",
    mode: ExecutionMode::Isolation,
    environment_identity_variable: "OCTACITY_CONTRACT_CONTAINERD_ENVIRONMENT_IDENTITY",
    work_root,
    workspace_bytes,
    image,
    host_platform: platform,
    target_platform: platform,
    probe: OciProbe::DisabledNetwork,
    factory_enforcement: Some(&FactoryEnforcementCapabilityV3::ALL),
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Apple Silicon macOS 26+, Apple container 0.6+, and a quota-backed APFS work root"]
async fn apple_vf_isolation_provider_satisfies_the_real_runner_contract() {
  assert_eq!(std::env::consts::OS, "macos", "Apple VF contract requires macOS");
  assert_eq!(std::env::consts::ARCH, "aarch64", "Apple VF contract requires ARM64");
  let work_root = required_path("OCTACITY_CONTRACT_APPLE_VF_WORK_ROOT");
  let state_root = required_path("OCTACITY_CONTRACT_APPLE_VF_STATE_ROOT");
  let workspace_bytes = required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES");
  let image = required_string("OCTACITY_CONTRACT_APPLE_VF_IMAGE");
  let engine = Arc::new(
    AppleVfEngine::new(AppleVfEngineConfig {
      agent_id: "backend-contract".to_owned(),
      executable: required_absolute_path("OCTACITY_CONTRACT_APPLE_VF_EXECUTABLE"),
      state_root,
      work_root: work_root.clone(),
      runner_platform: "linux-aarch64".to_owned(),
      max_workspace_bytes: workspace_bytes,
      cleanup_timeout: Duration::from_secs(10),
      open_files_limit: 65536,
    })
    .expect("Apple VF engine configuration must be usable"),
  );
  engine
    .validate_connection()
    .await
    .expect("Apple container service must be running");
  run_oci_v2_contract(OciV2Contract {
    engine,
    provider: APPLE_VF_PROVIDER_NAME,
    mode: ExecutionMode::Isolation,
    environment_identity_variable: "OCTACITY_CONTRACT_APPLE_VF_ENVIRONMENT_IDENTITY",
    work_root,
    workspace_bytes,
    image,
    host_platform: host_platform(),
    target_platform: linux_platform(),
    probe: OciProbe::DisabledNetwork,
    factory_enforcement: None,
  })
  .await;
}

struct OciV2Contract {
  engine: Arc<dyn OciEngine>,
  provider: &'static str,
  mode: ExecutionMode,
  environment_identity_variable: &'static str,
  work_root: PathBuf,
  workspace_bytes: u64,
  image: String,
  host_platform: PlatformSpec,
  target_platform: PlatformSpec,
  probe: OciProbe,
  factory_enforcement: Option<&'static [FactoryEnforcementCapabilityV3]>,
}

async fn run_oci_v2_contract(contract: OciV2Contract) {
  let OciV2Contract {
    engine,
    provider,
    mode,
    environment_identity_variable,
    work_root,
    workspace_bytes,
    image,
    host_platform,
    target_platform,
    probe,
    factory_enforcement,
  } = contract;
  assert!(
    matches!(mode, ExecutionMode::Isolation | ExecutionMode::Virtualization),
    "OCI-backed v2 contracts must provide isolation or virtualization"
  );
  let backend = Arc::new(OciBackend::new(vec![engine]).expect("OCI routing must be valid"));
  let capability = ExecutionCapabilityV2 {
    provider: ExecutionProviderId::new(provider).unwrap(),
    mode,
    host_platform,
    target_platform,
    guarantees: guarantees_for(mode),
    immutable_images: true,
  };
  let release_root = required_path("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT");
  let runner = RunnerInstallation::load(&release_root).expect("the configured Octa release must be valid");
  let identity_source = tempfile::NamedTempFile::new().expect("identity fixture must be creatable");
  std::fs::write(identity_source.path(), "backend-contract-identity").expect("identity fixture must be writable");
  let identity = FileWorkloadIdentityProvider::new(BTreeMap::from([(
    "backend-contract".to_owned(),
    identity_source.path().to_owned(),
  )]));
  let now = unix_now();
  let mut spec = oci_v2_specification(
    mode,
    host_platform,
    target_platform,
    image,
    workspace_bytes,
    now,
    &runner,
  );
  if let OciProbe::RestrictedNetwork { allowed_host, .. } = &probe {
    spec.runtime.network = NetworkPolicy::Restricted {
      allowed_hosts: vec![allowed_host.clone()],
    };
  }
  spec.runtime.workload_identity_profile = Some("backend-contract".to_owned());
  let factory_setup = if factory_enforcement.is_some() {
    let factory_body = factory_octafile(&probe);
    let factory_spec = oci_v3_specification(&spec, &runner, &factory_body);
    let (protected_origin, protected_server) = protected_input_server(factory_body.clone()).await;
    Some((factory_spec, factory_body, protected_origin, protected_server))
  } else {
    None
  };
  let mut configuration = executor_config(work_root, workspace_bytes, false, Vec::new());
  if provider == APPLE_VF_PROVIDER_NAME {
    configuration.runner_supervision.resource_sample_timeout = Duration::from_secs(5);
  }
  configuration.factory_permissions = factory_setup.as_ref().map(|setup| setup.0.permissions.clone());
  let route = ExecutionBackendRoute::new(
    capability,
    ExecutionEnvironmentId::new(required_string(environment_identity_variable)).unwrap(),
    backend,
  )
  .unwrap();
  let route = match factory_enforcement {
    Some(capabilities) => route.with_factory_enforcement(capabilities.iter().copied()).unwrap(),
    None => route,
  };
  let executor = JobExecutor::new(
    runner.clone(),
    Arc::new(FixtureSource {
      oci_probe: Some(probe.clone()),
    }),
    Arc::new(identity),
    BTreeMap::new(),
    JobExecutorConfig {
      allowed_network_hosts: match &probe {
        OciProbe::RestrictedNetwork { allowed_host, .. } => vec![allowed_host.clone()],
        OciProbe::DisabledNetwork => Vec::new(),
      },
      ..configuration
    },
  )
  .expect("job executor configuration must be valid")
  .with_execution_backends([route])
  .expect("v2/v3 OCI execution route must be valid");
  let executor = if let Some((_, _, protected_origin, _)) = &factory_setup {
    executor.with_protected_input_stager(
      ProtectedInputStager::new(ProtectedInputStagerConfig {
        allowed_origins: vec![protected_origin.clone()],
        download_timeout: Duration::from_secs(10),
      })
      .unwrap(),
    )
  } else {
    executor
  };
  executor.cleanup_orphans().await.expect("pre-test cleanup must succeed");
  exercise_contract(
    &executor,
    spec.into(),
    None,
    Some((provider, mode)),
    &format!("{provider} {mode:?}"),
  )
  .await;
  if let Some((factory_spec, factory_body, protected_origin, protected_server)) = factory_setup {
    exercise_factory_negative_contracts(
      &executor,
      &factory_spec,
      protected_transfer(&factory_body, &protected_origin),
    )
    .await;
    exercise_factory_contract(
      &executor,
      factory_spec,
      protected_transfer(&factory_body, &protected_origin),
    )
    .await;
    protected_server.await.unwrap();
  }
  executor
    .cleanup_orphans()
    .await
    .expect("post-test cleanup must succeed");
}

async fn run_contract(
  target: RuntimeTarget,
  work_root: PathBuf,
  workspace_bytes: u64,
  backend: Arc<dyn ExecutionBackend>,
  oci_probe: Option<OciProbe>,
  native_cache_contract: bool,
) {
  let mode = match &target {
    RuntimeTarget::Native { .. } => RuntimeMode::Native,
    RuntimeTarget::Oci { .. } => RuntimeMode::Oci,
  };
  let release_root = required_path("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT");
  let runner = RunnerInstallation::load(&release_root).expect("the configured Octa release must be valid");
  let identity_source = oci_probe.as_ref().map(|_| {
    let source = tempfile::NamedTempFile::new().expect("identity fixture must be creatable");
    std::fs::write(source.path(), "backend-contract-identity").expect("identity fixture must be writable");
    source
  });
  let identity = identity_source
    .as_ref()
    .map_or_else(FileWorkloadIdentityProvider::default, |source| {
      FileWorkloadIdentityProvider::new(BTreeMap::from([(
        "backend-contract".to_owned(),
        source.path().to_owned(),
      )]))
    });
  let executor = JobExecutor::new(
    runner.clone(),
    Arc::new(FixtureSource {
      oci_probe: oci_probe.clone(),
    }),
    Arc::new(identity),
    BTreeMap::from([(mode, backend)]),
    executor_config(
      work_root,
      workspace_bytes,
      native_cache_contract,
      match &oci_probe {
        Some(OciProbe::RestrictedNetwork { allowed_host, .. }) => vec![allowed_host.clone()],
        Some(OciProbe::DisabledNetwork) | None => Vec::new(),
      },
    ),
  )
  .expect("job executor configuration must be valid");
  let executor = if native_cache_contract {
    let cache_root = required_path("OCTACITY_RELEASE_NATIVE_CACHE_ROOT");
    let scope_bytes = workspace_bytes / 2;
    let cache = CacheSessionManager::new(CacheSessionManagerConfig {
      root: cache_root,
      capacity: LocalCacheCapacity::new(scope_bytes, scope_bytes * 9 / 10, scope_bytes * 8 / 10).unwrap(),
      max_scopes: 2,
      allow_read: true,
      allow_write: true,
      allowed_origins: vec!["https://cache.example".to_owned()],
      ca_certificate_file: None,
      native_identities: BTreeMap::from([(linux_platform().to_string(), "backend-contract-v1".to_owned())]),
      request_timeout_seconds: 5,
      max_parallel_transfers: 1,
    })
    .expect("Native cache configuration must be usable");
    executor.with_cache(Arc::new(cache))
  } else {
    executor
  };
  executor
    .cleanup_orphans()
    .await
    .expect("pre-test orphan cleanup must succeed");

  let now = unix_now();
  let mut spec = specification(target.clone(), workspace_bytes, now, &runner);
  if native_cache_contract {
    spec.runtime.network = NetworkPolicy::Unrestricted;
    spec.cache = Some(CachePolicy {
      namespace: "backend-contract".to_owned(),
      read: true,
      write: true,
    });
  }
  if let Some(OciProbe::RestrictedNetwork { allowed_host, .. }) = &oci_probe {
    spec.runtime.network = NetworkPolicy::Restricted {
      allowed_hosts: vec![allowed_host.clone()],
    };
  }
  if oci_probe.is_some() {
    spec.runtime.workload_identity_profile = Some("backend-contract".to_owned());
  }
  let cache_grant = native_cache_contract.then(|| BeginCacheSessionResponse {
    protocol_version: 1,
    request_id: "backend-contract-cache".to_owned(),
    session_id: "backend-contract-session".to_owned(),
    scope_id: "backend-contract-scope".to_owned(),
    remote: Some(RemoteCacheGrant {
      endpoint: "https://cache.example".to_owned(),
      bearer_token: "backend-contract-token".to_owned(),
      expires_at: now + 300,
    }),
  });
  exercise_contract(&executor, spec.into(), cache_grant, None, &format!("{mode:?}")).await;
  executor
    .cleanup_orphans()
    .await
    .expect("post-test backend cleanup must find no leaked state");
}

async fn exercise_contract(
  executor: &JobExecutor,
  spec: VerifiedJobSpec,
  cache_grant: Option<BeginCacheSessionResponse>,
  expected_execution: Option<(&str, ExecutionMode)>,
  label: &str,
) {
  let cancellation_spec = cancellation_spec(spec.clone());
  let (sender, mut receiver) = mpsc::channel(128);
  let started = Instant::now();
  let completion = executor
    .execute(
      ExecuteJobRequest {
        spec,
        source_credentials: BTreeMap::new(),
        cache_grant,
        protected_inputs: Vec::new(),
      },
      CancellationToken::new(),
      &sender,
    )
    .await
    .expect("real backend execution must succeed");
  drop(sender);
  let mut saw_event = false;
  while let Some(item) = receiver.recv().await {
    saw_event |= matches!(item, RunnerStreamItem::Event(_));
  }

  assert_eq!(completion.runner().status, RunStatus::Succeeded);
  match expected_execution {
    Some((provider, mode)) => {
      let evidence = completion
        .execution()
        .expect("v2 execution must retain provider evidence");
      assert_eq!(evidence.provider.as_str(), provider);
      assert_eq!(evidence.target.mode, mode);
      assert_eq!(evidence.target.required_guarantees, guarantees_for(mode));
      if mode == ExecutionMode::Host {
        assert_eq!(evidence.target.host_platform, evidence.target.target_platform);
      } else {
        assert!(evidence.target.immutable_image.is_some());
      }
    }
    None => assert!(completion.execution().is_none()),
  }
  assert!(saw_event, "the real runner must emit at least one structured event");
  let usage = &completion.runner().final_usage;
  assert!(usage.memory_peak_bytes > 0);
  assert!(usage.memory_peak_bytes >= usage.memory_current_bytes);
  assert!(usage.disk_peak_bytes >= usage.disk_current_bytes);
  assert_eq!(
    usage.network_received_bytes.is_some(),
    usage.network_transmitted_bytes.is_some(),
    "network counters must be reported as a complete pair"
  );
  assert!(completion.workspace().join("Octafile.yml").is_file());
  let job_root = completion
    .workspace()
    .parent()
    .expect("workspace must have a job root")
    .to_owned();
  completion.cleanup().await.expect("job workspace cleanup must succeed");
  assert!(!job_root.exists(), "job filesystem state must be removed");
  run_cancellation_contract(executor, cancellation_spec).await;
  eprintln!("{label} real backend contract completed in {:?}", started.elapsed());
}

async fn run_cancellation_contract(executor: &JobExecutor, spec: VerifiedJobSpec) {
  let cancellation = CancellationToken::new();
  let (sender, mut receiver) = mpsc::channel(128);
  let execution = executor.execute(
    ExecuteJobRequest {
      spec,
      source_credentials: BTreeMap::new(),
      cache_grant: None,
      protected_inputs: Vec::new(),
    },
    cancellation.clone(),
    &sender,
  );
  tokio::pin!(execution);
  loop {
    tokio::select! {
      result = &mut execution => match result {
        Ok(_) => panic!("cancellation fixture succeeded before its first runner event"),
        Err(error) => panic!("cancellation fixture failed before its first runner event: {error}"),
      },
      item = receiver.recv() => match item {
        Some(RunnerStreamItem::Event(_)) => {
          cancellation.cancel();
          break;
        }
        Some(RunnerStreamItem::ResourceUsage(_) | RunnerStreamItem::AccountingUnavailable { .. }) => {},
        None => panic!("cancellation fixture event channel closed before a runner event"),
      }
    }
  }
  let completion = execution
    .await
    .expect("real backend cancellation must complete cleanly");
  assert_eq!(completion.runner().status, RunStatus::Cancelled);
  assert_eq!(
    completion.runner().termination_reason,
    Some(octacity_runner::TerminationReason::Cancelled)
  );
  let job_root = completion
    .workspace()
    .parent()
    .expect("workspace must have a job root")
    .to_owned();
  completion
    .cleanup()
    .await
    .expect("cancelled job workspace cleanup must succeed");
  assert!(!job_root.exists(), "cancelled job filesystem state must be removed");
}

fn specification(target: RuntimeTarget, workspace_bytes: u64, now: u64, runner: &RunnerInstallation) -> JobSpecV1 {
  JobSpecV1 {
    protocol_version: AGENT_PROTOCOL_VERSION,
    job_id: format!(
      "backend-contract-{:?}",
      match &target {
        RuntimeTarget::Native { .. } => RuntimeMode::Native,
        RuntimeTarget::Oci { .. } => RuntimeMode::Oci,
      }
    )
    .to_lowercase(),
    attempt: 1,
    issued_at: now.saturating_sub(1),
    expires_at: now + 300,
    source: SourceSpec {
      provider: "fixture".to_owned(),
      plugin_version: "1.0.0".to_owned(),
      plugin_sha256: SOURCE_DIGEST.to_owned(),
      revision: "backend-contract-v1".to_owned(),
      reference: None,
      parameters: BTreeMap::new(),
    },
    octa: OctaSpec {
      version: runner.capabilities.octa_version.clone(),
      runner_sha256: runner.sha256.clone(),
      runner_protocol: first("runner protocol", &runner.capabilities.runner_protocols),
      event_schema: first("event schema", &runner.capabilities.event_schemas),
      plugin_protocol: first("plugin protocol", &runner.capabilities.plugin_protocols),
      plugin_digests: runner
        .plugins
        .iter()
        .map(|(name, plugin)| (name.clone(), plugin.sha256.clone()))
        .collect(),
    },
    execution: ExecutionSpec {
      octafile: Some("Octafile.yml".to_owned()),
      commands: vec!["contract".to_owned()],
      variables: BTreeMap::new(),
      arguments: Vec::new(),
      concurrency: None,
      parallel: false,
      failfast: true,
      secrets_profile: None,
    },
    runtime: RuntimeSpec {
      target: target.clone(),
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: workspace_bytes,
      timeout_seconds: 60,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    },
    cache: None,
    outputs: OutputLimits {
      artifact_count: 0,
      artifact_bytes: 0,
      report_count: 0,
      report_bytes: 0,
      single_output_bytes: 0,
    },
  }
}

fn host_specification(
  platform: PlatformSpec,
  workspace_bytes: u64,
  now: u64,
  runner: &RunnerInstallation,
) -> JobSpecV2 {
  let legacy = specification(RuntimeTarget::Native { platform }, workspace_bytes, now, runner);
  JobSpecV2 {
    protocol_version: EXECUTION_CONTRACT_V2,
    job_id: "backend-contract-host".to_owned(),
    attempt: legacy.attempt,
    issued_at: legacy.issued_at,
    expires_at: legacy.expires_at,
    source: legacy.source,
    octa: legacy.octa,
    execution: legacy.execution,
    runtime: RuntimeSpecV2 {
      target: ExecutionTargetV2 {
        mode: ExecutionMode::Host,
        host_platform: platform,
        target_platform: platform,
        required_guarantees: guarantees_for(ExecutionMode::Host),
        immutable_image: None,
      },
      cpu_millis: legacy.runtime.cpu_millis,
      memory_bytes: legacy.runtime.memory_bytes,
      writable_disk_bytes: legacy.runtime.writable_disk_bytes,
      timeout_seconds: legacy.runtime.timeout_seconds,
      network: NetworkPolicy::Unrestricted,
      workload_identity_profile: None,
    },
    cache: None,
    outputs: legacy.outputs,
  }
}

fn oci_v2_specification(
  mode: ExecutionMode,
  host_platform: PlatformSpec,
  target_platform: PlatformSpec,
  image: String,
  workspace_bytes: u64,
  now: u64,
  runner: &RunnerInstallation,
) -> JobSpecV2 {
  let legacy = specification(
    RuntimeTarget::Oci {
      platform: target_platform,
      isolation: if mode == ExecutionMode::Virtualization {
        OciIsolation::Hypervisor
      } else {
        OciIsolation::Process
      },
      image: image.clone(),
    },
    workspace_bytes,
    now,
    runner,
  );
  JobSpecV2 {
    protocol_version: EXECUTION_CONTRACT_V2,
    job_id: format!(
      "backend-contract-{}",
      match mode {
        ExecutionMode::Host => "host",
        ExecutionMode::Isolation => "isolation",
        ExecutionMode::Virtualization => "virtualization",
      }
    ),
    attempt: legacy.attempt,
    issued_at: legacy.issued_at,
    expires_at: legacy.expires_at,
    source: legacy.source,
    octa: legacy.octa,
    execution: legacy.execution,
    runtime: RuntimeSpecV2 {
      target: ExecutionTargetV2 {
        mode,
        host_platform,
        target_platform,
        required_guarantees: guarantees_for(mode),
        immutable_image: Some(image),
      },
      cpu_millis: legacy.runtime.cpu_millis,
      memory_bytes: legacy.runtime.memory_bytes,
      writable_disk_bytes: legacy.runtime.writable_disk_bytes,
      timeout_seconds: legacy.runtime.timeout_seconds,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    },
    cache: None,
    outputs: legacy.outputs,
  }
}

fn oci_v3_specification(ordinary: &JobSpecV2, runner: &RunnerInstallation, octafile: &[u8]) -> JobSpecV3 {
  let protected_input = ProtectedInputV3 {
    artifact_id: "factory-octafile".to_owned(),
    size_bytes: octafile.len() as u64,
    sha256: hex::encode(Sha256::digest(octafile)),
    media_type: "application/yaml".to_owned(),
    destination: "/octacity/protected/Octafile.yml".to_owned(),
  };
  let network_hosts = match &ordinary.runtime.network {
    NetworkPolicy::Restricted { allowed_hosts } => allowed_hosts.clone(),
    NetworkPolicy::Disabled => Vec::new(),
    NetworkPolicy::Unrestricted => panic!("Factory backend contract cannot use unrestricted networking"),
  };
  let plugins = runner
    .plugins
    .iter()
    .map(|(identity, plugin)| FactoryImmutableReferenceV3 {
      identity: identity.clone(),
      version: plugin.version.clone(),
      sha256: plugin.sha256.clone(),
    })
    .collect();
  JobSpecV3 {
    protocol_version: EXECUTION_CONTRACT_V3,
    job_id: format!("{}-factory", ordinary.job_id),
    attempt: ordinary.attempt,
    issued_at: ordinary.issued_at,
    expires_at: ordinary.expires_at,
    source: ordinary.source.clone(),
    octa: ordinary.octa.clone(),
    execution: ManagedOctaExecutionV3 {
      octafile_input: protected_input.artifact_id.clone(),
      tasks: vec!["contract".to_owned()],
      credential_profile: None,
      tool_control: None,
    },
    runtime: ordinary.runtime.clone(),
    cache: None,
    outputs: ordinary.outputs.clone(),
    factory: None,
    protected_inputs: ProtectedInputManifestV3 {
      inputs: vec![protected_input],
    },
    permissions: FactoryPermissionSetV3 {
      plugins,
      executables: Vec::new(),
      tools: Vec::new(),
      commands: Vec::new(),
      max_descendants: 30,
      mounts: vec![
        FactoryMountPermissionV3 {
          root: "/octacity/protected".to_owned(),
          mode: FactoryMountModeV3::ReadOnly,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/output".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/scratch".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/source".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
      ],
      network_hosts,
      secret_profiles: Vec::new(),
      workload_identity_profiles: vec!["backend-contract".to_owned()],
      resources: FactoryResourceLimitsV3 {
        cpu_millis: ordinary.runtime.cpu_millis,
        memory_bytes: ordinary.runtime.memory_bytes,
        disk_bytes: ordinary.runtime.writable_disk_bytes,
        process_count: 32,
        elapsed_millis: ordinary.runtime.timeout_seconds * 1_000,
      },
      outputs: FactoryOutputPermissionsV3 {
        kinds: Vec::new(),
        max_artifact_count: 0,
        max_artifact_bytes: 0,
        max_report_count: 0,
        max_report_bytes: 0,
      },
    },
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
  }
}

fn factory_octafile(probe: &OciProbe) -> Vec<u8> {
  let network_probe = match probe {
    OciProbe::RestrictedNetwork {
      allowed_host,
      denied_host,
    } => format!(
      "      curl --fail --silent --show-error --max-time 10 https://{allowed_host}/ > /dev/null\n      ! curl --fail --silent --show-error --max-time 3 https://{denied_host}/ > /dev/null 2>&1\n"
    ),
    OciProbe::DisabledNetwork => {
      "      ! curl --fail --silent --show-error --max-time 3 https://example.com/ > /dev/null 2>&1\n".to_owned()
    }
  };
  format!(
    "version: 1\n\ntasks:\n  contract:\n    shell: |\n      test \"$(cat /run/octa-identity)\" = backend-contract-identity\n      ! printf tamper >> /run/octa-identity\n      test -r /octacity/protected/Octafile.yml\n      ! printf tamper >> /octacity/protected/Octafile.yml\n      test ! -e /workspace/protected/Octafile.yml\n      printf source > /workspace/source/factory-source\n      printf scratch > /workspace/scratch/factory-scratch\n      printf output > /workspace/output/factory-output\n      ! mkdir /workspace/escape\n      test ! -e /run/octa-cache/token\n{network_probe}      echo octacity-factory-backend-contract\n"
  )
  .into_bytes()
}

async fn protected_input_server(body: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  let task = tokio::spawn(async move {
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(120), listener.accept())
      .await
      .expect("Factory protected-input request timed out")
      .unwrap();
    let mut request = [0_u8; 4096];
    let _ = socket.read(&mut request).await.unwrap();
    let response = format!(
      "HTTP/1.1 200 OK\r\nContent-Type: application/yaml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
      body.len()
    );
    socket.write_all(response.as_bytes()).await.unwrap();
    socket.write_all(&body).await.unwrap();
  });
  (format!("http://{address}"), task)
}

fn protected_transfer(body: &[u8], origin: &str) -> ProtectedInputTransferV3 {
  ProtectedInputTransferV3 {
    input: ProtectedInputV3 {
      artifact_id: "factory-octafile".to_owned(),
      size_bytes: body.len() as u64,
      sha256: hex::encode(Sha256::digest(body)),
      media_type: "application/yaml".to_owned(),
      destination: "/octacity/protected/Octafile.yml".to_owned(),
    },
    capability: ArtifactTransferCapability {
      url: format!("{origin}/factory-octafile"),
      required_headers: BTreeMap::new(),
      expires_at_unix_ms: SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .saturating_add(300_000) as u64,
    },
  }
}

async fn exercise_factory_contract(executor: &JobExecutor, spec: JobSpecV3, protected_input: ProtectedInputTransferV3) {
  let resource_limits = spec.permissions.resources;
  let (sender, mut receiver) = mpsc::channel(128);
  let completion = executor
    .execute(
      ExecuteJobRequest {
        spec: spec.into(),
        source_credentials: BTreeMap::new(),
        cache_grant: None,
        protected_inputs: vec![protected_input],
      },
      CancellationToken::new(),
      &sender,
    )
    .await
    .expect("qualified real backend Factory execution must succeed");
  drop(sender);
  let mut saw_event = false;
  while let Some(item) = receiver.recv().await {
    saw_event |= matches!(item, RunnerStreamItem::Event(_));
  }
  assert!(saw_event, "Factory runner must emit at least one event");
  assert_eq!(completion.runner().status, RunStatus::Succeeded);
  assert!(
    completion.runner().results.is_empty(),
    "Factory output authority is empty"
  );
  assert!(completion.runner().final_usage.memory_peak_bytes <= resource_limits.memory_bytes);
  assert!(completion.runner().final_usage.disk_peak_bytes <= resource_limits.disk_bytes);
  let job_root = completion.workspace().parent().unwrap().to_owned();
  assert!(completion.workspace().join("factory-source").is_file());
  assert!(job_root.join("scratch/factory-scratch").is_file());
  assert!(job_root.join("output/factory-output").is_file());
  assert!(!job_root.join("protected").exists());
  assert!(!job_root.join("escape").exists());
  completion.cleanup().await.unwrap();
  assert!(!job_root.exists());
}

async fn exercise_factory_negative_contracts(
  executor: &JobExecutor,
  baseline: &JobSpecV3,
  protected_input: ProtectedInputTransferV3,
) {
  for (name, spec, expected) in factory_negative_cases(baseline) {
    let (sender, _receiver) = mpsc::channel(1);
    let failure = executor
      .execute(
        ExecuteJobRequest {
          spec: spec.into(),
          source_credentials: BTreeMap::new(),
          cache_grant: None,
          protected_inputs: vec![protected_input.clone()],
        },
        CancellationToken::new(),
        &sender,
      )
      .await
      .expect_err("broader Factory authority must fail before real-backend spawn");
    assert!(
      matches!(failure.error(), JobError::FactoryPreflight(actual) if *actual == expected),
      "unexpected {name} Factory backend result: {failure:?}"
    );
    failure.cleanup().await.unwrap();
  }
}

fn factory_negative_cases(baseline: &JobSpecV3) -> [(&'static str, JobSpecV3, FactoryPreflightError); 6] {
  [
    factory_negative_case(baseline, "descendants", FactoryPreflightError::Descendants, |spec| {
      spec.permissions.max_descendants += 1;
    }),
    factory_negative_case(baseline, "mount", FactoryPreflightError::Mount, |spec| {
      spec.permissions.mounts.insert(
        1,
        FactoryMountPermissionV3 {
          root: "/workspace/escape".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
      );
    }),
    factory_negative_case(baseline, "network", FactoryPreflightError::Network, |spec| {
      spec.permissions.network_hosts.push("outside-policy.invalid".to_owned());
      spec.permissions.network_hosts.sort();
    }),
    factory_negative_case(baseline, "resources", FactoryPreflightError::Resources, |spec| {
      spec.permissions.resources.memory_bytes += 1;
    }),
    factory_negative_case(baseline, "secret", FactoryPreflightError::SecretProfile, |spec| {
      spec.execution.credential_profile = Some("undeclared-model-profile".to_owned());
      spec.permissions.secret_profiles = vec!["undeclared-model-profile".to_owned()];
    }),
    factory_negative_case(baseline, "outputs", FactoryPreflightError::Outputs, |spec| {
      spec.permissions.outputs = FactoryOutputPermissionsV3 {
        kinds: vec!["unexpected-output".to_owned()],
        max_artifact_count: 1,
        max_artifact_bytes: 1,
        max_report_count: 0,
        max_report_bytes: 0,
      };
    }),
  ]
}

fn factory_negative_case(
  baseline: &JobSpecV3,
  name: &'static str,
  expected: FactoryPreflightError,
  mutate: impl FnOnce(&mut JobSpecV3),
) -> (&'static str, JobSpecV3, FactoryPreflightError) {
  let mut spec = baseline.clone();
  spec.job_id = format!("{}-{name}", spec.job_id);
  mutate(&mut spec);
  (name, spec, expected)
}

#[test]
fn negative_factory_backend_cases_preserve_valid_permission_shapes() {
  let mut baseline: JobSpecV3 = serde_json::from_str(include_str!(
    "../../../shared/protocol-fixtures/job-spec/job-spec-v3.json"
  ))
  .unwrap();
  baseline.permissions.max_descendants = 30;
  baseline.permissions.resources.process_count = 32;
  baseline.permissions.network_hosts = vec!["z.example".to_owned()];

  for (name, spec, _) in factory_negative_cases(&baseline) {
    spec
      .permissions
      .validate()
      .unwrap_or_else(|error| panic!("{name} case invalidated signed permissions: {error}"));
  }
}

fn cancellation_spec(mut spec: VerifiedJobSpec) -> VerifiedJobSpec {
  match &mut spec {
    VerifiedJobSpec::V1(spec) => {
      spec.job_id.push_str("-cancel");
      spec.execution.commands = vec!["wait".to_owned()];
      spec.cache = None;
    }
    VerifiedJobSpec::V2(spec) => {
      spec.job_id.push_str("-cancel");
      spec.execution.commands = vec!["wait".to_owned()];
      spec.cache = None;
    }
    VerifiedJobSpec::V3(spec) => {
      spec.job_id.push_str("-cancel");
      spec.execution.tasks = vec!["wait".to_owned()];
      spec.cache = None;
    }
  }
  spec
}

fn executor_config(
  work_root: PathBuf,
  workspace_bytes: u64,
  allow_unrestricted_network: bool,
  allowed_network_hosts: Vec<String>,
) -> JobExecutorConfig {
  JobExecutorConfig {
    work_root,
    max_workspace_bytes: workspace_bytes,
    allow_unrestricted_network,
    allowed_network_hosts,
    max_output_limits: OutputLimits {
      artifact_count: 0,
      artifact_bytes: 0,
      report_count: 0,
      report_bytes: 0,
      single_output_bytes: 0,
    },
    cancellation_grace: Duration::from_secs(5),
    runner_supervision: RunnerSupervisionPolicy::default(),
    external_executables: BTreeMap::new(),
    factory_permissions: None,
  }
}

fn unix_now() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .expect("system clock must be after the Unix epoch")
    .as_secs()
}

fn host_environment() -> BTreeMap<String, String> {
  let path = env::var("OCTACITY_CONTRACT_HOST_PATH")
    .or_else(|_| env::var("PATH"))
    .expect("Host backend contract requires PATH");
  let mut environment = BTreeMap::from([("PATH".to_owned(), path)]);
  for name in ["COMSPEC", "HOME", "PATHEXT", "SYSTEMROOT", "TMP", "TEMP", "WINDIR"] {
    if let Ok(value) = env::var(name)
      && !value.is_empty()
    {
      environment.insert(name.to_owned(), value);
    }
  }
  environment
}

fn host_platform() -> PlatformSpec {
  let os = match std::env::consts::OS {
    "linux" => PlatformOs::Linux,
    "macos" => PlatformOs::Macos,
    "windows" => PlatformOs::Windows,
    other => panic!("unsupported host operating system '{other}'"),
  };
  let architecture = match std::env::consts::ARCH {
    "x86_64" => PlatformArchitecture::Amd64,
    "aarch64" => PlatformArchitecture::Arm64,
    other => panic!("unsupported host architecture '{other}'"),
  };
  PlatformSpec { os, architecture }
}

fn linux_platform() -> PlatformSpec {
  PlatformSpec {
    os: PlatformOs::Linux,
    architecture: if cfg!(target_arch = "aarch64") {
      PlatformArchitecture::Arm64
    } else {
      PlatformArchitecture::Amd64
    },
  }
}

fn first(name: &str, values: &[u16]) -> u16 {
  *values.first().unwrap_or_else(|| panic!("Octa release has no {name}"))
}

fn required_path(name: &str) -> PathBuf {
  let path = required_absolute_path(name);
  assert!(Path::new(&path).is_dir(), "{name} must name an existing directory");
  path
}

fn required_absolute_path(name: &str) -> PathBuf {
  let path = PathBuf::from(required_string(name));
  assert!(path.is_absolute(), "{name} must be an absolute path");
  path
}

fn required_string(name: &str) -> String {
  env::var(name)
    .unwrap_or_else(|_| panic!("{name} must be set for this explicit backend contract test"))
    .trim()
    .to_owned()
}

fn required_network_host(name: &str) -> String {
  let host = required_string(name);
  assert!(
    !host.starts_with('-')
      && !host.contains("..")
      && host
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')),
    "{name} must be one DNS hostname without a scheme, port, path, or shell metacharacters"
  );
  host
}

fn required_u64(name: &str) -> u64 {
  required_string(name)
    .parse()
    .unwrap_or_else(|_| panic!("{name} must be a positive integer"))
}
