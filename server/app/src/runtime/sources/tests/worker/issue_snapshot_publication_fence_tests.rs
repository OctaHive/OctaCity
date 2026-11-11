//! A late acknowledgement cannot publish replacement objects that were never written.
use super::*;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactByteReader, ArtifactObject, ArtifactStoreError};

struct BeforeWritePause(Notify);
#[async_trait::async_trait]
impl ArtifactBytePublisher for BeforeWritePause {
  async fn publish_verified(&self, _: &ArtifactObject, _: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self.0.notify_one();
    std::future::pending().await
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn issue_snapshot_rejects_late_publication_before_replacement_reservation() {
  late_publication(false).await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn issue_snapshot_rejects_late_publication_after_replacement_reservation() {
  late_publication(true).await;
}

async fn late_publication(after_reservation: bool) {
  let database = TestDatabase::migrated().await;
  let f = Fixture::enabled_with_flow("github", Some(snapshot_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let PreparedInput {
    issue,
    claim,
    preparation: original,
    manifest,
  } = prepared_input(&f).await;
  revise_issue(&f, issue).await;
  let documents = current_documents(&f, &original).await;
  f.store
    .refresh_source_issue_input(RefreshSourceIssueInput {
      expected_snapshot: original
        .issue_input
        .as_ref()
        .unwrap()
        .snapshot_digest()
        .unwrap()
        .unwrap(),
      security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
      replacement: FreezeSourcePreparationPolicy {
        claim: claim.clone(),
        preparation_id: original.id,
        policy: original.policy.clone().unwrap(),
        issue_input: Some(documents.clone()),
        now: time(110),
      },
    })
    .await
    .unwrap();
  f.clock.0.store(110, Ordering::SeqCst);
  let task = if after_reservation {
    let pause = Arc::new(BeforeWritePause(Notify::new()));
    let preparer = FactoryWorkSourcePreparer::new(f.store.clone(), pause.clone(), f.clock.clone());
    let policy = f.profile.preparation.clone();
    let current_claim = claim.clone();
    let current_documents = documents.clone();
    let task = tokio::spawn(async move {
      preparer
        .prepare_issue_input(current_claim, original.id, policy, &current_documents)
        .await
    });
    tokio::time::timeout(TEST_WAIT, pause.0.notified()).await.unwrap();
    Some(task)
  } else {
    None
  };
  let current = f
    .store
    .source_materialization(f.input.project_id(), original.id)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(
    current.mapping_digest, manifest.mapping_digest,
    "policy is unchanged while requirements differ"
  );
  assert_eq!(current.objects.is_empty(), !after_reservation);
  assert!(current.published_at.is_none());
  let result = f
    .store
    .publish_source_artifacts(PublishSourceArtifacts {
      claim: claim.clone(),
      preparation_id: original.id,
      mapping_digest: manifest.mapping_digest,
      objects: manifest.objects.clone(),
      now: time(111),
    })
    .await;
  if let Some(task) = task {
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
  }
  assert!(
    result.is_err(),
    "an old publication acknowledgement must not publish replacement bytes (reserved={after_reservation})"
  );
  assert_eq!(
    f.store
      .source_materialization(f.input.project_id(), original.id)
      .await
      .unwrap()
      .unwrap(),
    current
  );
  assert!(f.runs().await.is_empty());

  // A genuine replacement write still publishes the same logical preparation and admits once.
  f.clock.0.store(120, Ordering::SeqCst);
  FactoryWorkSourcePreparer::new(f.store.clone(), f.bytes.clone(), f.clock.clone())
    .prepare_issue_input(claim, original.id, f.profile.preparation.clone(), &documents)
    .await
    .unwrap();
  let published = f
    .store
    .source_materialization(f.input.project_id(), original.id)
    .await
    .unwrap()
    .unwrap();
  assert!(published.published_at.is_some());
  assert_eq!(published.expires_at, manifest.expires_at);
  let task = published
    .objects
    .iter()
    .find(|object| object.role == SourceArtifactRole::Task)
    .unwrap();
  let object = ArtifactObject::new(
    task.artifact.artifact_id(),
    task.upload_id,
    task.artifact.encoded_size(),
    task.artifact.content_digest().to_string(),
    task.media_type.as_str(),
  )
  .unwrap();
  let bytes = f
    .bytes
    .read_published(&object, task.artifact.encoded_size())
    .await
    .unwrap();
  assert!(std::str::from_utf8(&bytes).unwrap().contains("Revised canonical task"));
  f.clock.0.store(1001, Ordering::SeqCst);
  assert_eq!(
    worker(&f).run_once(CancellationToken::new()).await.unwrap().completed,
    1
  );
  assert_eq!(f.runs().await.len(), 1);
  assert_eq!(
    f.store
      .source_preparation(f.input.project_id(), original.id)
      .await
      .unwrap()
      .input,
    original.input
  );
  database.cleanup().await;
}
