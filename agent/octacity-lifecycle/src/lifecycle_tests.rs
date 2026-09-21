use std::sync::{
  Mutex as StdMutex,
  atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey};
use octacity_cache_session::{CacheSessionManager, CacheSessionManagerConfig};
use octacity_execution::{
  ExecutionBackend, ExecutionCacheMounts, ExecutionError, ExecutionExit, ExecutionIo, ExecutionPaths, ExecutionReader,
  ExecutionWriter, LocalCacheCapacity, ResourceUsage, RunnerProgram, RunningExecution, StartExecution,
};
use octacity_identity::FileWorkloadIdentityProvider;
use octacity_job::JobExecutorConfig;
use octacity_output::{FreezeOutputs, FrozenOutputs, OutputError, OutputPublisher, PublishOutputs};
use octacity_protocol::{
  AGENT_PROTOCOL_VERSION, AcquireLeaseResponse, AgentInventory, AppendEventsResponse, BeginCacheSessionRequest,
  BeginCacheSessionResponse, CACHE_FEATURE_V1, CACHE_HTTP_FEATURE_V1, COORDINATOR_PROTOCOL_VERSION, ExecutionSpec,
  HeartbeatDirective, JobSpecV1, NetworkPolicy, OctaSpec, OutputLimits, PlatformArchitecture, PlatformOs, PlatformSpec,
  RevokeCacheSessionRequest, RunnerEventPayload, RuntimeMode, RuntimeSpec, RuntimeTarget, SIGNATURE_ALGORITHM,
  SignedEnvelope, SourceSpec,
};
use octacity_runner::{RunnerCapabilities, RunnerInstallation, RunnerSupervisionPolicy};
use octacity_source::{MaterializedSource, SourceError, SourceMaterializationRequest, SourceMaterializer};
use tokio::{
  io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
  sync::Mutex,
};

use super::*;
use crate::{
  delivery::{append, flush},
  journal::JobJournal,
  spool::EventSpool,
};

struct NoopOutputPublisher;

#[async_trait]
impl OutputPublisher for NoopOutputPublisher {
  async fn freeze(
    &self,
    _request: FreezeOutputs<'_>,
    _cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError> {
    Ok(FrozenOutputs::empty())
  }

  async fn publish(&self, _request: PublishOutputs<'_>, _cancellation: CancellationToken) -> Result<(), OutputError> {
    Ok(())
  }
}

struct RecordingOutputPublisher {
  called: AtomicBool,
}

struct CancellationAwareOutputPublisher {
  called: AtomicBool,
}

struct BlockingOutputPublisher {
  started: Arc<AtomicBool>,
}

struct FailingOutputPublisher;

struct FailingUploadPublisher;

struct FencedOutputPublisher;

#[async_trait]
impl OutputPublisher for FailingOutputPublisher {
  async fn freeze(
    &self,
    _request: FreezeOutputs<'_>,
    _cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError> {
    Err(OutputError::Invalid("fixture rejected output".to_owned()))
  }

  async fn publish(&self, _request: PublishOutputs<'_>, _cancellation: CancellationToken) -> Result<(), OutputError> {
    unreachable!("failed freeze must not reach publication")
  }
}

#[async_trait]
impl OutputPublisher for FailingUploadPublisher {
  async fn freeze(
    &self,
    _request: FreezeOutputs<'_>,
    _cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError> {
    Ok(FrozenOutputs::empty())
  }

  async fn publish(&self, _request: PublishOutputs<'_>, _cancellation: CancellationToken) -> Result<(), OutputError> {
    Err(OutputError::Upload("fixture exhausted retries".to_owned()))
  }
}

#[async_trait]
impl OutputPublisher for FencedOutputPublisher {
  async fn freeze(
    &self,
    _request: FreezeOutputs<'_>,
    _cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError> {
    Ok(FrozenOutputs::empty())
  }

  async fn publish(&self, _request: PublishOutputs<'_>, _cancellation: CancellationToken) -> Result<(), OutputError> {
    Err(OutputError::Coordinator(CoordinatorError::Rejected {
      operation: "begin output upload",
      status: 409,
      code: "lease_fenced".to_owned(),
      message: "the attempt no longer owns this lease".to_owned(),
      retryable: false,
    }))
  }
}

#[async_trait]
impl OutputPublisher for RecordingOutputPublisher {
  async fn freeze(
    &self,
    request: FreezeOutputs<'_>,
    _cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError> {
    assert!(
      request.workspace.is_dir(),
      "workspace was removed before output freezing"
    );
    Ok(FrozenOutputs::empty())
  }

  async fn publish(&self, _request: PublishOutputs<'_>, _cancellation: CancellationToken) -> Result<(), OutputError> {
    self.called.store(true, Ordering::SeqCst);
    Ok(())
  }
}

#[async_trait]
impl OutputPublisher for CancellationAwareOutputPublisher {
  async fn freeze(
    &self,
    _request: FreezeOutputs<'_>,
    _cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError> {
    Ok(FrozenOutputs::empty())
  }

  async fn publish(&self, _request: PublishOutputs<'_>, cancellation: CancellationToken) -> Result<(), OutputError> {
    if cancellation.is_cancelled() {
      return Err(OutputError::Cancelled);
    }
    self.called.store(true, Ordering::SeqCst);
    Ok(())
  }
}

#[async_trait]
impl OutputPublisher for BlockingOutputPublisher {
  async fn freeze(
    &self,
    _request: FreezeOutputs<'_>,
    _cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError> {
    Ok(FrozenOutputs::empty())
  }

  async fn publish(&self, _request: PublishOutputs<'_>, cancellation: CancellationToken) -> Result<(), OutputError> {
    self.started.store(true, Ordering::SeqCst);
    cancellation.cancelled().await;
    Err(OutputError::Cancelled)
  }
}

struct ReplayCoordinator {
  fail_first: AtomicBool,
  reject_permanently: bool,
  calls: StdMutex<Vec<Vec<u64>>>,
}

struct LifecycleCoordinator {
  fence_on_heartbeat: bool,
  fence_after_output_starts: Option<Arc<AtomicBool>>,
  cancel_on_heartbeat: bool,
  cache_begin_failure: bool,
  cache_revoke_failure: bool,
  panic_backend: Arc<AtomicBool>,
  orphan_cleanup_observed: Arc<AtomicBool>,
  events: StdMutex<Vec<octacity_protocol::AttemptEventEnvelope>>,
  completions: StdMutex<Vec<CompleteLeaseRequest>>,
  heartbeat_snapshots: StdMutex<Vec<HostSnapshot>>,
  cache_begins: StdMutex<Vec<BeginCacheSessionRequest>>,
  cache_revocations: StdMutex<Vec<RevokeCacheSessionRequest>>,
  usage_observed: Arc<Notify>,
}

impl Default for LifecycleCoordinator {
  fn default() -> Self {
    Self {
      fence_on_heartbeat: false,
      fence_after_output_starts: None,
      cancel_on_heartbeat: false,
      cache_begin_failure: false,
      cache_revoke_failure: false,
      panic_backend: Arc::new(AtomicBool::new(false)),
      orphan_cleanup_observed: Arc::new(AtomicBool::new(false)),
      events: StdMutex::new(Vec::new()),
      completions: StdMutex::new(Vec::new()),
      heartbeat_snapshots: StdMutex::new(Vec::new()),
      cache_begins: StdMutex::new(Vec::new()),
      cache_revocations: StdMutex::new(Vec::new()),
      usage_observed: Arc::new(Notify::new()),
    }
  }
}

fn lifecycle_states(events: &[octacity_protocol::AttemptEventEnvelope]) -> Vec<JobLifecycleState> {
  events
    .iter()
    .filter_map(|event| match &event.kind {
      AttemptEventKind::Agent {
        event: AgentLifecycleEvent::StateChanged { state },
      } => Some(*state),
      _ => None,
    })
    .collect()
}

#[async_trait]
impl CoordinatorClient for LifecycleCoordinator {
  async fn register(
    &self,
    _inventory: &AgentInventory,
    _cancellation: CancellationToken,
  ) -> Result<Registration, CoordinatorError> {
    unreachable!()
  }

  async fn acquire_lease(
    &self,
    _registration: &Registration,
    _wait: Duration,
    _lease_safety_margin: Duration,
    _accept_jobs: bool,
    _snapshot: &HostSnapshot,
    _cancellation: CancellationToken,
  ) -> Result<AcquireLeaseResponse, CoordinatorError> {
    unreachable!()
  }

  async fn heartbeat(
    &self,
    _registration: &Registration,
    lease: &octacity_protocol::LeaseAssignment,
    snapshot: &HostSnapshot,
    _capacity: &HostCapacity,
    _lease_safety_margin: Duration,
    _cancellation: CancellationToken,
  ) -> Result<HeartbeatDirective, CoordinatorError> {
    self.heartbeat_snapshots.lock().unwrap().push(snapshot.clone());
    if snapshot
      .active_job
      .as_ref()
      .is_some_and(|job| job.resource_usage.is_some())
    {
      self.usage_observed.notify_one();
    }
    if self.fence_on_heartbeat
      || self
        .fence_after_output_starts
        .as_ref()
        .is_some_and(|started| started.load(Ordering::SeqCst))
    {
      return Ok(HeartbeatDirective::Fenced);
    }
    if self.cancel_on_heartbeat {
      return Ok(HeartbeatDirective::Cancel);
    }
    Ok(HeartbeatDirective::Continue {
      expires_at: lease.expires_at,
    })
  }

  async fn append_events(
    &self,
    _registration: &Registration,
    _lease: &octacity_protocol::LeaseAssignment,
    events: &[octacity_protocol::AttemptEventEnvelope],
    _cancellation: CancellationToken,
  ) -> Result<AppendEventsResponse, CoordinatorError> {
    self.events.lock().unwrap().extend_from_slice(events);
    Ok(AppendEventsResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: "ack".to_owned(),
      acknowledged_sequence: events.last().unwrap().stream_sequence,
    })
  }

  async fn complete_lease(
    &self,
    _registration: &Registration,
    _lease: &octacity_protocol::LeaseAssignment,
    completion: &CompleteLeaseRequest,
    _cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    self.completions.lock().unwrap().push(completion.clone());
    Ok(())
  }
}

#[async_trait]
impl CacheSessionCoordinator for LifecycleCoordinator {
  async fn begin_cache_session(
    &self,
    _registration: &Registration,
    _lease: &octacity_protocol::LeaseAssignment,
    request: &BeginCacheSessionRequest,
    _cancellation: CancellationToken,
  ) -> Result<BeginCacheSessionResponse, CoordinatorError> {
    request.validate()?;
    self.cache_begins.lock().unwrap().push(request.clone());
    if self.cache_begin_failure {
      return Err(CoordinatorError::Invalid("fixture cache begin failure".to_owned()));
    }
    Ok(BeginCacheSessionResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request.request_id.clone(),
      session_id: "cache-session-1".to_owned(),
      scope_id: "project-trust-domain-1".to_owned(),
      remote: None,
    })
  }

  async fn revoke_cache_session(
    &self,
    _registration: &Registration,
    _lease: &octacity_protocol::LeaseAssignment,
    request: &RevokeCacheSessionRequest,
    _cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    request.validate()?;
    self.cache_revocations.lock().unwrap().push(request.clone());
    if self.cache_revoke_failure {
      return Err(CoordinatorError::Invalid("fixture cache revoke failure".to_owned()));
    }
    Ok(())
  }
}

struct LifecycleSource;

#[async_trait]
impl SourceMaterializer for LifecycleSource {
  async fn materialize(
    &self,
    requirement: &SourceSpec,
    request: SourceMaterializationRequest,
    _operation_timeout: Duration,
    _cancellation_grace: Duration,
    _cancellation: CancellationToken,
  ) -> Result<MaterializedSource, SourceError> {
    fs::write(request.destination.join("Octafile.yml"), "version: 1\n").unwrap();
    Ok(MaterializedSource {
      revision: requirement.revision.clone(),
      provenance: BTreeMap::new(),
      progress: Vec::new(),
      diagnostics: Vec::new(),
    })
  }
}

struct LifecycleBackend {
  wait_for_cancel: bool,
  usage_observed: Arc<Notify>,
  panic_on_start: Arc<AtomicBool>,
  orphan_cleanup_observed: Arc<AtomicBool>,
}

struct LifecycleExecution {
  io: Option<ExecutionIo>,
  paths: ExecutionPaths,
  exit_code: i32,
}

#[async_trait]
impl ExecutionBackend for LifecycleBackend {
  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    _cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError> {
    assert!(!self.panic_on_start.load(Ordering::SeqCst), "fixture backend panic");
    let request_id = request.execution_id;
    let (agent_stdin, runner_input) = tokio::io::duplex(4096);
    let (runner_output, agent_stdout) = tokio::io::duplex(4096);
    let (agent_stderr, runner_stderr) = tokio::io::duplex(64);
    drop(agent_stderr);
    tokio::spawn(lifecycle_runner(
      request_id,
      runner_input,
      runner_output,
      self.wait_for_cancel,
      self.usage_observed.clone(),
    ));
    Ok(Box::new(LifecycleExecution {
      io: Some(ExecutionIo {
        stdin: Box::pin(agent_stdin) as ExecutionWriter,
        stdout: Box::pin(agent_stdout) as ExecutionReader,
        stderr: Box::pin(runner_stderr) as ExecutionReader,
      }),
      paths: ExecutionPaths {
        workspace: request.workspace,
        data_dir: request.data_dir,
        plugins_dir: runner.plugins_dir.clone(),
        plugin_lock: runner.plugin_lock.clone(),
        cache: request.cache.as_ref().map(ExecutionCacheMounts::projected_paths),
      },
      exit_code: if self.wait_for_cancel { 130 } else { 0 },
    }))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
    self.orphan_cleanup_observed.store(true, Ordering::SeqCst);
    Ok(())
  }
}

#[async_trait]
impl RunningExecution for LifecycleExecution {
  fn paths(&self) -> &ExecutionPaths {
    &self.paths
  }

  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.io.take().ok_or(ExecutionError::IoTaken)
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    Ok(lifecycle_usage())
  }

  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    Ok(ExecutionExit {
      code: Some(self.exit_code),
    })
  }

  async fn kill(&mut self) -> Result<(), ExecutionError> {
    Ok(())
  }

  async fn destroy(self: Box<Self>) -> Result<(), ExecutionError> {
    Ok(())
  }
}

async fn lifecycle_runner(
  request_id: String,
  input: tokio::io::DuplexStream,
  mut output: tokio::io::DuplexStream,
  wait_for_cancel: bool,
  usage_observed: Arc<Notify>,
) {
  output
    .write_all(
      b"{\"type\":\"hello\",\"protocol_version\":3,\"octa_version\":\"0.3.0\",\"event_schema_version\":3,\"plugin_protocol_version\":1}\n",
    )
    .await
    .unwrap();
  let mut input = BufReader::new(input);
  let mut command = String::new();
  input.read_line(&mut command).await.unwrap();
  let id = serde_json::to_string(&request_id).unwrap();
  output
    .write_all(
      format!(
        "{{\"type\":\"accepted\",\"request_id\":{id}}}\n{{\"type\":\"event\",\"request_id\":{id},\"event\":{{\"schema_version\":3,\"sequence\":1,\"timestamp\":\"2026-09-11T12:00:00Z\",\"category\":\"execution\",\"data\":{{}}}}}}\n"
      )
      .as_bytes(),
    )
    .await
    .unwrap();
  let status = if wait_for_cancel {
    command.clear();
    input.read_line(&mut command).await.unwrap();
    assert!(command.contains("cancel"));
    "cancelled"
  } else {
    // Keep the synthetic runner alive until the lease monitor has observed a
    // resource-bearing heartbeat. This is a causal test condition and remains
    // deterministic even when a loaded CI host skips timer ticks.
    usage_observed.notified().await;
    "succeeded"
  };
  output
    .write_all(
      format!("{{\"type\":\"finished\",\"request_id\":{id},\"status\":\"{status}\",\"results\":[]}}\n").as_bytes(),
    )
    .await
    .unwrap();
}

fn lifecycle_usage() -> ResourceUsage {
  ResourceUsage {
    elapsed_ms: 10,
    cpu_time_ms: 2,
    memory_current_bytes: 1024,
    memory_peak_bytes: 2048,
    disk_current_bytes: 4096,
    disk_peak_bytes: 4096,
    io_read_bytes: 12,
    io_written_bytes: 34,
    network_received_bytes: None,
    network_transmitted_bytes: None,
  }
}

fn lifecycle_spec(now: u64) -> JobSpecV1 {
  let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
  JobSpecV1 {
    protocol_version: AGENT_PROTOCOL_VERSION,
    job_id: "job-1".to_owned(),
    attempt: 1,
    issued_at: now.saturating_sub(1),
    expires_at: now + 60,
    source: SourceSpec {
      provider: "git".to_owned(),
      plugin_version: "0.1.0".to_owned(),
      plugin_sha256: digest.to_owned(),
      revision: "abcdef".to_owned(),
      reference: None,
      parameters: BTreeMap::new(),
    },
    octa: OctaSpec {
      version: "0.3.0".to_owned(),
      runner_sha256: digest.to_owned(),
      runner_protocol: 3,
      event_schema: 3,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::new(),
    },
    execution: ExecutionSpec {
      octafile: Some("Octafile.yml".to_owned()),
      commands: vec!["build".to_owned()],
      variables: BTreeMap::new(),
      arguments: Vec::new(),
      concurrency: None,
      parallel: false,
      failfast: true,
      secrets_profile: None,
    },
    runtime: RuntimeSpec {
      target: RuntimeTarget::Native {
        platform: PlatformSpec {
          os: PlatformOs::Linux,
          architecture: PlatformArchitecture::Amd64,
        },
      },
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024,
      timeout_seconds: 30,
      network: NetworkPolicy::Unrestricted,
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

fn signed_spec(spec: &JobSpecV1) -> SignedEnvelope {
  let key = SigningKey::from_bytes(&[7; 32]);
  let payload = serde_json::to_vec(spec).unwrap();
  SignedEnvelope {
    key_id: "test-key".to_owned(),
    algorithm: SIGNATURE_ALGORITHM.to_owned(),
    payload: BASE64.encode(&payload),
    signature: BASE64.encode(key.sign(&payload).to_bytes()),
  }
}

fn lifecycle_runner_installation() -> RunnerInstallation {
  let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
  RunnerInstallation {
    root: PathBuf::from("/opt/octa"),
    executable: PathBuf::from("/opt/octa/octa-runner"),
    plugins_dir: PathBuf::from("/opt/octa/plugins"),
    default_plugin_lock: PathBuf::from("/opt/octa/Octa.lock"),
    sha256: digest.to_owned(),
    capabilities: RunnerCapabilities {
      octa_version: "0.3.0".to_owned(),
      runner_protocols: vec![3],
      event_schemas: vec![3],
      plugin_protocols: vec![1],
      octafile_versions: vec![1],
      platform: "any".to_owned(),
      features: vec![CACHE_FEATURE_V1.to_owned(), CACHE_HTTP_FEATURE_V1.to_owned()],
      build_commit: None,
    },
    plugins: BTreeMap::new(),
  }
}

/// Owns the private cache root used by the real cache-session adapter.
///
/// Windows temporary directories inherit an ACL that deliberately fails the
/// production ancestry check. Creating the fixture directly below the volume
/// root gives it an atomic private DACL while retaining trusted ancestors.
struct CacheFixtureRoot {
  path: std::path::PathBuf,
}

impl CacheFixtureRoot {
  fn path(&self) -> &std::path::Path {
    &self.path
  }
}

fn cache_fixture_root(_state_root: &std::path::Path) -> CacheFixtureRoot {
  #[cfg(windows)]
  {
    static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let profile = std::env::var_os("USERPROFILE").expect("Windows tests require USERPROFILE");
    let profile = std::fs::canonicalize(profile).expect("Windows tests require a canonical USERPROFILE");
    let volume_root = profile
      .ancestors()
      .last()
      .expect("Windows profile must have a volume root");
    for _ in 0..16 {
      let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
      let path = volume_root.join(format!(
        ".octacity-lifecycle-cache-test-{}-{sequence}",
        std::process::id()
      ));
      match octacity_private_fs::create_private_directory(&path) {
        Ok(()) => return CacheFixtureRoot { path },
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => panic!("failed to create protected Windows cache fixture: {error}"),
      }
    }
    panic!("failed to allocate a protected Windows cache fixture")
  }
  #[cfg(not(windows))]
  {
    let path = _state_root.join("cache");
    octacity_private_fs::create_private_directory(&path).unwrap();
    CacheFixtureRoot { path }
  }
}

#[cfg(windows)]
impl Drop for CacheFixtureRoot {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.path);
  }
}

fn lifecycle_fixture(
  coordinator: Arc<LifecycleCoordinator>,
  outputs: Arc<dyn OutputPublisher>,
  state_root: &std::path::Path,
  work_root: &std::path::Path,
  wait_for_cancel: bool,
) -> (CacheFixtureRoot, JobLifecycle, VerifiedLease, HostSnapshot) {
  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs();
  let spec = lifecycle_spec(now);
  let lease = octacity_protocol::LeaseAssignment {
    lease_id: "lease-1".to_owned(),
    job_id: spec.job_id.clone(),
    attempt: spec.attempt,
    fencing_token: "fence-1".to_owned(),
    issued_at: now,
    expires_at: now + 60,
    signed_job_spec: signed_spec(&spec),
  };
  let cache_root = cache_fixture_root(state_root);
  let executor = Arc::new(
    JobExecutor::new(
      lifecycle_runner_installation(),
      Arc::new(LifecycleSource),
      Arc::new(FileWorkloadIdentityProvider::default()),
      BTreeMap::from([(
        RuntimeMode::Native,
        Arc::new(LifecycleBackend {
          wait_for_cancel,
          usage_observed: coordinator.usage_observed.clone(),
          panic_on_start: coordinator.panic_backend.clone(),
          orphan_cleanup_observed: coordinator.orphan_cleanup_observed.clone(),
        }) as Arc<dyn ExecutionBackend>,
      )]),
      JobExecutorConfig {
        work_root: work_root.to_owned(),
        max_workspace_bytes: 2 * 1024 * 1024,
        allow_unrestricted_network: true,
        allowed_network_hosts: Vec::new(),
        max_output_limits: spec.outputs.clone(),
        cancellation_grace: Duration::from_secs(1),
        runner_supervision: RunnerSupervisionPolicy {
          resource_sample_interval: Duration::from_millis(1),
          ..RunnerSupervisionPolicy::default()
        },
      },
    )
    .unwrap()
    .with_cache(Arc::new(
      CacheSessionManager::new(CacheSessionManagerConfig {
        root: cache_root.path().to_owned(),
        capacity: LocalCacheCapacity::new(1024 * 1024, 900 * 1024, 800 * 1024).unwrap(),
        max_scopes: 2,
        allow_read: true,
        allow_write: true,
        allowed_origins: Vec::new(),
        ca_certificate_file: None,
        native_identities: BTreeMap::from([("linux-amd64".to_owned(), "rust-1.98-toolchain-v1".to_owned())]),
        request_timeout_seconds: 10,
        max_parallel_transfers: 2,
      })
      .unwrap(),
    )),
  );
  let capacity = HostCapacity {
    logical_cpu_count: 2,
    total_memory_bytes: 1024 * 1024 * 1024,
    work_disk_total_bytes: 4 * 1024 * 1024,
    state_disk_total_bytes: 4 * 1024 * 1024,
    virtualization_available: false,
  };
  let snapshot = HostSnapshot {
    available_cpu_millis: 2000,
    available_memory_bytes: capacity.total_memory_bytes,
    work_disk_free_bytes: capacity.work_disk_total_bytes,
    state_disk_free_bytes: capacity.state_disk_total_bytes,
    active_job: None,
    backends: Vec::new(),
  };
  let lifecycle = JobLifecycle::new(
    coordinator.clone(),
    coordinator,
    Registration {
      agent_id: "agent-1".to_owned(),
      registration_id: "registration-1".to_owned(),
      max_retry_delay: Duration::from_secs(1),
    },
    executor,
    outputs,
    capacity,
    JobLifecycleConfig {
      state_root: state_root.to_owned(),
      event_channel_capacity: 8,
      event_retry_delay: Duration::from_millis(1),
      spool: SpoolLimits {
        max_bytes: 64 * 1024,
        max_records: 32,
        batch_bytes: 16 * 1024,
        batch_records: 8,
      },
      lease_monitor: LeaseMonitorPolicy {
        heartbeat_interval: Duration::from_millis(5),
        lease_safety_margin: Duration::from_secs(5),
      },
    },
  )
  .unwrap();
  (cache_root, lifecycle, VerifiedLease { lease, spec }, snapshot)
}

include!("lifecycle_tests/cache_and_output.rs");
include!("lifecycle_tests/delivery_and_recovery.rs");
