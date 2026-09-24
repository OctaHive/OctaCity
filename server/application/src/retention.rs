use std::{
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};

use octacity_artifact_store::{ArtifactObject, ArtifactStore, ArtifactStoreError, LogChunkStore, LogChunkStoreError};
use octacity_server_domain::Timestamp;
use octacity_server_store::{
  BuildRetentionStore, ClaimOrphanLogChunks, ClaimRetentionWork, CompleteOrphanLogChunk, CompleteRetentionObject,
  CompleteRetentionSearch, FailOrphanLogChunk, FailRetentionWork, FinishRetentionPass, LogChunkManifestStore,
  LogSearchError, LogSearchIndex, OrphanLogChunkStore, PrepareRetentionWork, RetentionObject, RetentionPassOutcome,
  RetentionWorkClaim, StoreError, WorkerOwner,
};
use thiserror::Error;

use crate::DurableRetryPolicy;

/// Counts produced by one bounded Build Result retention pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildRetentionBatchOutcome {
  /// Durable component operations acquired by this replica.
  pub claimed: usize,
  /// Components whose logical and physical cleanup completed.
  pub completed: usize,
  /// Components released with another bounded object page remaining.
  pub pending: usize,
  /// Transient failures scheduled for another attempt.
  pub retries_scheduled: usize,
  /// Failures whose bounded retry budget was exhausted.
  pub dead_lettered: usize,
  /// Staged orphan-log candidates examined in this pass.
  pub orphan_claimed: usize,
  /// Unreferenced staged log objects removed or committed objects retained.
  pub orphan_completed: usize,
}

/// Restart-safe coordinator for Build Result visibility, search, and object cleanup.
pub struct BuildRetentionWorker<S, I> {
  store: Arc<S>,
  index: Arc<I>,
  artifacts: Arc<dyn ArtifactStore>,
  logs: Arc<dyn LogChunkStore>,
  retry_policy: DurableRetryPolicy,
  clock: Arc<dyn RetentionClock>,
}

impl<S, I> BuildRetentionWorker<S, I>
where
  S: BuildRetentionStore + LogChunkManifestStore + OrphanLogChunkStore,
  I: LogSearchIndex,
{
  /// Creates a worker from authoritative work, derived search, and byte-store ports.
  pub fn new(
    store: Arc<S>,
    index: Arc<I>,
    artifacts: Arc<dyn ArtifactStore>,
    logs: Arc<dyn LogChunkStore>,
    retry_policy: DurableRetryPolicy,
  ) -> Self {
    Self {
      store,
      index,
      artifacts,
      logs,
      retry_policy,
      clock: Arc::new(SystemRetentionClock),
    }
  }

  #[cfg(test)]
  fn with_clock(
    store: Arc<S>,
    index: Arc<I>,
    artifacts: Arc<dyn ArtifactStore>,
    logs: Arc<dyn LogChunkStore>,
    retry_policy: DurableRetryPolicy,
    clock: Arc<dyn RetentionClock>,
  ) -> Self {
    Self {
      store,
      index,
      artifacts,
      logs,
      retry_policy,
      clock,
    }
  }

  /// Claims and advances one bounded batch without retaining process-local correctness state.
  pub async fn run_once(
    &self,
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    work_limit: u16,
    object_limit: u16,
  ) -> Result<BuildRetentionBatchOutcome, BuildRetentionWorkerError> {
    let mut outcome = self
      .cleanup_orphans(owner.clone(), observed_at, claim_expires_at, object_limit)
      .await?;
    let claims = self
      .store
      .claim_retention_work(ClaimRetentionWork::new(
        owner,
        observed_at,
        claim_expires_at,
        work_limit,
      )?)
      .await?;
    outcome.claimed = claims.len();
    for claim in claims {
      match self.apply(&claim, object_limit).await {
        Ok(pass) => match pass {
          RetentionPassOutcome::Completed => outcome.completed += 1,
          RetentionPassOutcome::Pending => outcome.pending += 1,
        },
        Err(BuildRetentionWorkerError::Store(error)) => return Err(error.into()),
        Err(error) => {
          let failed_at = self.clock.now()?;
          let retry_at = error
            .is_retryable()
            .then(|| self.retry_policy.retry_at(claim.attempt, failed_at))
            .flatten();
          self
            .store
            .fail_retention_work(FailRetentionWork {
              work_id: claim.work_id,
              owner: claim.owner.clone(),
              failed_at,
              error_code: error.code().to_owned(),
              retry_at,
            })
            .await?;
          if retry_at.is_some() {
            outcome.retries_scheduled += 1;
          } else {
            outcome.dead_lettered += 1;
          }
        }
      }
    }
    Ok(outcome)
  }

  async fn cleanup_orphans(
    &self,
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<BuildRetentionBatchOutcome, BuildRetentionWorkerError> {
    let claims = self
      .store
      .claim_orphan_log_chunks(ClaimOrphanLogChunks::new(owner, observed_at, claim_expires_at, limit)?)
      .await?;
    let mut outcome = BuildRetentionBatchOutcome {
      orphan_claimed: claims.len(),
      ..BuildRetentionBatchOutcome::default()
    };
    for claim in claims {
      let result = async {
        if !self.store.log_chunk_is_committed(claim.manifest.chunk_id()).await? {
          self.logs.delete_chunk(&claim.manifest).await?;
        }
        Ok::<(), BuildRetentionWorkerError>(())
      }
      .await;
      let settled_at = self.clock.now()?;
      match result {
        Ok(()) => {
          self
            .store
            .complete_orphan_log_chunk(CompleteOrphanLogChunk {
              chunk_id: claim.manifest.chunk_id(),
              owner: claim.owner,
              completed_at: settled_at,
            })
            .await?;
          outcome.orphan_completed += 1;
        }
        Err(BuildRetentionWorkerError::Store(error)) => return Err(error.into()),
        Err(error) => {
          let retry_at = error
            .is_retryable()
            .then(|| self.retry_policy.retry_at(claim.attempt, settled_at))
            .flatten();
          self
            .store
            .fail_orphan_log_chunk(FailOrphanLogChunk {
              chunk_id: claim.manifest.chunk_id(),
              owner: claim.owner,
              failed_at: settled_at,
              error_code: error.code().to_owned(),
              retry_at,
            })
            .await?;
          if retry_at.is_some() {
            outcome.retries_scheduled += 1;
          } else {
            outcome.dead_lettered += 1;
          }
        }
      }
    }
    Ok(outcome)
  }

  async fn apply(
    &self,
    claim: &RetentionWorkClaim,
    object_limit: u16,
  ) -> Result<RetentionPassOutcome, BuildRetentionWorkerError> {
    let prepared_at = self.clock.now()?;
    let preparation = self
      .store
      .prepare_retention_work(PrepareRetentionWork::new(
        claim.work_id,
        claim.owner.clone(),
        prepared_at,
        object_limit,
      )?)
      .await?;
    if let Some(deletion) = preparation.search_deletion {
      self.index.delete(deletion).await?;
      self
        .store
        .complete_retention_search(CompleteRetentionSearch {
          work_id: claim.work_id,
          owner: claim.owner.clone(),
          completed_at: self.clock.now()?,
        })
        .await?;
    }
    for object in preparation.objects {
      self.delete_object(&object).await?;
      self
        .store
        .complete_retention_object(CompleteRetentionObject {
          work_id: claim.work_id,
          owner: claim.owner.clone(),
          object: object.identity(),
          completed_at: self.clock.now()?,
        })
        .await?;
    }
    self
      .store
      .finish_retention_pass(FinishRetentionPass {
        work_id: claim.work_id,
        owner: claim.owner.clone(),
        finished_at: self.clock.now()?,
      })
      .await
      .map_err(Into::into)
  }

  async fn delete_object(&self, object: &RetentionObject) -> Result<(), BuildRetentionWorkerError> {
    match object {
      RetentionObject::Log(manifest) => self.logs.delete_chunk(manifest).await.map_err(Into::into),
      RetentionObject::Output(upload) => {
        let identity = upload.artifact.identity();
        let object = ArtifactObject::new(
          identity.artifact_id,
          upload.upload_id,
          identity.size_bytes,
          identity.digest.to_string(),
          upload.transport_media_type.as_str(),
        )
        .map_err(|_| BuildRetentionWorkerError::InvalidArtifactObject)?;
        self.artifacts.delete(&object).await.map_err(Into::into)
      }
    }
  }
}

trait RetentionClock: Send + Sync {
  fn now(&self) -> Result<Timestamp, BuildRetentionWorkerError>;
}

struct SystemRetentionClock;

impl RetentionClock for SystemRetentionClock {
  fn now(&self) -> Result<Timestamp, BuildRetentionWorkerError> {
    let milliseconds = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .ok()
      .and_then(|duration| i64::try_from(duration.as_millis()).ok())
      .ok_or(BuildRetentionWorkerError::ClockUnavailable)?;
    Timestamp::from_unix_millis(milliseconds).map_err(|_| BuildRetentionWorkerError::ClockUnavailable)
  }
}

/// Safe failure from one bounded Build Result retention pass.
#[derive(Debug, Error)]
pub enum BuildRetentionWorkerError {
  /// Authoritative retention work could not be read or settled.
  #[error("Build Result retention store failed")]
  Store(#[from] StoreError),
  /// Derived search deletion could not be applied.
  #[error("Build-log search retention failed")]
  Search(#[from] LogSearchError),
  /// Artifact or report bytes could not be removed.
  #[error("output-byte retention failed")]
  ArtifactBytes(#[from] ArtifactStoreError),
  /// Archived log bytes could not be removed.
  #[error("log-byte retention failed")]
  LogBytes(#[from] LogChunkStoreError),
  /// Persisted output metadata could not reconstruct its backend-neutral object.
  #[error("retained output identity is invalid")]
  InvalidArtifactObject,
  /// The process clock cannot be represented by the domain timestamp.
  #[error("retention worker clock is unavailable")]
  ClockUnavailable,
}

impl BuildRetentionWorkerError {
  const fn code(&self) -> &'static str {
    match self {
      Self::Store(_) => "authoritative_store",
      Self::Search(_) => "search_index",
      Self::ArtifactBytes(_) => "artifact_store",
      Self::LogBytes(_) => "log_store",
      Self::InvalidArtifactObject => "invalid_artifact_object",
      Self::ClockUnavailable => "clock_unavailable",
    }
  }

  const fn is_retryable(&self) -> bool {
    !matches!(self, Self::InvalidArtifactObject)
  }
}

#[cfg(test)]
mod tests {
  use std::{
    future::Future,
    sync::Mutex,
    task::{Context, Poll, Waker},
    time::Duration,
  };

  use async_trait::async_trait;
  use octacity_artifact_store::{DownloadAuthorization, LogChunkWrite, UploadAuthorization};
  use octacity_server_domain::{BuildId, JobId, LogIndexingWorkId, ProjectId, RetentionWorkId};
  use octacity_server_store::{
    BuildLogStream, BuildResultComponent, DeleteLogSearchDocuments, IndexedLogSearchPage, LogChunkManifest,
    LogIndexPosition, LogSearchMutationDisposition, LogSearchQuery, RetentionObjectIdentity, RetentionPhase,
    RetentionPreparation, WriteLogSearchDocument,
  };

  use super::*;

  #[test]
  fn logical_object_is_deleted_before_its_metadata_is_completed() {
    run_ready(async {
      let owner = WorkerOwner::new("retention:test").unwrap();
      let manifest = manifest();
      let store = Arc::new(FakeRetentionStore::new(
        claim(owner.clone(), BuildResultComponent::Logs),
        RetentionPreparation {
          search_deletion: None,
          objects: vec![RetentionObject::Log(manifest.clone())],
        },
      ));
      let bytes = Arc::new(FakeBytes::default());
      let worker = worker(store.clone(), bytes.clone());

      let outcome = worker.run_once(owner, time(10), time(30), 1, 1).await.unwrap();

      assert_eq!(outcome.completed, 1);
      assert_eq!(bytes.deleted_logs.lock().unwrap().as_slice(), &[manifest.chunk_id()]);
      assert_eq!(
        store.completed_objects.lock().unwrap().as_slice(),
        &[RetentionObjectIdentity::Log(manifest.chunk_id())]
      );
    });
  }

  #[test]
  fn interrupted_byte_deletion_is_released_for_a_durable_retry() {
    run_ready(async {
      let owner = WorkerOwner::new("retention:test").unwrap();
      let manifest = manifest();
      let store = Arc::new(FakeRetentionStore::new(
        claim(owner.clone(), BuildResultComponent::Logs),
        RetentionPreparation {
          search_deletion: None,
          objects: vec![RetentionObject::Log(manifest)],
        },
      ));
      let bytes = Arc::new(FakeBytes {
        fail_log_delete: true,
        ..FakeBytes::default()
      });
      let worker = worker(store.clone(), bytes);

      let outcome = worker.run_once(owner, time(10), time(30), 1, 1).await.unwrap();

      assert_eq!(outcome.retries_scheduled, 1);
      assert!(store.completed_objects.lock().unwrap().is_empty());
      let failures = store.failures.lock().unwrap();
      assert_eq!(failures.len(), 1);
      assert_eq!(failures[0].failed_at, time(20));
      assert_eq!(failures[0].error_code, "log_store");
      assert_eq!(failures[0].retry_at, Some(time(30)));
    });
  }

  #[test]
  fn exhausted_byte_deletion_is_dead_lettered_without_another_claim() {
    run_ready(async {
      let owner = WorkerOwner::new("retention:test").unwrap();
      let store = Arc::new(FakeRetentionStore::new(
        claim_with_attempt(owner.clone(), BuildResultComponent::Logs, 3),
        RetentionPreparation {
          search_deletion: None,
          objects: vec![RetentionObject::Log(manifest())],
        },
      ));
      let bytes = Arc::new(FakeBytes {
        fail_log_delete: true,
        ..FakeBytes::default()
      });

      let outcome = worker(store.clone(), bytes)
        .run_once(owner, time(10), time(30), 1, 1)
        .await
        .unwrap();

      assert_eq!(outcome.dead_lettered, 1);
      assert_eq!(outcome.retries_scheduled, 0);
      assert_eq!(store.failures.lock().unwrap()[0].retry_at, None);
    });
  }

  #[test]
  fn log_tombstone_is_durable_before_chunk_bytes_are_deleted() {
    run_ready(async {
      let owner = WorkerOwner::new("retention:test").unwrap();
      let manifest = manifest();
      let deletion = DeleteLogSearchDocuments {
        work_id: LogIndexingWorkId::from_uuid(uuid(5)).unwrap(),
        position: LogIndexPosition::new(1).unwrap(),
        project_id: ProjectId::from_uuid(uuid(2)).unwrap(),
        build_id: BuildId::from_uuid(uuid(3)).unwrap(),
      };
      let events = Arc::new(Mutex::new(Vec::new()));
      let store = Arc::new(FakeRetentionStore::with_events(
        claim(owner.clone(), BuildResultComponent::Logs),
        RetentionPreparation {
          search_deletion: Some(deletion),
          objects: vec![RetentionObject::Log(manifest)],
        },
        events.clone(),
      ));
      let bytes = Arc::new(FakeBytes {
        events: Some(events.clone()),
        ..FakeBytes::default()
      });
      let worker = BuildRetentionWorker::with_clock(
        store,
        Arc::new(FakeIndex(events.clone())),
        bytes.clone(),
        bytes,
        DurableRetryPolicy::new(3, 10, 100).unwrap(),
        Arc::new(FixedClock(time(20))),
      );

      worker.run_once(owner, time(10), time(30), 1, 1).await.unwrap();

      assert_eq!(
        *events.lock().unwrap(),
        [
          "search-delete",
          "search-complete",
          "bytes-delete",
          "object-complete",
          "finish"
        ]
      );
    });
  }

  fn worker(
    store: Arc<FakeRetentionStore>,
    bytes: Arc<FakeBytes>,
  ) -> BuildRetentionWorker<FakeRetentionStore, FakeIndex> {
    BuildRetentionWorker::with_clock(
      store,
      Arc::new(FakeIndex::default()),
      bytes.clone(),
      bytes,
      DurableRetryPolicy::new(3, 10, 100).unwrap(),
      Arc::new(FixedClock(time(20))),
    )
  }

  struct FixedClock(Timestamp);

  impl RetentionClock for FixedClock {
    fn now(&self) -> Result<Timestamp, BuildRetentionWorkerError> {
      Ok(self.0)
    }
  }

  struct FakeRetentionStore {
    claims: Mutex<Vec<RetentionWorkClaim>>,
    preparation: RetentionPreparation,
    completed_objects: Mutex<Vec<RetentionObjectIdentity>>,
    failures: Mutex<Vec<FailRetentionWork>>,
    events: Option<Arc<Mutex<Vec<&'static str>>>>,
  }

  impl FakeRetentionStore {
    fn new(claim: RetentionWorkClaim, preparation: RetentionPreparation) -> Self {
      Self {
        claims: Mutex::new(vec![claim]),
        preparation,
        completed_objects: Mutex::new(Vec::new()),
        failures: Mutex::new(Vec::new()),
        events: None,
      }
    }

    fn with_events(
      claim: RetentionWorkClaim,
      preparation: RetentionPreparation,
      events: Arc<Mutex<Vec<&'static str>>>,
    ) -> Self {
      Self {
        events: Some(events),
        ..Self::new(claim, preparation)
      }
    }

    fn event(&self, event: &'static str) {
      if let Some(events) = &self.events {
        events.lock().unwrap().push(event);
      }
    }
  }

  #[async_trait]
  impl BuildRetentionStore for FakeRetentionStore {
    async fn claim_retention_work(&self, _request: ClaimRetentionWork) -> Result<Vec<RetentionWorkClaim>, StoreError> {
      Ok(std::mem::take(&mut *self.claims.lock().unwrap()))
    }

    async fn prepare_retention_work(&self, _request: PrepareRetentionWork) -> Result<RetentionPreparation, StoreError> {
      Ok(self.preparation.clone())
    }

    async fn complete_retention_search(&self, _request: CompleteRetentionSearch) -> Result<(), StoreError> {
      self.event("search-complete");
      Ok(())
    }

    async fn complete_retention_object(&self, request: CompleteRetentionObject) -> Result<(), StoreError> {
      self.event("object-complete");
      self.completed_objects.lock().unwrap().push(request.object);
      Ok(())
    }

    async fn finish_retention_pass(&self, _request: FinishRetentionPass) -> Result<RetentionPassOutcome, StoreError> {
      self.event("finish");
      Ok(RetentionPassOutcome::Completed)
    }

    async fn fail_retention_work(&self, request: FailRetentionWork) -> Result<(), StoreError> {
      self.failures.lock().unwrap().push(request);
      Ok(())
    }
  }

  #[async_trait]
  impl LogChunkManifestStore for FakeRetentionStore {
    async fn log_chunk_is_committed(&self, _chunk_id: octacity_server_domain::LogChunkId) -> Result<bool, StoreError> {
      Ok(false)
    }
  }

  #[async_trait]
  impl OrphanLogChunkStore for FakeRetentionStore {
    async fn stage_orphan_log_chunk(
      &self,
      _request: octacity_server_store::StageOrphanLogChunk,
    ) -> Result<(), StoreError> {
      unreachable!()
    }

    async fn claim_orphan_log_chunks(
      &self,
      _request: ClaimOrphanLogChunks,
    ) -> Result<Vec<octacity_server_store::OrphanLogChunkClaim>, StoreError> {
      Ok(Vec::new())
    }

    async fn complete_orphan_log_chunk(&self, _request: CompleteOrphanLogChunk) -> Result<(), StoreError> {
      unreachable!()
    }

    async fn fail_orphan_log_chunk(&self, _request: FailOrphanLogChunk) -> Result<(), StoreError> {
      unreachable!()
    }
  }

  #[derive(Default)]
  struct FakeIndex(Arc<Mutex<Vec<&'static str>>>);

  #[async_trait]
  impl LogSearchIndex for FakeIndex {
    async fn search(&self, _query: LogSearchQuery) -> Result<IndexedLogSearchPage, LogSearchError> {
      unreachable!()
    }

    async fn index(&self, _request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError> {
      unreachable!()
    }

    async fn indexed_through(&self, _project_id: ProjectId) -> Result<Option<LogIndexPosition>, LogSearchError> {
      unreachable!()
    }

    async fn delete(&self, _request: DeleteLogSearchDocuments) -> Result<LogSearchMutationDisposition, LogSearchError> {
      self.0.lock().unwrap().push("search-delete");
      Ok(LogSearchMutationDisposition::Applied)
    }

    async fn rebuild(&self, _request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError> {
      unreachable!()
    }
  }

  #[derive(Default)]
  struct FakeBytes {
    deleted_logs: Mutex<Vec<octacity_server_domain::LogChunkId>>,
    fail_log_delete: bool,
    events: Option<Arc<Mutex<Vec<&'static str>>>>,
  }

  #[async_trait]
  impl ArtifactStore for FakeBytes {
    async fn authorize_upload(
      &self,
      _object: &ArtifactObject,
      _expires_in: Duration,
    ) -> Result<UploadAuthorization, ArtifactStoreError> {
      unreachable!()
    }

    async fn complete_upload(&self, _object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
      unreachable!()
    }

    async fn authorize_download(
      &self,
      _object: &ArtifactObject,
      _expires_in: Duration,
    ) -> Result<DownloadAuthorization, ArtifactStoreError> {
      unreachable!()
    }

    async fn delete(&self, _object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
      unreachable!()
    }
  }

  #[async_trait]
  impl LogChunkStore for FakeBytes {
    async fn put_verified(
      &self,
      _manifest: &LogChunkManifest,
      _bytes: Vec<u8>,
    ) -> Result<LogChunkWrite, LogChunkStoreError> {
      unreachable!()
    }

    async fn read_verified(&self, _manifest: &LogChunkManifest) -> Result<Vec<u8>, LogChunkStoreError> {
      unreachable!()
    }

    async fn delete_chunk(&self, manifest: &LogChunkManifest) -> Result<(), LogChunkStoreError> {
      if self.fail_log_delete {
        return Err(LogChunkStoreError::Unavailable);
      }
      if let Some(events) = &self.events {
        events.lock().unwrap().push("bytes-delete");
      }
      self.deleted_logs.lock().unwrap().push(manifest.chunk_id());
      Ok(())
    }
  }

  fn claim(owner: WorkerOwner, component: BuildResultComponent) -> RetentionWorkClaim {
    claim_with_attempt(owner, component, 1)
  }

  fn claim_with_attempt(owner: WorkerOwner, component: BuildResultComponent, attempt: u16) -> RetentionWorkClaim {
    RetentionWorkClaim {
      work_id: RetentionWorkId::from_uuid(uuid(1)).unwrap(),
      project_id: ProjectId::from_uuid(uuid(2)).unwrap(),
      build_id: BuildId::from_uuid(uuid(3)).unwrap(),
      component,
      deadline: time(10),
      phase: RetentionPhase::Pending,
      attempt,
      owner,
      claim_expires_at: time(30),
    }
  }

  fn manifest() -> LogChunkManifest {
    LogChunkManifest::prepare(
      JobId::from_uuid(uuid(4)).unwrap(),
      BuildLogStream::Stdout,
      1,
      1,
      b"redacted",
    )
    .unwrap()
  }

  const fn uuid(value: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(value)
  }

  fn time(milliseconds: i64) -> Timestamp {
    Timestamp::from_unix_millis(milliseconds).unwrap()
  }

  fn run_ready(future: impl Future<Output = ()>) {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    assert!(matches!(future.as_mut().poll(&mut context), Poll::Ready(())));
  }
}
