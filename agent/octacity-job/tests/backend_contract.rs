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
use octacity_job::{ExecuteJobRequest, ExecutionBackendRoute, JobExecutor, JobExecutorConfig};
use octacity_protocol::{
  AGENT_PROTOCOL_VERSION, BeginCacheSessionResponse, CachePolicy, EXECUTION_CONTRACT_V2, ExecutionCapabilityV2,
  ExecutionEnvironmentId, ExecutionMode, ExecutionProviderId, ExecutionSpec, ExecutionTargetV2, JobSpecV1, JobSpecV2,
  NetworkPolicy, OciIsolation, OctaSpec, OutputLimits, PlatformArchitecture, PlatformOs, PlatformSpec,
  RemoteCacheGrant, RuntimeMode, RuntimeSpec, RuntimeSpecV2, RuntimeTarget, SourceSpec, VerifiedJobSpec,
  guarantees_for,
};
use octacity_runner::{RunStatus, RunnerInstallation, RunnerStreamItem, RunnerSupervisionPolicy};
use octacity_source::{MaterializedSource, SourceError, SourceMaterializationRequest, SourceMaterializer};
use tokio::sync::mpsc;
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
#[ignore = "requires a delegated cgroup v2 root and quota-mounted work root; see docs/backend-contract-tests.md"]
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
#[ignore = "requires Linux/KVM, Apple Silicon macOS, or Windows/WHP and a provisioned Microsandbox image; see docs/backend-contract-tests.md"]
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
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Linux containerd, runc/crun, cgroup v2, and a quota-mounted work root; see docs/backend-contract-tests.md"]
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
  let mut configuration = executor_config(work_root, workspace_bytes, false, Vec::new());
  if provider == APPLE_VF_PROVIDER_NAME {
    configuration.runner_supervision.resource_sample_timeout = Duration::from_secs(5);
  }
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
  .with_execution_backends([ExecutionBackendRoute::new(
    capability,
    ExecutionEnvironmentId::new(required_string(environment_identity_variable)).unwrap(),
    backend,
  )
  .unwrap()])
  .expect("v2 OCI execution route must be valid");
  executor.cleanup_orphans().await.expect("pre-test cleanup must succeed");
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
  if let OciProbe::RestrictedNetwork { allowed_host, .. } = probe {
    spec.runtime.network = NetworkPolicy::Restricted {
      allowed_hosts: vec![allowed_host],
    };
  }
  spec.runtime.workload_identity_profile = Some("backend-contract".to_owned());
  exercise_contract(
    &executor,
    spec.into(),
    None,
    Some((provider, mode)),
    &format!("{provider} {mode:?}"),
  )
  .await;
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
