#[tokio::test]
async fn completes_a_verified_job_after_durable_ordered_delivery_and_cleanup() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator::default());
  let outputs = Arc::new(RecordingOutputPublisher {
    called: AtomicBool::new(false),
  });
  let (_cache_root, lifecycle, lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    outputs.clone(),
    state_root.path(),
    work_root.path(),
    false,
  );
  let outcome = lifecycle.run(lease, snapshot, CancellationToken::new()).await.unwrap();

  assert_eq!(outcome.status, JobCompletionStatus::Succeeded);
  assert!(!outcome.drain);
  assert!(outputs.called.load(Ordering::SeqCst));
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
  assert_eq!(
    lifecycle_states(&events),
    [
      JobLifecycleState::Preparing,
      JobLifecycleState::Running,
      JobLifecycleState::Freezing,
      JobLifecycleState::Uploading,
      JobLifecycleState::Cleaning,
      JobLifecycleState::Completing,
    ]
  );
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
async fn brackets_a_cache_enabled_execution_with_fenced_session_operations() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator::default());
  let (cache_root, lifecycle, mut lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(NoopOutputPublisher),
    state_root.path(),
    work_root.path(),
    false,
  );
  let octacity_protocol::VerifiedJobSpec::V1(spec) = &mut lease.spec else {
    panic!("fixture must use the legacy JobSpec");
  };
  spec.cache = Some(octacity_protocol::CachePolicy {
    namespace: "project/main".to_owned(),
    read: true,
    write: true,
  });

  let outcome = lifecycle.run(lease, snapshot, CancellationToken::new()).await.unwrap();
  assert_eq!(outcome.status, JobCompletionStatus::Succeeded);
  let begins = coordinator.cache_begins.lock().unwrap();
  let revocations = coordinator.cache_revocations.lock().unwrap();
  assert_eq!(begins.len(), 1);
  assert_eq!(revocations.len(), 1);
  assert_eq!(begins[0].lease, revocations[0].lease);
  assert_eq!(revocations[0].session_id, "cache-session-1");
  assert_ne!(begins[0].request_id, revocations[0].request_id);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
  assert!(cache_root.path().join("v1").is_dir());
}

#[tokio::test]
async fn cache_begin_failure_stops_before_execution_and_removes_attempt_state() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator {
    cache_begin_failure: true,
    ..LifecycleCoordinator::default()
  });
  let (_cache_root, lifecycle, mut lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(NoopOutputPublisher),
    state_root.path(),
    work_root.path(),
    false,
  );
  let octacity_protocol::VerifiedJobSpec::V1(spec) = &mut lease.spec else {
    panic!("fixture must use the legacy JobSpec");
  };
  spec.cache = Some(octacity_protocol::CachePolicy {
    namespace: "project/main".to_owned(),
    read: true,
    write: true,
  });

  assert!(matches!(
    lifecycle.run(lease, snapshot, CancellationToken::new()).await,
    Err(JobLifecycleError::Coordinator(CoordinatorError::Invalid(message)))
      if message.contains("cache begin")
  ));
  assert_eq!(coordinator.cache_begins.lock().unwrap().len(), 1);
  assert!(coordinator.cache_revocations.lock().unwrap().is_empty());
  assert!(coordinator.completions.lock().unwrap().is_empty());
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
  assert!(!state_root.path().join("jobs").read_dir().unwrap().any(|_| true));
}

#[tokio::test]
async fn cache_revoke_failure_cleans_the_job_but_refuses_terminal_completion() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator {
    cache_revoke_failure: true,
    ..LifecycleCoordinator::default()
  });
  let (_cache_root, lifecycle, mut lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(NoopOutputPublisher),
    state_root.path(),
    work_root.path(),
    false,
  );
  let octacity_protocol::VerifiedJobSpec::V1(spec) = &mut lease.spec else {
    panic!("fixture must use the legacy JobSpec");
  };
  spec.cache = Some(octacity_protocol::CachePolicy {
    namespace: "project/main".to_owned(),
    read: true,
    write: true,
  });

  assert!(matches!(
    lifecycle.run(lease, snapshot, CancellationToken::new()).await,
    Err(JobLifecycleError::Coordinator(CoordinatorError::Invalid(message)))
      if message.contains("cache revoke")
  ));
  assert_eq!(coordinator.cache_revocations.lock().unwrap().len(), 1);
  assert!(coordinator.completions.lock().unwrap().is_empty());
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn panicked_cache_job_cleans_orphans_then_revokes_the_server_session() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator::default());
  coordinator.panic_backend.store(true, Ordering::SeqCst);
  let (_cache_root, lifecycle, mut lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(NoopOutputPublisher),
    state_root.path(),
    work_root.path(),
    false,
  );
  let octacity_protocol::VerifiedJobSpec::V1(spec) = &mut lease.spec else {
    panic!("fixture must use the legacy JobSpec");
  };
  spec.cache = Some(octacity_protocol::CachePolicy {
    namespace: "project/main".to_owned(),
    read: true,
    write: true,
  });

  assert!(matches!(
    lifecycle.run(lease, snapshot, CancellationToken::new()).await,
    Err(JobLifecycleError::Join(_))
  ));
  assert!(coordinator.orphan_cleanup_observed.load(Ordering::SeqCst));
  assert_eq!(coordinator.cache_revocations.lock().unwrap().len(), 1);
  assert!(coordinator.completions.lock().unwrap().is_empty());
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn unpublished_invalid_output_is_discarded_and_its_workspace_is_cleaned() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator::default());
  let (_cache_root, lifecycle, lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(FailingOutputPublisher),
    state_root.path(),
    work_root.path(),
    false,
  );

  let outcome = lifecycle.run(lease, snapshot, CancellationToken::new()).await.unwrap();

  assert_eq!(outcome.status, JobCompletionStatus::InfrastructureFailed);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
  assert!(!state_root.path().join("jobs").read_dir().unwrap().any(|_| true));
  let completions = coordinator.completions.lock().unwrap();
  assert_eq!(completions.len(), 1);
  assert_eq!(completions[0].status, JobCompletionStatus::InfrastructureFailed);
  assert!(completions[0].results.is_empty());
  drop(completions);
  assert_eq!(
    lifecycle_states(&coordinator.events.lock().unwrap()),
    [
      JobLifecycleState::Preparing,
      JobLifecycleState::Running,
      JobLifecycleState::Freezing,
      JobLifecycleState::Cleaning,
      JobLifecycleState::Completing,
    ]
  );
}

#[tokio::test]
async fn partial_upload_discards_results_and_cleans_the_disposable_workspace() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator::default());
  let outputs = Arc::new(PartialUploadPublisher {
    uploaded_components: AtomicUsize::new(0),
  });
  let (_cache_root, lifecycle, lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    outputs.clone(),
    state_root.path(),
    work_root.path(),
    false,
  );

  let outcome = lifecycle.run(lease, snapshot, CancellationToken::new()).await.unwrap();

  assert_eq!(outcome.status, JobCompletionStatus::InfrastructureFailed);
  assert_eq!(outputs.uploaded_components.load(Ordering::SeqCst), 1);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
  let completions = coordinator.completions.lock().unwrap();
  assert_eq!(completions.len(), 1);
  assert!(completions[0].results.is_empty());
  drop(completions);
  assert_eq!(
    lifecycle_states(&coordinator.events.lock().unwrap()),
    [
      JobLifecycleState::Preparing,
      JobLifecycleState::Running,
      JobLifecycleState::Freezing,
      JobLifecycleState::Uploading,
      JobLifecycleState::Cleaning,
      JobLifecycleState::Completing,
    ]
  );
}

#[tokio::test]
async fn fenced_output_endpoint_preserves_the_lease_loss_reason() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator::default());
  let (_cache_root, lifecycle, lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(FencedOutputPublisher),
    state_root.path(),
    work_root.path(),
    false,
  );

  let error = lifecycle
    .run(lease, snapshot, CancellationToken::new())
    .await
    .unwrap_err();

  assert!(matches!(
    error,
    JobLifecycleError::LeaseLost(LeaseMonitorOutcome::Fenced)
  ));
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
  assert!(coordinator.completions.lock().unwrap().is_empty());
}

#[tokio::test]
async fn fencing_cancels_execution_and_retains_recoverable_attempt_state() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator {
    fence_on_heartbeat: true,
    ..LifecycleCoordinator::default()
  });
  let (_cache_root, lifecycle, lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(NoopOutputPublisher),
    state_root.path(),
    work_root.path(),
    true,
  );

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
  // Cleaning is synced before the workspace is removed, even when fencing
  // prevents the terminal acknowledgement from reaching the coordinator.
  assert_eq!(recovered[0].last_state, JobLifecycleState::Cleaning);
}

#[tokio::test]
async fn cancellation_does_not_discard_outputs_produced_before_runner_exit() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator {
    cancel_on_heartbeat: true,
    ..LifecycleCoordinator::default()
  });
  let outputs = Arc::new(CancellationAwareOutputPublisher {
    called: AtomicBool::new(false),
  });
  let (_cache_root, lifecycle, lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    outputs.clone(),
    state_root.path(),
    work_root.path(),
    true,
  );

  let outcome = tokio::time::timeout(
    Duration::from_secs(5),
    lifecycle.run(lease, snapshot, CancellationToken::new()),
  )
  .await
  .unwrap()
  .unwrap();

  assert_eq!(outcome.status, JobCompletionStatus::Cancelled);
  assert!(outputs.called.load(Ordering::SeqCst));
  assert_eq!(coordinator.completions.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn fencing_during_output_publication_keeps_the_lease_loss_reason() {
  for freeze_delay in [Duration::ZERO, Duration::from_secs(6)] {
    let state_root = tempfile::tempdir().unwrap();
    let work_root = tempfile::tempdir().unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let publication_started = Arc::new(StdMutex::new(None));
    let coordinator = Arc::new(LifecycleCoordinator {
      fence_after_output_starts: Some(started.clone()),
      ..LifecycleCoordinator::default()
    });
    let (_cache_root, lifecycle, lease, snapshot) = lifecycle_fixture(
      coordinator.clone(),
      Arc::new(BlockingOutputPublisher {
        started,
        freeze_delay,
        publication_started: publication_started.clone(),
      }),
      state_root.path(),
      work_root.path(),
      false,
    );

    // Bound setup separately from the fencing response; slow disk work before
    // publication must not consume the reaction deadline checked below.
    let error = tokio::time::timeout(
      Duration::from_secs(30),
      lifecycle.run(lease, snapshot, CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap_err();

    assert!(
      publication_started
        .lock()
        .unwrap()
        .expect("publication must start")
        .elapsed()
        < Duration::from_secs(5),
      "fencing must finish within five seconds after publication starts"
    );
    assert_eq!(
      error.to_string(),
      JobLifecycleError::LeaseLost(LeaseMonitorOutcome::Fenced).to_string()
    );
    assert!(matches!(
      error,
      JobLifecycleError::LeaseLost(LeaseMonitorOutcome::Fenced)
    ));
    assert!(coordinator.completions.lock().unwrap().is_empty());
  }
}
struct CredentialOutputPublisher {
  contents: &'static [u8],
  published: AtomicBool,
}

struct UnusedUploadCoordinator;
#[async_trait]
impl octacity_coordinator::OutputUploadCoordinator for UnusedUploadCoordinator {
  async fn begin_output_upload(
    &self,
    _: &Registration,
    _: &LeaseAssignment,
    _: &octacity_protocol::BeginOutputUploadRequest,
    _: CancellationToken,
  ) -> Result<octacity_protocol::BeginOutputUploadResponse, CoordinatorError> {
    panic!("the lifecycle fixture does not request upload capabilities")
  }
  async fn complete_output_upload(
    &self,
    _: &Registration,
    _: &LeaseAssignment,
    _: &octacity_protocol::CompleteOutputUploadRequest,
    _: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    panic!("the lifecycle fixture does not complete uploads")
  }
}
#[async_trait]
impl OutputPublisher for CredentialOutputPublisher {
  async fn freeze(
    &self,
    request: FreezeOutputs<'_>,
    cancellation: CancellationToken,
  ) -> Result<FrozenOutputs, OutputError> {
    fs::write(request.workspace.join("result.txt"), self.contents).unwrap();
    let publisher = octacity_output::PresignedOutputPublisher::new(
      Arc::new(UnusedUploadCoordinator),
      octacity_output::PresignedOutputPublisherConfig {
        allowed_origins: vec!["http://127.0.0.1:1".into()],
        ca_certificate_file: None,
        max_archive_entries: 16,
        upload_timeout: Duration::from_secs(2),
        retry: octacity_coordinator::RetryPolicy {
          max_attempts: 1,
          initial_delay: Duration::from_millis(1),
          max_delay: Duration::from_millis(1),
        },
      },
    )
    .unwrap();
    let results =
      vec![serde_json::json!({"run_id":1,"tasks":[{"task_id":1,"artifacts":[{"name":"result","path":"result.txt"}]}]})];
    publisher
      .freeze(
        FreezeOutputs {
          results: &results,
          ..request
        },
        cancellation,
      )
      .await
  }
  async fn publish(&self, request: PublishOutputs<'_>, _: CancellationToken) -> Result<(), OutputError> {
    self.published.store(true, Ordering::SeqCst);
    drop(request);
    Ok(())
  }
}

#[tokio::test]
async fn credential_bearing_output_stops_before_uploading_and_cleans_the_job() {
  for (contents, expected) in [
    (
      b"workload-private-output-token".as_slice(),
      JobCompletionStatus::InfrastructureFailed,
    ),
    (b"ordinary retained result".as_slice(), JobCompletionStatus::Succeeded),
  ] {
    let state_root = tempfile::tempdir().unwrap();
    let work_root = tempfile::tempdir().unwrap();
    let credentials = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(credentials.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    let source = credentials.path().canonicalize().unwrap().join("identity");
    fs::write(&source, b"workload-private-output-token").unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let coordinator = Arc::new(LifecycleCoordinator::default());
    let outputs = Arc::new(CredentialOutputPublisher {
      contents,
      published: AtomicBool::new(false),
    });
    let (_cache, lifecycle, mut lease, snapshot) = lifecycle_fixture_with_identity(
      coordinator.clone(),
      outputs.clone(),
      state_root.path(),
      work_root.path(),
      false,
      Arc::new(FileWorkloadIdentityProvider::new(BTreeMap::from([(
        "private".into(),
        source,
      )]))),
    );
    let octacity_protocol::VerifiedJobSpec::V1(spec) = &mut lease.spec else {
      unreachable!()
    };
    spec.runtime.workload_identity_profile = Some("private".into());
    spec.outputs.artifact_count = 1;
    spec.outputs.artifact_bytes = 1024;
    spec.outputs.single_output_bytes = 1024;
    let outcome = lifecycle.run(lease, snapshot, CancellationToken::new()).await.unwrap();
    assert_eq!(outcome.status, expected);
    assert_eq!(
      outputs.published.load(Ordering::SeqCst),
      expected == JobCompletionStatus::Succeeded
    );
    assert_eq!(
      lifecycle_states(&coordinator.events.lock().unwrap()).contains(&JobLifecycleState::Uploading),
      expected == JobCompletionStatus::Succeeded
    );
    assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
    assert!(coordinator.completions.lock().unwrap()[0].results.is_empty());
  }
}
