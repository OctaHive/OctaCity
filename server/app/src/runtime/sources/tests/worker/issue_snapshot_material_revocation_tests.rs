//! Copied text does not inherit a body-read grant for an unadmitted Source preparation.
use super::*;
use tokio::sync::Notify;

#[derive(Default)]
struct RevisionGate {
  resolving: Notify,
  resume: Notify,
}
#[async_trait::async_trait]
impl RevisionResolver for RevisionGate {
  async fn resolve(&self, request: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
    self.resolving.notify_one();
    self.resume.notified().await;
    ExactRevisionResolver.resolve(request).await
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn source_material_access_revocation_after_preparation_blocks_admission_until_restored() {
  let database = TestDatabase::migrated().await;
  let f = Fixture::enabled_with_flow("github", Some(material_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let issue = linked_issue(&f, "TASK-native", true).await;
  let approved = comment(&f, &issue, "PRIVATE prepared text", "revoked-comment").await;
  select(
    &f,
    &issue,
    vec![IssueMaterialReference::Comment {
      project_id: issue.project_id,
      issue_id: issue.id,
      comment_id: approved.id,
      body_digest: approved.body_digest,
    }],
    "approve-before-revocation",
  )
  .await;
  let access = Arc::new(MaterialAccess::default());
  let revisions = Arc::new(RevisionGate::default());
  let reader = crate::runtime::sources::source_issue_snapshots::reader(
    f.store.clone(),
    f.bytes.clone(),
    revisions.clone(),
    access.clone(),
    f.clock.clone(),
  );
  let paused = FactorySourceWorker::new(
    FactorySourceWorkerDependencies {
      store: f.store.clone(),
      bytes: f.bytes.clone(),
      project_inputs: None,
      revisions: revisions.clone(),
      authorization: Arc::new(TrustedNetworkManagementPolicy),
      registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(f.root.path()).unwrap()),
      clock: f.clock.clone(),
    },
    FactorySourceInstallation {
      projects: vec![f.input.project_id()],
      profiles: vec![f.profile.clone()],
      settings: settings(),
    },
  )
  .unwrap()
  .with_issue_snapshots(reader);
  let task = tokio::spawn(async move { paused.run_once(CancellationToken::new()).await });
  tokio::time::timeout(TEST_WAIT, revisions.resolving.notified())
    .await
    .unwrap();
  let prepared = pending(&f).await;
  assert_eq!(
    prepared.issue_input.as_ref().unwrap().issue_snapshot().unwrap().values[&key("materials")],
    serde_json::json!(["PRIVATE prepared text"])
  );
  assert!(prepared.intent.is_some());
  assert!(f.runs().await.is_empty());
  access.denied.store(true, Ordering::SeqCst);
  revisions.resume.notify_one();
  let result = tokio::time::timeout(TEST_WAIT, task).await.unwrap().unwrap().unwrap();
  assert_eq!((result.completed, result.deferred), (1, 0));
  let waiting = f.store.source_preparation(issue.project_id, prepared.id).await.unwrap();
  assert_eq!(waiting.waiting.unwrap().reason, WorkSourceWaitReason::Inaccessible);
  assert!(waiting.admission.is_none());
  assert!(
    f.runs().await.is_empty(),
    "previously copied text does not bypass current content permission"
  );

  access.denied.store(false, Ordering::SeqCst);
  f.clock.0.store(110, Ordering::SeqCst);
  assert_eq!(
    worker_with_policy(&f, access)
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  let runs = f.runs().await;
  assert_eq!(runs.len(), 1);
  let admitted = f.store.source_preparation(issue.project_id, prepared.id).await.unwrap();
  assert_eq!(admitted.admission.unwrap().run_id, runs[0].id);
  database.cleanup().await;
}
