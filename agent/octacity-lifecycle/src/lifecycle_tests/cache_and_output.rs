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
  lease.spec.cache = Some(octacity_protocol::CachePolicy {
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
  lease.spec.cache = Some(octacity_protocol::CachePolicy {
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
  lease.spec.cache = Some(octacity_protocol::CachePolicy {
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
  lease.spec.cache = Some(octacity_protocol::CachePolicy {
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
async fn invalid_output_completes_as_infrastructure_failure_without_entering_uploading() {
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
async fn upload_failure_completes_as_infrastructure_failure_after_uploading() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator::default());
  let (_cache_root, lifecycle, lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(FailingUploadPublisher),
    state_root.path(),
    work_root.path(),
    false,
  );

  let outcome = lifecycle.run(lease, snapshot, CancellationToken::new()).await.unwrap();

  assert_eq!(outcome.status, JobCompletionStatus::InfrastructureFailed);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
  assert_eq!(coordinator.completions.lock().unwrap().len(), 1);
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
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let started = Arc::new(AtomicBool::new(false));
  let coordinator = Arc::new(LifecycleCoordinator {
    fence_after_output_starts: Some(started.clone()),
    ..LifecycleCoordinator::default()
  });
  let (_cache_root, lifecycle, lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(BlockingOutputPublisher { started }),
    state_root.path(),
    work_root.path(),
    false,
  );

  let error = tokio::time::timeout(
    Duration::from_secs(5),
    lifecycle.run(lease, snapshot, CancellationToken::new()),
  )
  .await
  .unwrap()
  .unwrap_err();

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
