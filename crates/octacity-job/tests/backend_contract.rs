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
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey};
use octacity_execution::ExecutionBackend;
use octacity_execution_containerd::{ContainerdEngine, ContainerdEngineConfig};
use octacity_execution_microsandbox::{MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_native::{LinuxNativeConfig, NativeBackend};
use octacity_execution_oci::{OciBackend, OciEngine};
use octacity_job::{ExecuteJobRequest, JobExecutor, JobExecutorConfig};
use octacity_protocol::{
  AGENT_PROTOCOL_VERSION, ExecutionSpec, JobSpecV1, NetworkPolicy, OciIsolation, OctaSpec, OutputLimits,
  PlatformArchitecture, PlatformOs, PlatformSpec, RuntimeMode, RuntimeSpec, RuntimeTarget, SIGNATURE_ALGORITHM,
  SignedEnvelope, SourceSpec,
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

struct FixtureSource;

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
    std::fs::write(request.destination.join("Octafile.yml"), FIXTURE_OCTAFILE).map_err(|source| {
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
  )
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Linux/KVM or Apple Silicon macOS and a provisioned Microsandbox image; see docs/backend-contract-tests.md"]
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
  let backend = Arc::new(OciBackend::new(vec![engine]).expect("OCI routing must be valid"));
  run_contract(
    RuntimeTarget::Oci {
      platform: linux_platform(),
      isolation: OciIsolation::Hypervisor,
      image,
    },
    work_root,
    workspace_bytes,
    backend,
  )
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Linux containerd, runc/crun, cgroup v2, and a quota-mounted work root; see docs/backend-contract-tests.md"]
async fn containerd_process_engine_satisfies_the_real_runner_contract() {
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
  let backend = Arc::new(OciBackend::new(vec![engine]).expect("OCI routing must be valid"));
  run_contract(
    RuntimeTarget::Oci {
      platform: linux_platform(),
      isolation: OciIsolation::Process,
      image,
    },
    work_root,
    workspace_bytes,
    backend,
  )
  .await;
}

async fn run_contract(
  target: RuntimeTarget,
  work_root: PathBuf,
  workspace_bytes: u64,
  backend: Arc<dyn ExecutionBackend>,
) {
  let mode = match &target {
    RuntimeTarget::Native { .. } => RuntimeMode::Native,
    RuntimeTarget::Oci { .. } => RuntimeMode::Oci,
  };
  let release_root = required_path("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT");
  let runner = RunnerInstallation::load(&release_root).expect("the configured Octa release must be valid");
  let key = SigningKey::from_bytes(&[23; 32]);
  let executor = JobExecutor::new(
    BTreeMap::from([("backend-contract".to_owned(), key.verifying_key())]),
    runner.clone(),
    Arc::new(FixtureSource),
    BTreeMap::from([(mode, backend)]),
    JobExecutorConfig {
      work_root,
      max_workspace_bytes: workspace_bytes,
      cancellation_grace: Duration::from_secs(5),
      runner_supervision: RunnerSupervisionPolicy::default(),
    },
  )
  .expect("job executor configuration must be valid");
  executor
    .cleanup_orphans()
    .await
    .expect("pre-test orphan cleanup must succeed");

  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .expect("system clock must be after the Unix epoch")
    .as_secs();
  let spec = specification(target.clone(), workspace_bytes, now, &runner);
  let envelope = sign(&spec, &key);
  let (sender, mut receiver) = mpsc::channel(128);
  let started = Instant::now();
  let completion = executor
    .execute(
      ExecuteJobRequest {
        envelope,
        job_id: spec.job_id.clone(),
        attempt: spec.attempt,
        now,
        source_credentials: BTreeMap::new(),
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
  run_cancellation_contract(&executor, target, workspace_bytes, now, &runner).await;
  executor
    .cleanup_orphans()
    .await
    .expect("post-test backend cleanup must find no leaked state");
  eprintln!("{mode:?} real backend contract completed in {:?}", started.elapsed());
}

async fn run_cancellation_contract(
  executor: &JobExecutor,
  target: RuntimeTarget,
  workspace_bytes: u64,
  now: u64,
  runner: &RunnerInstallation,
) {
  let mut spec = specification(target, workspace_bytes, now, runner);
  spec.job_id.push_str("-cancel");
  spec.execution.commands = vec!["wait".to_owned()];
  let envelope = sign(&spec, &SigningKey::from_bytes(&[23; 32]));
  let cancellation = CancellationToken::new();
  let (sender, mut receiver) = mpsc::channel(128);
  let execution = executor.execute(
    ExecuteJobRequest {
      envelope,
      job_id: spec.job_id,
      attempt: spec.attempt,
      now,
      source_credentials: BTreeMap::new(),
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
    outputs: OutputLimits {
      artifact_count: 0,
      artifact_bytes: 0,
      report_count: 0,
      report_bytes: 0,
    },
  }
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

fn sign(spec: &JobSpecV1, key: &SigningKey) -> SignedEnvelope {
  let payload = serde_json::to_vec(spec).expect("fixture JobSpec must serialize");
  SignedEnvelope {
    key_id: "backend-contract".to_owned(),
    algorithm: SIGNATURE_ALGORITHM.to_owned(),
    payload: BASE64.encode(&payload),
    signature: BASE64.encode(key.sign(&payload).to_bytes()),
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

fn required_u64(name: &str) -> u64 {
  required_string(name)
    .parse()
    .unwrap_or_else(|_| panic!("{name} must be a positive integer"))
}
