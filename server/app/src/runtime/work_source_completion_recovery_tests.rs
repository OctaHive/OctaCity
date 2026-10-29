//! Crash barriers at external bytes/VCS, retaining the real Source and admission owners.
use super::*;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject, ArtifactStoreError};
use std::sync::atomic::AtomicUsize;
use tokio::sync::Notify;

struct PublicationBarrier {
  bytes: Arc<Bytes>,
  call: usize,
  after_write: bool,
  calls: AtomicUsize,
  reached: Notify,
  resume: Notify,
}
impl PublicationBarrier {
  fn new(bytes: Arc<Bytes>, call: usize, after_write: bool) -> Self {
    Self {
      bytes,
      call,
      after_write,
      calls: AtomicUsize::new(0),
      reached: Notify::new(),
      resume: Notify::new(),
    }
  }
  async fn stop(&self) {
    self.reached.notify_one();
    self.resume.notified().await;
  }
}
#[async_trait::async_trait]
impl ArtifactBytePublisher for PublicationBarrier {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    let selected = self.calls.fetch_add(1, Ordering::SeqCst) + 1 == self.call;
    if selected && !self.after_write {
      self.stop().await;
    }
    self.bytes.publish_verified(object, bytes).await?;
    if selected && self.after_write {
      self.stop().await;
    }
    Ok(())
  }
}

struct RevisionBarrier {
  reached: Notify,
  resume: Notify,
}
struct NeverResolve;
#[async_trait::async_trait]
impl RevisionResolver for NeverResolve {
  async fn resolve(&self, _: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
    panic!("an admitted native item must recover without resolving a new revision")
  }
}
#[async_trait::async_trait]
impl RevisionResolver for RevisionBarrier {
  async fn resolve(&self, _: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
    self.reached.notify_one();
    self.resume.notified().await;
    Ok(ImmutableRevision::new("0123456789abcdef").unwrap())
  }
}

#[tokio::test]
async fn published_completion_recovers_before_admission_without_republishing_and_replays_once() {
  published_before_admission(Fixture::enabled("github").await).await;
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn published_completion_recovers_before_admission_with_postgres_parity() {
  let database = database::TestDatabase::migrated().await;
  published_before_admission(
    Fixture::enabled("github")
      .await
      .into_postgres(database.pool.clone())
      .await,
  )
  .await;
  database.cleanup().await;
}
async fn published_before_admission<S>(mut fixture: Fixture<S>)
where
  S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + ProjectTextInputStore + 'static,
{
  install_completion(&mut fixture).await;
  let barrier = Arc::new(RevisionBarrier {
    reached: Notify::new(),
    resume: Notify::new(),
  });
  let worker = Arc::new(completion_worker(&fixture, fixture.bytes.clone(), barrier.clone()));
  let task = tokio::spawn(async move { worker.run_once(CancellationToken::new()).await });
  tokio::time::timeout(TEST_WAIT, barrier.reached.notified())
    .await
    .unwrap();
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let pending = fixture
    .store
    .pending_source_preparations(fixture.input.project_id(), target, None, 2)
    .await
    .unwrap()
    .remove(0);
  let original_intent = pending.intent.clone().unwrap();
  let original = fixture
    .store
    .source_materialization(fixture.input.project_id(), pending.id)
    .await
    .unwrap()
    .unwrap();
  assert!(original.published_at.is_some());
  assert!(pending.admission.is_none());
  task.abort();
  assert!(task.await.unwrap_err().is_cancelled());
  let writes = fixture.bytes.0.lock().unwrap().len();
  fixture.clock.0.store(60_101, Ordering::SeqCst);
  fixture.profile.preparation.mapping.acceptance = SourceAcceptanceMapping::ProcessCompletion(
    SourceCompletionDocument::review_preset(fixture.input.project_id(), TRIAGE_COMPLETION_PRESET).unwrap(),
  );
  // Leave the original ticket visible: recovery must survive rediscovery/echo as well.
  let result = completion_worker(&fixture, fixture.bytes.clone(), Arc::new(ExactRevisionResolver))
    .run_once(CancellationToken::new())
    .await
    .unwrap();
  assert_eq!((result.completed, result.deferred), (1, 0));
  let accepted = fixture
    .store
    .source_preparation(fixture.input.project_id(), pending.id)
    .await
    .unwrap();
  assert_eq!(accepted.intent, Some(original_intent));
  let association = accepted.admission.unwrap();
  let work = fixture
    .store
    .factory_run_snapshot(association.run_id)
    .await
    .unwrap()
    .work;
  fixture.clock.0.store(60_201, Ordering::SeqCst);
  let outcome = completion_worker(&fixture, fixture.bytes.clone(), Arc::new(NeverResolve))
    .run_once(CancellationToken::new())
    .await
    .unwrap();
  assert_eq!(outcome.completed, 1);
  assert_eq!(fixture.runs().await.len(), 2);
  assert_eq!(fixture.bytes.0.lock().unwrap().len(), writes);
  assert_eq!(
    fixture
      .store
      .factory_run_snapshot(association.run_id)
      .await
      .unwrap()
      .work,
    work
  );
  assert_eq!(
    fixture
      .store
      .source_materialization(fixture.input.project_id(), pending.id)
      .await
      .unwrap()
      .unwrap(),
    original
  );
}

#[tokio::test]
async fn completion_publication_crashes_keep_the_original_criteria_and_admit_once() {
  for (call, after_write) in [(1, false), (1, true), (2, false), (2, true)] {
    let mut fixture = Fixture::enabled("github").await;
    install_completion(&mut fixture).await;
    let restart = fixture.store.clone();
    publication_crash(fixture, restart, call, after_write).await;
  }
}

async fn publication_crash<S>(mut fixture: Fixture<S>, restart: Arc<S>, call: usize, after_write: bool) -> Fixture<S>
where
  S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + ProjectTextInputStore + 'static,
{
  let original = serde_json::to_vec(&fixture.profile.preparation).unwrap();
  let barrier = Arc::new(PublicationBarrier::new(fixture.bytes.clone(), call, after_write));
  let worker = Arc::new(completion_worker(
    &fixture,
    barrier.clone(),
    Arc::new(ExactRevisionResolver),
  ));
  let running = tokio::spawn(async move { worker.run_once(CancellationToken::new()).await });
  tokio::time::timeout(TEST_WAIT, barrier.reached.notified())
    .await
    .unwrap();
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let retained = fixture
    .store
    .pending_source_preparations(fixture.input.project_id(), target, None, 2)
    .await
    .unwrap()
    .remove(0);
  assert_eq!(retained.policy.as_ref(), Some(&original));
  let manifest = fixture
    .store
    .source_materialization(fixture.input.project_id(), retained.id)
    .await
    .unwrap()
    .unwrap();
  assert!(manifest.published_at.is_none());
  assert!(retained.intent.is_none());
  assert_eq!(
    fixture.runs().await.len(),
    1,
    "only the reviewed document intake exists before recovery"
  );
  running.abort();
  assert!(running.await.unwrap_err().is_cancelled());
  // A new worker/store must recover without rediscovering the ticket or adopting an edited policy.
  fixture.store = restart;
  fixture.clock.0.store(60_101, Ordering::SeqCst);
  fixture.profile.preparation.mapping.acceptance = SourceAcceptanceMapping::ProcessCompletion(
    SourceCompletionDocument::review_preset(fixture.input.project_id(), TRIAGE_COMPLETION_PRESET).unwrap(),
  );
  fixture.profile.preparation.source =
    RepositorySourceSelection::ExactRevision(ImmutableRevision::new("replacement-base").unwrap());
  std::fs::write(fixture.root.path().join("github/mode.json"), br#"{"candidates":[]}"#).unwrap();
  let result = completion_worker(&fixture, fixture.bytes.clone(), Arc::new(ExactRevisionResolver))
    .run_once(CancellationToken::new())
    .await
    .unwrap();
  assert_eq!((result.completed, result.deferred), (1, 0));
  let recovered = fixture
    .store
    .source_preparation(fixture.input.project_id(), retained.id)
    .await
    .unwrap();
  assert_eq!(recovered.policy, Some(original));
  assert_eq!(recovered.snapshot, retained.snapshot);
  let admission = recovered.admission.clone().unwrap();
  let materialized = fixture
    .store
    .source_materialization(fixture.input.project_id(), retained.id)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(materialized.objects, manifest.objects);
  assert_eq!(materialized.expires_at, manifest.expires_at);
  assert_eq!(fixture.runs().await.len(), 2);
  assert_eq!(
    fixture
      .store
      .factory_run_snapshot(admission.run_id)
      .await
      .unwrap()
      .work
      .subject()
      .base_revision()
      .as_str(),
    "0123456789abcdef"
  );
  {
    let bytes = fixture.bytes.0.lock().unwrap();
    let document = materialized
      .objects
      .iter()
      .find(|o| o.role == FactoryInputRole::Acceptance)
      .unwrap();
    let content = bytes
      .iter()
      .find(|b| FactoryDigest::content_sha256(b) == document.artifact.content_digest())
      .unwrap();
    let criteria: serde_json::Value = serde_json::from_slice(content).unwrap();
    assert_eq!(criteria["criteria"], CRITERIA);
    assert_eq!(criteria["purpose"], "process_completion");
  }
  fixture.clock.0.store(60_201, Ordering::SeqCst);
  assert_eq!(
    completion_worker(&fixture, fixture.bytes.clone(), Arc::new(ExactRevisionResolver))
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  assert_eq!(
    fixture
      .store
      .source_preparation(fixture.input.project_id(), retained.id)
      .await
      .unwrap()
      .admission,
    Some(admission)
  );
  assert_eq!(fixture.runs().await.len(), 2);
  fixture
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn completion_publication_crashes_preserve_canonical_issue_acceptance_in_postgres() {
  for (call, after_write) in [(1, false), (1, true), (2, false), (2, true)] {
    let database = database::TestDatabase::migrated().await;
    let mut fixture = Fixture::enabled("github")
      .await
      .into_postgres(database.pool.clone())
      .await;
    install_completion(&mut fixture).await;
    let issue = import_title_only_issue(&fixture).await;
    let restart = Arc::new(octacity_server_store_postgres::PostgresStore::new(
      database.pool.clone(),
    ));
    let fixture = publication_crash(fixture, restart, call, after_write).await;
    let actual = fixture.store.issue(issue.project_id, issue.id, None).await.unwrap();
    assert_eq!(
      actual, issue,
      "process completion must not become canonical Issue requirements"
    );
    assert!(actual.content.acceptance.is_none());
    database.cleanup().await;
  }
}

async fn import_title_only_issue(fixture: &Fixture<octacity_server_store_postgres::PostgresStore>) -> IssueRevision {
  let project = fixture.input.project_id();
  let reader = Arc::new(
    HostedWorkSourceObservations::new(
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      vec![fixture.profile.read.clone()],
      fixture.clock.clone(),
      settings().operation_timeout,
      settings().cancellation_grace,
      CancellationToken::new(),
    )
    .unwrap(),
  );
  let selection = IssueSourceSelection {
    connection: None,
    project_id: project,
    source: fixture.profile.read.binding.clone(),
    item: WorkSourceItem {
      resource: ExternalWorkIdentity::new("native-container").unwrap(),
      item: ExternalWorkIdentity::new("TASK-native").unwrap(),
    },
  };
  let handler = IssueSourceHandlers::new(
    fixture.store.clone(),
    reader,
    Arc::new(TrustedNetworkManagementPolicy),
    fixture.clock.clone(),
  )
  .with_source_mappings(vec![InstalledIssueSourceMapping {
    project_id: project,
    source: selection.source.clone(),
    native_resource: selection.item.resource.clone(),
    readable: WorkSourceReportingPolicy {
      ownership: None,
      states: vec!["native-state".into()],
      labels: vec![],
    },
    writer: None,
  }])
  .unwrap();
  let outcome = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handler))
    .handle_command(
      &fixture.profile.read.context,
      ImportIssueSourceCommand {
        selection,
        read: ReadWorkSourceObservations {
          connection: None,
          project_id: project,
          source: fixture.profile.read.binding.clone(),
          scan_id: digest(66),
          window: None,
          continuation: None,
          limit: 1,
        },
        idempotency_key: "import-title-only".parse().unwrap(),
        imported_at: time(100),
      },
    )
    .await
    .unwrap();
  let IssueSourceOutcome::Linked { link, .. } = outcome else {
    panic!("the verified ticket must import")
  };
  let issue = fixture.store.issue(project, link.issue_id, None).await.unwrap();
  assert!(issue.content.acceptance.is_none());
  assert!(issue.content.description.is_empty());
  issue
}

#[tokio::test]
async fn completion_takeover_and_retention_races_fence_late_publication() {
  for expire in [false, true] {
    late_publication(Fixture::enabled("github").await, expire).await;
  }
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn completion_takeover_and_retention_races_have_postgres_parity() {
  for expire in [false, true] {
    let database = database::TestDatabase::migrated().await;
    late_publication(
      Fixture::enabled("github")
        .await
        .into_postgres(database.pool.clone())
        .await,
      expire,
    )
    .await;
    database.cleanup().await;
  }
}
async fn late_publication<S>(mut fixture: Fixture<S>, expire: bool)
where
  S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + ProjectTextInputStore + 'static,
{
  install_completion(&mut fixture).await;
  let barrier = Arc::new(PublicationBarrier::new(fixture.bytes.clone(), 1, false));
  let worker = Arc::new(completion_worker(
    &fixture,
    barrier.clone(),
    Arc::new(ExactRevisionResolver),
  ));
  let old = tokio::spawn(async move { worker.run_once(CancellationToken::new()).await });
  tokio::time::timeout(TEST_WAIT, barrier.reached.notified())
    .await
    .unwrap();
  assert_eq!(
    completion_worker(&fixture, fixture.bytes.clone(), Arc::new(NeverResolve))
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .claimed,
    0,
    "a concurrent worker cannot acquire another live claim for the same input"
  );
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let original = fixture
    .store
    .pending_source_preparations(fixture.input.project_id(), target, None, 2)
    .await
    .unwrap()
    .remove(0);
  let manifest = fixture
    .store
    .source_materialization(fixture.input.project_id(), original.id)
    .await
    .unwrap()
    .unwrap();
  let cleaner = FactorySourceInputRetentionWorker::new(
    fixture.store.clone(),
    Arc::new(TransferBytes(fixture.bytes.clone())),
    Duration::from_millis(1),
  )
  .unwrap();
  if expire {
    fixture
      .clock
      .0
      .store(manifest.expires_at.unix_millis(), Ordering::SeqCst);
    assert_eq!(
      cleaner.run_once(manifest.expires_at, None, 2).await.unwrap().completed,
      1
    );
  } else {
    fixture.clock.0.store(60_101, Ordering::SeqCst);
    assert_eq!(
      completion_worker(&fixture, fixture.bytes.clone(), Arc::new(ExactRevisionResolver))
        .run_once(CancellationToken::new())
        .await
        .unwrap()
        .completed,
      1
    );
  }
  barrier.resume.notify_one();
  assert_eq!(
    tokio::time::timeout(TEST_WAIT, old)
      .await
      .unwrap()
      .unwrap()
      .unwrap()
      .deferred,
    1
  );
  let recovered = fixture
    .store
    .source_preparation(fixture.input.project_id(), original.id)
    .await
    .unwrap();
  let materialized = fixture
    .store
    .source_materialization(fixture.input.project_id(), original.id)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(materialized.objects, manifest.objects);
  assert_eq!(materialized.expires_at, manifest.expires_at);
  assert_eq!(fixture.runs().await.len(), if expire { 1 } else { 2 });
  if expire {
    assert!(recovered.snapshot.is_empty());
    assert!(recovered.policy.is_none());
    assert!(recovered.admission.is_none());
    assert!(materialized.published_at.is_none());
    assert_eq!(
      cleaner
        .run_once(time(manifest.expires_at.unix_millis() + 10), None, 2)
        .await
        .unwrap()
        .completed,
      1
    );
    assert_eq!(
      fixture.bytes.0.lock().unwrap().len(),
      2,
      "late Source writes are cleared; original Project documents remain separate"
    );
    fixture
      .clock
      .0
      .store(manifest.expires_at.unix_millis() + 20, Ordering::SeqCst);
    assert_eq!(
      completion_worker(&fixture, fixture.bytes.clone(), Arc::new(NeverResolve))
        .run_once(CancellationToken::new())
        .await
        .unwrap()
        .completed,
      1
    );
    assert_eq!(
      fixture.runs().await.len(),
      1,
      "expiry cannot resurrect this original input identity"
    );
  } else {
    assert_eq!(recovered.snapshot, original.snapshot);
    assert_eq!(recovered.policy, original.policy);
    assert!(recovered.admission.is_some());
  }
}
