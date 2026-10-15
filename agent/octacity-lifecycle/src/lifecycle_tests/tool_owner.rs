struct RecordingLeaseToolProvider {
  leases: StdMutex<Vec<(String, String, u32, String)>>,
}
impl FactoryToolExecutionProvider for RecordingLeaseToolProvider {
  fn bind(
    &self,
    registration: &Registration,
    lease: &octacity_protocol::LeaseAssignment,
  ) -> Arc<dyn octacity_job::FactoryToolExecutionOwner> {
    self.leases.lock().unwrap().push((
      registration.registration_id.clone(),
      lease.job_id.clone(),
      lease.attempt,
      lease.fencing_token.clone(),
    ));
    Arc::new(NoOrdinaryToolOwner)
  }
}
struct NoOrdinaryToolOwner;
impl octacity_job::FactoryToolExecutionOwner for NoOrdinaryToolOwner {
  fn prepare(
    &self,
    _: octacity_job::FactoryToolExecutionScope,
  ) -> Result<Arc<dyn ExecutionBackend>, octacity_job::FactoryPreflightError> {
    panic!("ordinary Jobs must never acquire controlled execution authority")
  }
}
#[tokio::test]
async fn configured_tool_provider_is_bound_to_the_current_lease_without_changing_ordinary_jobs() {
  let state_root = tempfile::tempdir().unwrap();
  let work_root = tempfile::tempdir().unwrap();
  let coordinator = Arc::new(LifecycleCoordinator::default());
  let (_cache, lifecycle, lease, snapshot) = lifecycle_fixture(
    coordinator.clone(),
    Arc::new(NoopOutputPublisher),
    state_root.path(),
    work_root.path(),
    false,
  );
  let expected = (
    "registration-1".to_owned(),
    lease.lease.job_id.clone(),
    lease.lease.attempt,
    lease.lease.fencing_token.clone(),
  );
  let provider = Arc::new(RecordingLeaseToolProvider {
    leases: StdMutex::new(Vec::new()),
  });
  let outcome = lifecycle
    .with_factory_tools(provider.clone())
    .run(lease, snapshot, CancellationToken::new())
    .await
    .unwrap();
  assert_eq!(outcome.status, JobCompletionStatus::Succeeded);
  assert_eq!(*provider.leases.lock().unwrap(), vec![expected]);
  assert!(fs::read_dir(work_root.path()).unwrap().next().is_none());
}
