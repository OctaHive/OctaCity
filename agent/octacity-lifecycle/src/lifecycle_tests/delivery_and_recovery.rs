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
    _accept_jobs: bool,
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
