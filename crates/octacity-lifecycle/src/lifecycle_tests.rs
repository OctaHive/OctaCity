use std::sync::{
  Mutex as StdMutex,
  atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey};
use octacity_execution::{
  ExecutionBackend, ExecutionError, ExecutionExit, ExecutionIo, ExecutionPaths, ExecutionReader, ExecutionWriter,
  ResourceUsage, RunnerProgram, RunningExecution, StartExecution,
};
use octacity_job::JobExecutorConfig;
use octacity_protocol::{
  AGENT_PROTOCOL_VERSION, AcquireLeaseResponse, AgentInventory, AppendEventsResponse, COORDINATOR_PROTOCOL_VERSION,
  ExecutionSpec, HeartbeatDirective, JobSpecV1, NetworkPolicy, OctaSpec, OutputLimits, PlatformArchitecture,
  PlatformOs, PlatformSpec, RunnerEventPayload, RuntimeMode, RuntimeSpec, RuntimeTarget, SIGNATURE_ALGORITHM,
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

struct ReplayCoordinator {
  fail_first: AtomicBool,
  reject_permanently: bool,
  calls: StdMutex<Vec<Vec<u64>>>,
}

struct LifecycleCoordinator {
  fence_on_heartbeat: bool,
  events: StdMutex<Vec<octacity_protocol::AttemptEventEnvelope>>,
  completions: StdMutex<Vec<CompleteLeaseRequest>>,
  heartbeat_snapshots: StdMutex<Vec<HostSnapshot>>,
  usage_observed: Arc<Notify>,
}

impl Default for LifecycleCoordinator {
  fn default() -> Self {
    Self {
      fence_on_heartbeat: false,
      events: StdMutex::new(Vec::new()),
      completions: StdMutex::new(Vec::new()),
      heartbeat_snapshots: StdMutex::new(Vec::new()),
      usage_observed: Arc::new(Notify::new()),
    }
  }
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
    if self.fence_on_heartbeat {
      return Ok(HeartbeatDirective::Fenced);
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
      },
      exit_code: if self.wait_for_cancel { 130 } else { 0 },
    }))
  }

  async fn cleanup_orphans(&self) -> Result<(), ExecutionError> {
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
      b"{\"type\":\"hello\",\"protocol_version\":1,\"octa_version\":\"0.3.0\",\"event_schema_version\":3,\"plugin_protocol_version\":1}\n",
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
      runner_protocol: 1,
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
    outputs: OutputLimits {
      artifact_count: 0,
      artifact_bytes: 0,
      report_count: 0,
      report_bytes: 0,
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
      runner_protocols: vec![1],
      event_schemas: vec![3],
      plugin_protocols: vec![1],
      octafile_versions: vec![1],
      platform: "any".to_owned(),
      features: Vec::new(),
      build_commit: None,
    },
    plugins: BTreeMap::new(),
  }
}

fn lifecycle_fixture(
  coordinator: Arc<LifecycleCoordinator>,
  state_root: &std::path::Path,
  work_root: &std::path::Path,
  wait_for_cancel: bool,
) -> (JobLifecycle, VerifiedLease, HostSnapshot) {
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
  let executor = Arc::new(
    JobExecutor::new(
      lifecycle_runner_installation(),
      Arc::new(LifecycleSource),
      BTreeMap::from([(
        RuntimeMode::Native,
        Arc::new(LifecycleBackend {
          wait_for_cancel,
          usage_observed: coordinator.usage_observed.clone(),
        }) as Arc<dyn ExecutionBackend>,
      )]),
      JobExecutorConfig {
        work_root: work_root.to_owned(),
        max_workspace_bytes: 2 * 1024 * 1024,
        cancellation_grace: Duration::from_secs(1),
        runner_supervision: RunnerSupervisionPolicy {
          resource_sample_interval: Duration::from_millis(1),
          ..RunnerSupervisionPolicy::default()
        },
      },
    )
    .unwrap(),
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
    coordinator,
    Registration {
      agent_id: "agent-1".to_owned(),
      registration_id: "registration-1".to_owned(),
      max_retry_delay: Duration::from_secs(1),
    },
    executor,
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
  (lifecycle, VerifiedLease { lease, spec }, snapshot)
}

#[tokio::test]
async fn completes_a_verified_job_after_durable_ordered_delivery_and_cleanup() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator::default());
  let (lifecycle, lease, snapshot) = lifecycle_fixture(coordinator.clone(), state_root.path(), work_root.path(), false);
  let outcome = lifecycle.run(lease, snapshot, CancellationToken::new()).await.unwrap();

  assert_eq!(outcome.status, JobCompletionStatus::Succeeded);
  assert!(!outcome.drain);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
  assert!(!state_root.path().join("jobs").read_dir().unwrap().any(|_| true));
  let events = coordinator.events.lock().unwrap();
  assert_eq!(
    events.iter().map(|event| event.stream_sequence).collect::<Vec<_>>(),
    (1..=outcome.last_event_sequence).collect::<Vec<_>>()
  );
  assert!(
    events
      .iter()
      .any(|event| matches!(event.kind, AttemptEventKind::Runner { .. }))
  );
  assert!(events.iter().any(|event| matches!(
    event.kind,
    AttemptEventKind::Agent {
      event: AgentLifecycleEvent::ResourceUsage { .. }
    }
  )));
  drop(events);
  let completions = coordinator.completions.lock().unwrap();
  assert_eq!(completions.len(), 1);
  assert_eq!(completions[0].last_event_sequence, outcome.last_event_sequence);
  assert_eq!(completions[0].status, JobCompletionStatus::Succeeded);
  assert!(coordinator.heartbeat_snapshots.lock().unwrap().iter().any(|snapshot| {
    snapshot
      .active_job
      .as_ref()
      .is_some_and(|job| job.resource_usage.is_some())
  }));
}

#[tokio::test]
async fn fencing_cancels_execution_and_retains_recoverable_attempt_state() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator {
    fence_on_heartbeat: true,
    ..LifecycleCoordinator::default()
  });
  let (lifecycle, lease, snapshot) = lifecycle_fixture(coordinator.clone(), state_root.path(), work_root.path(), true);

  let error = tokio::time::timeout(
    Duration::from_secs(5),
    lifecycle.run(lease, snapshot, CancellationToken::new()),
  )
  .await
  .unwrap()
  .unwrap_err();

  assert!(matches!(
    error,
    JobLifecycleError::LeaseLost(LeaseMonitorOutcome::Fenced)
  ));
  assert!(coordinator.completions.lock().unwrap().is_empty());
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
  let recovered = cleanup_incomplete_attempts(state_root.path()).unwrap();
  assert_eq!(recovered.len(), 1);
  assert_eq!(recovered[0].last_state, JobLifecycleState::Cleaning);
}

#[async_trait]
impl CoordinatorClient for ReplayCoordinator {
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
    _cancellation: CancellationToken,
  ) -> Result<AcquireLeaseResponse, CoordinatorError> {
    unreachable!()
  }

  async fn heartbeat(
    &self,
    _registration: &Registration,
    _lease: &octacity_protocol::LeaseAssignment,
    _snapshot: &HostSnapshot,
    _capacity: &HostCapacity,
    _lease_safety_margin: Duration,
    _cancellation: CancellationToken,
  ) -> Result<HeartbeatDirective, CoordinatorError> {
    unreachable!()
  }

  async fn append_events(
    &self,
    _registration: &Registration,
    _lease: &octacity_protocol::LeaseAssignment,
    events: &[octacity_protocol::AttemptEventEnvelope],
    _cancellation: CancellationToken,
  ) -> Result<AppendEventsResponse, CoordinatorError> {
    self
      .calls
      .lock()
      .unwrap()
      .push(events.iter().map(|event| event.stream_sequence).collect());
    if self.reject_permanently {
      return Err(CoordinatorError::Rejected {
        operation: "append job events",
        status: 403,
        code: "forbidden".to_owned(),
        message: "event append is not authorized".to_owned(),
        retryable: false,
      });
    }
    if self.fail_first.swap(false, Ordering::SeqCst) {
      return Err(CoordinatorError::Rejected {
        operation: "append job events",
        status: 503,
        code: "unavailable".to_owned(),
        message: "retry".to_owned(),
        retryable: true,
      });
    }
    Ok(AppendEventsResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: "mock".to_owned(),
      acknowledged_sequence: events.last().unwrap().stream_sequence,
    })
  }

  async fn complete_lease(
    &self,
    _registration: &Registration,
    _lease: &octacity_protocol::LeaseAssignment,
    _completion: &CompleteLeaseRequest,
    _cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    unreachable!()
  }
}

#[test]
fn recovery_removes_only_recognized_attempt_state() {
  let directory = tempfile::tempdir().unwrap();
  let jobs = directory.path().join("jobs");
  fs::create_dir(&jobs).unwrap();
  let fence = LeaseFence {
    lease_id: "lease-1".to_owned(),
    job_id: "job-1".to_owned(),
    attempt: 1,
    fencing_token: "fence-1".to_owned(),
  };
  let owned = jobs.join(attempt_directory(&fence));
  fs::create_dir(&owned).unwrap();
  let mut journal = JobJournal::create(&owned.join("journal.jsonl")).unwrap();
  journal.transition(JobLifecycleState::Preparing).unwrap();
  fs::create_dir(owned.join("events")).unwrap();
  fs::write(owned.join("events/00000000000000000001.json"), "event").unwrap();
  fs::write(owned.join("events/00000000000000000002.json"), "event").unwrap();
  fs::write(owned.join("completion.json"), "completion").unwrap();
  let foreign = jobs.join(format!("attempt-{}", "a".repeat(64)));
  fs::create_dir(&foreign).unwrap();
  fs::write(foreign.join("journal.jsonl"), "foreign").unwrap();

  let recovered = cleanup_incomplete_attempts(directory.path()).unwrap();
  assert_eq!(recovered.len(), 1);
  assert_eq!(recovered[0].attempt_id, attempt_directory(&fence));
  assert_eq!(recovered[0].last_state, JobLifecycleState::Preparing);
  assert_eq!(recovered[0].unacknowledged_events, 2);
  assert!(recovered[0].completion_persisted);
  assert!(!owned.exists());
  assert!(foreign.exists());
}

#[test]
fn stable_completion_identity_contains_no_server_controlled_text() {
  let fence = LeaseFence {
    lease_id: "../lease\n".to_owned(),
    job_id: "job".to_owned(),
    attempt: 1,
    fencing_token: "../../fence".to_owned(),
  };
  let attempt = attempt_directory(&fence);
  assert!(is_attempt_directory(&attempt));
  assert_eq!(completion_id(&fence).len(), 75);
}

#[tokio::test]
async fn network_failure_replays_without_loss_or_reordering() {
  const EVENT_COUNT: u64 = 12;

  let directory = tempfile::tempdir().unwrap();
  let attempt_root = directory.path().join("attempt");
  fs::create_dir(&attempt_root).unwrap();
  let fence = LeaseFence {
    lease_id: "lease-1".to_owned(),
    job_id: "job-1".to_owned(),
    attempt: 1,
    fencing_token: "fence-1".to_owned(),
  };
  let spool = Arc::new(Mutex::new(
    EventSpool::create(
      attempt_root,
      fence.clone(),
      SpoolLimits {
        max_bytes: 16 * 1024,
        max_records: 3,
        batch_bytes: 8 * 1024,
        batch_records: 2,
      },
    )
    .unwrap(),
  ));
  let coordinator = Arc::new(ReplayCoordinator {
    fail_first: AtomicBool::new(true),
    reject_permanently: false,
    calls: StdMutex::new(Vec::new()),
  });
  let lease = octacity_protocol::LeaseAssignment {
    lease_id: fence.lease_id.clone(),
    job_id: fence.job_id.clone(),
    attempt: fence.attempt,
    fencing_token: fence.fencing_token.clone(),
    issued_at: 1,
    expires_at: u64::MAX,
    signed_job_spec: octacity_protocol::SignedEnvelope {
      key_id: "unused".to_owned(),
      algorithm: "unused".to_owned(),
      payload: String::new(),
      signature: String::new(),
    },
  };
  let work_available = Arc::new(Notify::new());
  let (progress_sender, progress) = watch::channel(0_u64);
  let stop = CancellationToken::new();
  let delivery = DeliveryTask {
    coordinator: coordinator.clone(),
    registration: Registration {
      agent_id: "agent-1".to_owned(),
      registration_id: "registration-1".to_owned(),
      max_retry_delay: Duration::from_secs(1),
    },
    lease,
    spool: spool.clone(),
    work_available: work_available.clone(),
    progress: progress_sender,
    retry_delay: Duration::from_millis(1),
    stop: stop.clone(),
  }
  .spawn();
  // More than three spool capacities exercises repeated backpressure and
  // batching without making this correctness test a durable-disk benchmark.
  tokio::time::timeout(Duration::from_secs(30), async {
    for runner_sequence in 1..=EVENT_COUNT {
      append(
        &spool,
        &work_available,
        &progress,
        AttemptEventKind::Runner {
          event: RunnerEventPayload {
            schema_version: 3,
            sequence: runner_sequence,
            timestamp: "2026-09-11T12:00:00Z".to_owned(),
            category: "stdout".to_owned(),
            data: serde_json::Map::new(),
          },
        },
        &stop,
      )
      .await
      .unwrap();
    }
    flush(&spool, &work_available, &progress, &stop).await.unwrap();
  })
  .await
  .unwrap_or_else(|_| panic!("delivery timed out after calls {:?}", coordinator.calls.lock().unwrap()));
  stop.cancel();
  work_available.notify_waiters();
  delivery.await.unwrap().unwrap();

  let calls = coordinator.calls.lock().unwrap();
  let minimum_calls = EVENT_COUNT.div_ceil(2) as usize + 1;
  assert!(calls.len() >= minimum_calls);
  assert_eq!(calls[1], calls[0]);
  assert!(
    calls
      .iter()
      .all(|batch| batch.windows(2).all(|pair| pair[1] == pair[0] + 1))
  );
  let delivered = calls.iter().skip(1).flatten().copied().collect::<Vec<_>>();
  assert_eq!(delivered, (1..=EVENT_COUNT).collect::<Vec<_>>());
}

#[tokio::test]
async fn permanent_event_rejection_stops_delivery_without_a_retry_loop() {
  let directory = tempfile::tempdir().unwrap();
  let attempt_root = directory.path().join("attempt");
  fs::create_dir(&attempt_root).unwrap();
  let fence = LeaseFence {
    lease_id: "lease-1".to_owned(),
    job_id: "job-1".to_owned(),
    attempt: 1,
    fencing_token: "fence-1".to_owned(),
  };
  let spool = Arc::new(Mutex::new(
    EventSpool::create(
      attempt_root,
      fence.clone(),
      SpoolLimits {
        max_bytes: 16 * 1024,
        max_records: 3,
        batch_bytes: 8 * 1024,
        batch_records: 2,
      },
    )
    .unwrap(),
  ));
  spool
    .lock()
    .await
    .append(AttemptEventKind::Agent {
      event: AgentLifecycleEvent::StateChanged {
        state: JobLifecycleState::Preparing,
      },
    })
    .unwrap();
  let coordinator = Arc::new(ReplayCoordinator {
    fail_first: AtomicBool::new(false),
    reject_permanently: true,
    calls: StdMutex::new(Vec::new()),
  });
  let work_available = Arc::new(Notify::new());
  let (progress, _) = watch::channel(0_u64);
  let stop = CancellationToken::new();
  let delivery = DeliveryTask {
    coordinator: coordinator.clone(),
    registration: Registration {
      agent_id: "agent-1".to_owned(),
      registration_id: "registration-1".to_owned(),
      max_retry_delay: Duration::from_secs(1),
    },
    lease: octacity_protocol::LeaseAssignment {
      lease_id: fence.lease_id,
      job_id: fence.job_id,
      attempt: fence.attempt,
      fencing_token: fence.fencing_token,
      issued_at: 1,
      expires_at: u64::MAX,
      signed_job_spec: octacity_protocol::SignedEnvelope {
        key_id: "unused".to_owned(),
        algorithm: "unused".to_owned(),
        payload: String::new(),
        signature: String::new(),
      },
    },
    spool,
    work_available: work_available.clone(),
    progress,
    retry_delay: Duration::from_millis(1),
    stop,
  }
  .spawn();
  work_available.notify_one();

  let error = tokio::time::timeout(Duration::from_secs(1), delivery)
    .await
    .unwrap()
    .unwrap()
    .unwrap_err();
  assert!(matches!(
    error,
    JobLifecycleError::Coordinator(CoordinatorError::Rejected { retryable: false, .. })
  ));
  assert_eq!(coordinator.calls.lock().unwrap().len(), 1);
}

#[test]
fn validates_lifecycle_configuration_boundaries() {
  let state = tempfile::tempdir().unwrap();
  let valid = JobLifecycleConfig {
    state_root: state.path().to_owned(),
    event_channel_capacity: 1,
    event_retry_delay: Duration::from_millis(1),
    spool: SpoolLimits {
      max_bytes: 1024,
      max_records: 2,
      batch_bytes: 512,
      batch_records: 1,
    },
    lease_monitor: LeaseMonitorPolicy {
      heartbeat_interval: Duration::from_millis(1),
      lease_safety_margin: Duration::from_millis(2),
    },
  };
  assert!(valid.validate().is_ok());

  let mut invalid = valid.clone();
  invalid.state_root = PathBuf::from("relative");
  assert!(invalid.validate().unwrap_err().to_string().contains("state_root"));
  invalid = valid.clone();
  invalid.event_channel_capacity = 0;
  assert!(invalid.validate().unwrap_err().to_string().contains("channel capacity"));
  invalid = valid.clone();
  invalid.spool.batch_records = 3;
  assert!(invalid.validate().unwrap_err().to_string().contains("batch"));
  invalid = valid;
  invalid.lease_monitor.heartbeat_interval = invalid.lease_monitor.lease_safety_margin;
  assert!(invalid.validate().unwrap_err().to_string().contains("shorter"));
}

#[test]
fn maps_runner_items_and_terminal_statuses_without_losing_meaning() {
  assert_eq!(runner_status(RunStatus::Failed), JobCompletionStatus::Failed);
  assert_eq!(job_error_status(&JobError::Cancelled), JobCompletionStatus::Cancelled);
  assert_eq!(job_error_status(&JobError::TimedOut), JobCompletionStatus::TimedOut);
  assert_eq!(
    job_error_status(&JobError::Invalid("broken".to_owned())),
    JobCompletionStatus::InfrastructureFailed
  );
  assert!(cleanup_confirmed(&JobError::Cancelled));
  assert!(!cleanup_confirmed(&JobError::Cleanup(std::io::Error::other("cleanup"))));
  assert!(!cleanup_confirmed(&JobError::OrphanCleanup("cleanup".to_owned())));
  assert!(!cleanup_confirmed(&JobError::OperationAndCleanup {
    operation: Box::new(JobError::Cancelled),
    cleanup: std::io::Error::other("cleanup"),
  }));

  assert!(matches!(
    stream_kind(RunnerStreamItem::AccountingUnavailable {
      consecutive_failures: 3,
    }),
    AttemptEventKind::Agent {
      event: AgentLifecycleEvent::AccountingUnavailable {
        consecutive_failures: 3
      }
    }
  ));
  assert!(!fatal_lease(&LeaseMonitorOutcome::Cancelled));
  assert!(!fatal_lease(&LeaseMonitorOutcome::Stopped));
  assert!(fatal_lease(&LeaseMonitorOutcome::Fenced));
  assert!(fatal_lease(&LeaseMonitorOutcome::Expired));
  assert!(fatal_lease(&LeaseMonitorOutcome::Shutdown));
}

#[tokio::test]
async fn classifies_delivery_task_termination_and_stops_only_once() {
  assert!(matches!(
    delivery_failure(Ok(Ok(()))),
    JobLifecycleError::Invalid(message) if message.contains("stopped before")
  ));
  assert!(matches!(
    delivery_failure(Ok(Err(JobLifecycleError::Invalid("delivery".to_owned())))),
    JobLifecycleError::Invalid(message) if message == "delivery"
  ));
  let join_error = tokio::spawn(async { panic!("delivery panic") }).await.unwrap_err();
  assert!(matches!(delivery_failure(Err(join_error)), JobLifecycleError::Join(_)));

  let stop = CancellationToken::new();
  let work = Notify::new();
  let mut task = tokio::spawn(async { Ok(()) });
  let mut finished = true;
  stop_delivery(&stop, &work, &mut task, &mut finished).await.unwrap();
  assert!(stop.is_cancelled());
  task.await.unwrap().unwrap();
}
