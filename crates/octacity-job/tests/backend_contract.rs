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
use octacity_execution::ExecutionBackend;
use octacity_execution_containerd::{ContainerdEngine, ContainerdEngineConfig};
use octacity_execution_microsandbox::{MicrosandboxEngine, MicrosandboxEngineConfig};
use octacity_execution_native::{LinuxNativeConfig, NativeBackend};
use octacity_execution_oci::{OciBackend, OciEngine};
use octacity_identity::FileWorkloadIdentityProvider;
use octacity_job::{ExecuteJobRequest, JobExecutor, JobExecutorConfig};
use octacity_protocol::{
  AGENT_PROTOCOL_VERSION, ExecutionSpec, JobSpecV1, NetworkPolicy, OciIsolation, OctaSpec, OutputLimits,
  PlatformArchitecture, PlatformOs, PlatformSpec, RuntimeMode, RuntimeSpec, RuntimeTarget, SourceSpec,
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
  network_probe: Option<NetworkProbe>,
}

#[derive(Clone)]
struct NetworkProbe {
  allowed_host: String,
  denied_host: String,
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
    let octafile = if let Some(probe) = &self.network_probe {
      format!(
        "version: 1\n\ntasks:\n  contract:\n    shell: |\n      test \"$(cat /run/octa-identity)\" = backend-contract-identity\n      ! printf tamper >> /run/octa-identity\n      curl --fail --silent --show-error --max-time 10 https://{}/ > /dev/null\n      ! curl --fail --silent --show-error --max-time 3 https://{}/ > /dev/null 2>&1\n      echo octacity-backend-contract\n  wait:\n    shell: sleep 60\n",
        probe.allowed_host, probe.denied_host
      )
    } else {
      FIXTURE_OCTAFILE.to_owned()
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
    false,
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
    true,
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
    false,
  )
  .await;
}

async fn run_contract(
  target: RuntimeTarget,
  work_root: PathBuf,
  workspace_bytes: u64,
  backend: Arc<dyn ExecutionBackend>,
  secure_oci_contract: bool,
) {
  let mode = match &target {
    RuntimeTarget::Native { .. } => RuntimeMode::Native,
    RuntimeTarget::Oci { .. } => RuntimeMode::Oci,
  };
  let release_root = required_path("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT");
  let runner = RunnerInstallation::load(&release_root).expect("the configured Octa release must be valid");
  let network_probe = secure_oci_contract.then(|| NetworkProbe {
    allowed_host: required_network_host("OCTACITY_CONTRACT_MICROSANDBOX_ALLOWED_HOST"),
    denied_host: required_network_host("OCTACITY_CONTRACT_MICROSANDBOX_DENIED_HOST"),
  });
  let identity_source = network_probe.as_ref().map(|_| {
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
      network_probe: network_probe.clone(),
    }),
    Arc::new(identity),
    BTreeMap::from([(mode, backend)]),
    JobExecutorConfig {
      work_root,
      max_workspace_bytes: workspace_bytes,
      allow_unrestricted_network: false,
      allowed_network_hosts: network_probe
        .as_ref()
        .map(|probe| probe.allowed_host.clone())
        .into_iter()
        .collect(),
      max_output_limits: OutputLimits {
        artifact_count: 0,
        artifact_bytes: 0,
        report_count: 0,
        report_bytes: 0,
        single_output_bytes: 0,
      },
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
  let mut spec = specification(target.clone(), workspace_bytes, now, &runner);
  if let Some(probe) = &network_probe {
    spec.runtime.network = NetworkPolicy::Restricted {
      allowed_hosts: vec![probe.allowed_host.clone()],
    };
    spec.runtime.workload_identity_profile = Some("backend-contract".to_owned());
  }
  let (sender, mut receiver) = mpsc::channel(128);
  let started = Instant::now();
  let completion = executor
    .execute(
      ExecuteJobRequest {
        spec,
        source_credentials: BTreeMap::new(),
        cache_grant: None,
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
