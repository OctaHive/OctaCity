struct ScriptedClient {
  leases: Mutex<VecDeque<Result<AcquireLeaseResponse, CoordinatorError>>>,
  heartbeats: Mutex<VecDeque<Result<HeartbeatDirective, CoordinatorError>>>,
}

#[async_trait]
impl CoordinatorClient for ScriptedClient {
  async fn register(
    &self,
    _inventory: &AgentInventory,
    _cancellation: CancellationToken,
  ) -> Result<Registration, CoordinatorError> {
    Ok(registration())
  }

  async fn acquire_lease(
    &self,
    _registration: &Registration,
    _wait: Duration,
    _lease_safety_margin: Duration,
    _accept_jobs: bool,
    _snapshot: &HostSnapshot,
    cancellation: CancellationToken,
  ) -> Result<AcquireLeaseResponse, CoordinatorError> {
    if let Some(result) = self.leases.lock().unwrap().pop_front() {
      return result;
    }
    cancellation.cancelled().await;
    Err(CoordinatorError::Cancelled)
  }

  async fn heartbeat(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    _snapshot: &HostSnapshot,
    _capacity: &HostCapacity,
    _lease_safety_margin: Duration,
    cancellation: CancellationToken,
  ) -> Result<HeartbeatDirective, CoordinatorError> {
    if let Some(result) = self.heartbeats.lock().unwrap().pop_front() {
      return result;
    }
    cancellation.cancelled().await;
    Err(CoordinatorError::Cancelled)
  }

  async fn append_events(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    _events: &[AttemptEventEnvelope],
    _cancellation: CancellationToken,
  ) -> Result<AppendEventsResponse, CoordinatorError> {
    unreachable!("event delivery is not used by lease-monitor tests")
  }

  async fn complete_lease(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    _completion: &CompleteLeaseRequest,
    _cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    unreachable!("completion is not used by lease-monitor tests")
  }
}

fn signed_lease(signing_key: &SigningKey, now: u64) -> LeaseAssignment {
  let spec = JobSpecV1 {
    protocol_version: 1,
    job_id: "job-1".to_owned(),
    attempt: 1,
    issued_at: now.saturating_sub(1),
    expires_at: now + 300,
    source: SourceSpec {
      provider: "git".to_owned(),
      plugin_version: "0.1.0".to_owned(),
      plugin_sha256: "1".repeat(64),
      revision: "a".repeat(40),
      reference: None,
      parameters: BTreeMap::from([("url".to_owned(), json!("https://example.com/repository.git"))]),
    },
    octa: OctaSpec {
      version: "0.3.0".to_owned(),
      runner_sha256: "2".repeat(64),
      runner_protocol: 1,
      event_schema: 1,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::new(),
    },
    execution: ExecutionSpec {
      octafile: None,
      commands: vec!["build".to_owned()],
      variables: BTreeMap::new(),
      arguments: Vec::new(),
      concurrency: None,
      parallel: false,
      failfast: true,
      secrets_profile: None,
    },
    runtime: RuntimeSpec {
      target: RuntimeTarget::Oci {
        platform: platform(),
        isolation: OciIsolation::Hypervisor,
        image: format!("example/build@sha256:{}", "3".repeat(64)),
      },
      cpu_millis: 1000,
      memory_bytes: 1024,
      writable_disk_bytes: 1024,
      timeout_seconds: 60,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    },
    cache: None,
    outputs: OutputLimits {
      artifact_count: 1,
      artifact_bytes: 1024,
      report_count: 1,
      report_bytes: 1024,
      single_output_bytes: 1024,
    },
  };
  let payload = serde_json::to_vec(&spec).unwrap();
  LeaseAssignment {
    lease_id: "lease-1".to_owned(),
    job_id: spec.job_id.clone(),
    attempt: spec.attempt,
    fencing_token: "fence-1".to_owned(),
    issued_at: now.saturating_sub(1),
    expires_at: now + 120,
    signed_job_spec: SignedEnvelope {
      key_id: "primary".to_owned(),
      algorithm: SIGNATURE_ALGORITHM.to_owned(),
      payload: BASE64.encode(&payload),
      signature: BASE64.encode(signing_key.sign(&payload).to_bytes()),
    },
  }
}

fn unix_now() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs()
}

#[tokio::test]
async fn poller_exposes_no_work_then_a_verified_lease() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let lease = signed_lease(&signing_key, unix_now());
  let client = Arc::new(ScriptedClient {
    leases: Mutex::new(VecDeque::from([
      Ok(AcquireLeaseResponse::NoWork {
        protocol_version: 1,
        request_id: "poll-1".to_owned(),
        retry_after_ms: 1,
      }),
      Ok(AcquireLeaseResponse::Lease {
        protocol_version: 1,
        request_id: "poll-2".to_owned(),
        lease,
      }),
    ])),
    heartbeats: Mutex::new(VecDeque::new()),
  });
  let keys: BTreeMap<String, VerifyingKey> = BTreeMap::from([("primary".to_owned(), signing_key.verifying_key())]);
  let poller = LeasePoller::new(
    client,
    registration(),
    Arc::new(keys),
    Duration::from_secs(1),
    Duration::from_secs(10),
  )
  .unwrap();

  assert!(matches!(
    poller.next(true, &snapshot(), CancellationToken::new()).await.unwrap(),
    LeasePollOutcome::NoWork { retry_after } if retry_after == Duration::from_millis(1)
  ));
  let LeasePollOutcome::Lease(lease) = poller.next(true, &snapshot(), CancellationToken::new()).await.unwrap() else {
    panic!("expected a lease");
  };
  assert_eq!(lease.spec.job_id, "job-1");
  assert_eq!(lease.lease.fencing_token, "fence-1");
}

#[tokio::test]
async fn poller_rejects_a_job_signature_that_does_not_match_the_lease() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let mut lease = signed_lease(&signing_key, unix_now());
  lease.signed_job_spec.signature = BASE64.encode([0_u8; 64]);
  let client = Arc::new(ScriptedClient {
    leases: Mutex::new(VecDeque::from([Ok(AcquireLeaseResponse::Lease {
      protocol_version: 1,
      request_id: "poll-1".to_owned(),
      lease,
    })])),
    heartbeats: Mutex::new(VecDeque::new()),
  });
  let keys = BTreeMap::from([("primary".to_owned(), signing_key.verifying_key())]);
  let poller = LeasePoller::new(
    client,
    registration(),
    Arc::new(keys),
    Duration::from_secs(1),
    Duration::from_secs(10),
  )
  .unwrap();
  assert!(matches!(
    poller.next(true, &snapshot(), CancellationToken::new()).await,
    Err(CoordinatorError::JobSpec(_))
  ));
}

#[tokio::test]
async fn poller_rejects_a_lease_while_local_admission_is_paused() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let lease = signed_lease(&signing_key, unix_now());
  let client = Arc::new(ScriptedClient {
    leases: Mutex::new(VecDeque::from([Ok(AcquireLeaseResponse::Lease {
      protocol_version: 1,
      request_id: "poll-1".to_owned(),
      lease,
    })])),
    heartbeats: Mutex::new(VecDeque::new()),
  });
  let keys = BTreeMap::from([("primary".to_owned(), signing_key.verifying_key())]);
  let poller = LeasePoller::new(
    client,
    registration(),
    Arc::new(keys),
    Duration::from_secs(1),
    Duration::from_secs(10),
  )
  .unwrap();

  assert!(matches!(
    poller.next(false, &snapshot(), CancellationToken::new()).await,
    Err(CoordinatorError::Invalid(message)) if message.contains("not accepting jobs")
  ));
}

#[tokio::test]
async fn heartbeat_keeps_drain_sticky_then_fences_and_cancels_the_job() {
  let now = unix_now();
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let lease = signed_lease(&signing_key, now);
  let client = Arc::new(ScriptedClient {
    leases: Mutex::new(VecDeque::new()),
    heartbeats: Mutex::new(VecDeque::from([
      Ok(HeartbeatDirective::Drain { expires_at: now + 120 }),
      Ok(HeartbeatDirective::Fenced),
    ])),
  });
  let (_snapshot_sender, snapshots) = watch::channel(snapshot());
  let monitor = LeaseMonitor::start(
    client,
    registration(),
    lease,
    capacity(),
    snapshots,
    LeaseMonitorPolicy {
      heartbeat_interval: Duration::from_millis(10),
      lease_safety_margin: Duration::from_secs(10),
    },
    CancellationToken::new(),
  )
  .unwrap();
  let cancellation = monitor.job_cancellation();
  let draining = monitor.draining();
  let outcome = monitor.wait().await.unwrap();

  assert!(matches!(outcome, LeaseMonitorOutcome::Fenced));
  assert!(cancellation.is_cancelled());
  assert!(*draining.borrow());
}

#[tokio::test]
async fn heartbeat_failure_cancels_at_the_lease_safety_deadline() {
  let now = unix_now();
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let mut lease = signed_lease(&signing_key, now);
  lease.expires_at = now + 2;
  let failures = (0..20)
    .map(|_| {
      Err(CoordinatorError::Rejected {
        operation: "heartbeat lease",
        status: 503,
        code: "unavailable".to_owned(),
        message: "retry".to_owned(),
        retryable: true,
      })
    })
    .collect();
  let client = Arc::new(ScriptedClient {
    leases: Mutex::new(VecDeque::new()),
    heartbeats: Mutex::new(failures),
  });
  let (_snapshot_sender, snapshots) = watch::channel(snapshot());
  let monitor = LeaseMonitor::start(
    client,
    registration(),
    lease,
    capacity(),
    snapshots,
    LeaseMonitorPolicy {
      heartbeat_interval: Duration::from_millis(50),
      lease_safety_margin: Duration::from_secs(1),
    },
    CancellationToken::new(),
  )
  .unwrap();
  let cancellation = monitor.job_cancellation();
  let outcome = monitor.wait().await.unwrap();

  assert!(matches!(outcome, LeaseMonitorOutcome::Expired));
  assert!(cancellation.is_cancelled());
}
