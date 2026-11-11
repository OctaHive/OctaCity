//! Restart after a private byte write succeeds but its acknowledgement is lost.
use super::*;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject, ArtifactStoreError};
use std::sync::atomic::AtomicUsize;
use tokio::sync::Notify;

#[path = "issue_snapshot_refresh_tests.rs"]
mod refresh;

fn interrupted_worker<B: ArtifactBytePublisher + 'static>(
  fixture: &Fixture<PostgresStore>,
  bytes: Arc<B>,
  revisions: Arc<dyn RevisionResolver>,
) -> FactorySourceWorker<PostgresStore, B> {
  FactorySourceWorker::new(
    FactorySourceWorkerDependencies {
      store: fixture.store.clone(),
      bytes,
      project_inputs: None,
      revisions,
      authorization: Arc::new(TrustedNetworkManagementPolicy),
      registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      clock: fixture.clock.clone(),
    },
    FactorySourceInstallation {
      projects: vec![fixture.input.project_id()],
      profiles: vec![fixture.profile.clone()],
      settings: settings(),
    },
  )
  .unwrap()
  .with_issue_snapshots(Arc::new(IssueSourceSnapshotReader::new(
    fixture.store.clone(),
    Arc::new(TrustedNetworkManagementPolicy),
  )))
}

struct ByteAcknowledgementBarrier {
  bytes: Arc<Bytes>,
  calls: AtomicUsize,
  written: Notify,
}
#[async_trait::async_trait]
impl ArtifactBytePublisher for ByteAcknowledgementBarrier {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self.bytes.publish_verified(object, bytes).await?;
    if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
      self.written.notify_one();
      std::future::pending().await
    } else {
      Ok(())
    }
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn automatic_issue_snapshot_restart_after_lost_byte_ack_retains_preparation_and_admits_once() {
  let database = TestDatabase::migrated().await;
  let mut fixture = Fixture::enabled_with_flow("github", Some(snapshot_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let issue = linked_issue(&fixture, "TASK-native", true).await;
  let barrier = Arc::new(ByteAcknowledgementBarrier {
    bytes: fixture.bytes.clone(),
    calls: AtomicUsize::new(0),
    written: Notify::new(),
  });
  let interrupted = interrupted_worker(&fixture, barrier.clone(), Arc::new(ExactRevisionResolver));
  let task = tokio::spawn(async move { interrupted.run_once(CancellationToken::new()).await });
  tokio::time::timeout(TEST_WAIT, barrier.written.notified())
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
    .unwrap();
  assert_eq!(pending.len(), 1);
  let original = &pending[0];
  assert!(original.intent.is_none());
  assert!(original.admission.is_none());
  let documents = original.issue_input.clone().unwrap();
  assert_eq!(documents.issue_snapshot().unwrap().issue_id, issue.id);
  assert!(fixture.runs().await.is_empty());
  let manifest = fixture
    .store
    .source_materialization(fixture.input.project_id(), original.id)
    .await
    .unwrap()
    .unwrap();
  assert!(manifest.published_at.is_none());
  task.abort();
  assert!(task.await.unwrap_err().is_cancelled());

  fixture.clock.0.store(60_101, Ordering::SeqCst);
  fixture.store = Arc::new(PostgresStore::new(database.pool.clone()));
  assert_eq!(
    worker(&fixture)
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  let recovered = fixture
    .store
    .source_preparation(fixture.input.project_id(), original.id)
    .await
    .unwrap();
  assert_eq!(recovered.input, original.input);
  assert_eq!(recovered.issue_input.as_ref(), Some(&documents));
  let receipt = recovered.admission.clone().unwrap();
  let published = fixture
    .store
    .source_materialization(fixture.input.project_id(), original.id)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(
    published.objects, manifest.objects,
    "the same byte identities survive acknowledgement loss"
  );
  let run = fixture.store.factory_run_snapshot(receipt.run_id).await.unwrap();
  assert_eq!(run.work.id(), receipt.work_id);
  assert_eq!(
    run.flow.data.incoming[0].payload().value(),
    &serde_json::json!({
      "title":"Canonical TASK-native", "size":"small", "priority":"high"
    })
  );
  assert_eq!(
    run.flow.data.incoming[0].issue_snapshot().unwrap().digest,
    documents.issue_snapshot().unwrap().digest().unwrap()
  );
  let writes = fixture.bytes.0.lock().unwrap().clone();
  for at in [60_201, 60_301] {
    fixture.clock.0.store(at, Ordering::SeqCst);
    fixture.store = Arc::new(PostgresStore::new(database.pool.clone()));
    assert_eq!(
      worker(&fixture)
        .run_once(CancellationToken::new())
        .await
        .unwrap()
        .completed,
      1
    );
    assert_eq!(fixture.runs().await.len(), 1);
    assert_eq!(
      fixture
        .store
        .source_preparation(fixture.input.project_id(), original.id)
        .await
        .unwrap()
        .admission,
      Some(receipt.clone())
    );
  }
  assert_eq!(*fixture.bytes.0.lock().unwrap(), writes);
  database.cleanup().await;
}
