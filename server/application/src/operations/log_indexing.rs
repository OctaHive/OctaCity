use std::{
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};

use octacity_artifact_store::{LogChunkStore, LogChunkStoreError};
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_store::{
  ClaimLogIndexWork, CompleteLogIndexWork, DeleteLogSearchDocuments, FailLogIndexWork, LogIndexDocumentSource,
  LogIndexWorkClaim, LogIndexWorkKind, LogIndexWorkQueue, LogSearchDocument, LogSearchError, LogSearchIndex,
  MAX_LOG_SEARCH_DOCUMENT_BYTES, WorkerOwner, WriteLogSearchDocument,
};
use thiserror::Error;

use crate::DurableRetryPolicy;

/// Counts produced by one bounded Build-log indexing pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LogIndexingBatchOutcome {
  /// Durable work items acquired by this replica.
  pub claimed: usize,
  /// Items applied or replayed by the derived index and durably completed.
  pub completed: usize,
  /// Transient failures scheduled for another attempt.
  pub retries_scheduled: usize,
  /// Invalid, corrupt, or retry-exhausted items retained as dead letters.
  pub dead_letters: usize,
  /// Items whose claim expired or was reset before durable settlement.
  pub lost_claims: usize,
}

/// Restart-safe worker that projects immutable redacted log chunks.
pub struct LogIndexingWorker<W, I> {
  work: Arc<W>,
  index: Arc<I>,
  objects: Arc<dyn LogChunkStore>,
  retry_policy: DurableRetryPolicy,
  clock: Arc<dyn LogIndexClock>,
}

impl<W, I> LogIndexingWorker<W, I>
where
  W: LogIndexWorkQueue,
  I: LogSearchIndex,
{
  /// Creates a worker over durable work, one derived index, and immutable bytes.
  pub fn new(work: Arc<W>, index: Arc<I>, objects: Arc<dyn LogChunkStore>, retry_policy: DurableRetryPolicy) -> Self {
    Self {
      work,
      index,
      objects,
      retry_policy,
      clock: Arc::new(SystemLogIndexClock),
    }
  }

  #[cfg(test)]
  fn with_clock(
    work: Arc<W>,
    index: Arc<I>,
    objects: Arc<dyn LogChunkStore>,
    retry_policy: DurableRetryPolicy,
    clock: Arc<dyn LogIndexClock>,
  ) -> Self {
    Self {
      work,
      index,
      objects,
      retry_policy,
      clock,
    }
  }

  /// Claims and advances one bounded durable batch.
  pub async fn run_once(
    &self,
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<LogIndexingBatchOutcome, LogIndexingWorkerError> {
    let claims = self
      .work
      .claim_log_index_work(ClaimLogIndexWork::new(owner, observed_at, claim_expires_at, limit)?)
      .await?;
    let mut outcome = LogIndexingBatchOutcome {
      claimed: claims.len(),
      ..LogIndexingBatchOutcome::default()
    };
    for claim in claims {
      match self.apply(&claim).await {
        Ok(()) => {
          let completed_at = self.clock.now()?;
          let settlement = self
            .work
            .complete_log_index_work(CompleteLogIndexWork {
              work_id: claim.work_id,
              owner: claim.owner,
              completed_at,
            })
            .await;
          if settlement_lost(settlement)? {
            outcome.lost_claims += 1;
          } else {
            outcome.completed += 1;
          }
        }
        Err(failure) => {
          let failed_at = self.clock.now()?;
          let retry_at = failure
            .retryable()
            .then(|| self.retry_policy.retry_at(claim.attempt, failed_at))
            .flatten();
          let settlement = self
            .work
            .fail_log_index_work(FailLogIndexWork {
              work_id: claim.work_id,
              owner: claim.owner,
              error_code: failure.code().to_owned(),
              failed_at,
              retry_at,
            })
            .await;
          if settlement_lost(settlement)? {
            outcome.lost_claims += 1;
          } else if retry_at.is_some() {
            outcome.retries_scheduled += 1;
          } else {
            outcome.dead_letters += 1;
          }
        }
      }
    }
    Ok(outcome)
  }

  async fn apply(&self, claim: &LogIndexWorkClaim) -> Result<(), IndexingFailure> {
    match &claim.kind {
      LogIndexWorkKind::Index(source) => {
        let request = self.document_request(claim, source).await?;
        self.index.index(request).await.map_err(IndexingFailure::from)?;
      }
      LogIndexWorkKind::Rebuild(source) => {
        let request = self.document_request(claim, source).await?;
        self.index.rebuild(request).await.map_err(IndexingFailure::from)?;
      }
      LogIndexWorkKind::DeleteBuild => {
        self
          .index
          .delete(DeleteLogSearchDocuments {
            work_id: claim.work_id,
            position: claim.position,
            project_id: claim.project_id,
            build_id: claim.build_id,
          })
          .await
          .map_err(IndexingFailure::from)?;
      }
    }
    Ok(())
  }

  async fn document_request(
    &self,
    claim: &LogIndexWorkClaim,
    source: &LogIndexDocumentSource,
  ) -> Result<WriteLogSearchDocument, IndexingFailure> {
    let bytes = self
      .objects
      .read_verified(&source.manifest)
      .await
      .map_err(IndexingFailure::from)?;
    let redacted_text = normalized_text(&bytes);
    let document = LogSearchDocument {
      chunk_id: source.manifest.chunk_id(),
      project_id: claim.project_id,
      build_id: claim.build_id,
      attempt_id: source.attempt_id,
      job_id: source.job_id,
      stream: source.manifest.stream(),
      first_sequence: source.manifest.first_sequence(),
      last_sequence: source.manifest.last_sequence(),
      occurred_at: source.occurred_at,
      redacted_text,
    };
    document.validate().map_err(|_| IndexingFailure::InvalidDocument)?;
    Ok(WriteLogSearchDocument {
      work_id: claim.work_id,
      position: claim.position,
      document,
    })
  }
}

fn settlement_lost(
  result: Result<octacity_server_store::MutationDisposition, octacity_server_store::StoreError>,
) -> Result<bool, LogIndexingWorkerError> {
  match result {
    Ok(_) => Ok(false),
    Err(octacity_server_store::StoreError::Conflict {
      entity: EntityKind::LogIndexingWork,
    }) => Ok(true),
    Err(error) => Err(error.into()),
  }
}

trait LogIndexClock: Send + Sync {
  fn now(&self) -> Result<Timestamp, LogIndexingWorkerError>;
}

struct SystemLogIndexClock;

impl LogIndexClock for SystemLogIndexClock {
  fn now(&self) -> Result<Timestamp, LogIndexingWorkerError> {
    let milliseconds = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .ok()
      .and_then(|duration| i64::try_from(duration.as_millis()).ok())
      .ok_or(LogIndexingWorkerError::ClockUnavailable)?;
    Timestamp::from_unix_millis(milliseconds).map_err(|_| LogIndexingWorkerError::ClockUnavailable)
  }
}

fn normalized_text(bytes: &[u8]) -> String {
  let text = String::from_utf8_lossy(bytes);
  if text.len() <= MAX_LOG_SEARCH_DOCUMENT_BYTES {
    return text.into_owned();
  }
  let mut end = MAX_LOG_SEARCH_DOCUMENT_BYTES;
  while !text.is_char_boundary(end) {
    end -= 1;
  }
  text[..end].to_owned()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IndexingFailure {
  ObjectUnavailable,
  ObjectIntegrity,
  InvalidDocument,
  IndexUnavailable,
  IndexConflict,
}

impl IndexingFailure {
  const fn retryable(self) -> bool {
    matches!(self, Self::ObjectUnavailable | Self::IndexUnavailable)
  }

  const fn code(self) -> &'static str {
    match self {
      Self::ObjectUnavailable => "object_unavailable",
      Self::ObjectIntegrity => "object_integrity",
      Self::InvalidDocument => "invalid_document",
      Self::IndexUnavailable => "index_unavailable",
      Self::IndexConflict => "index_conflict",
    }
  }
}

impl From<LogChunkStoreError> for IndexingFailure {
  fn from(error: LogChunkStoreError) -> Self {
    match error {
      LogChunkStoreError::Unavailable | LogChunkStoreError::NotFound => Self::ObjectUnavailable,
      LogChunkStoreError::Integrity => Self::ObjectIntegrity,
    }
  }
}

impl From<LogSearchError> for IndexingFailure {
  fn from(error: LogSearchError) -> Self {
    match error {
      LogSearchError::Unavailable => Self::IndexUnavailable,
      LogSearchError::InvalidInput { .. }
      | LogSearchError::WorkConflict { .. }
      | LogSearchError::DocumentConflict { .. } => Self::IndexConflict,
    }
  }
}

/// Failure to claim or durably settle a Build-log indexing item.
#[derive(Debug, Error)]
pub enum LogIndexingWorkerError {
  /// Authoritative durable work could not be claimed or settled.
  #[error("log indexing work store failed")]
  Store(#[from] octacity_server_store::StoreError),
  /// The process clock could not produce a representable settlement time.
  #[error("system clock is unavailable for log indexing settlement")]
  ClockUnavailable,
}

#[cfg(test)]
mod tests {
  use std::{
    future::Future,
    sync::Mutex,
    task::{Context, Poll, Waker},
  };

  use async_trait::async_trait;
  use octacity_artifact_store::LogChunkWrite;
  use octacity_server_domain::{AttemptId, BuildId, JobId, LogIndexingWorkId, ProjectId};
  use octacity_server_store::{
    IndexedLogSearchPage, LogIndexPosition, LogSearchMutationDisposition, MutationDisposition, StoreError,
  };

  use super::*;

  #[test]
  fn invalid_utf8_is_normalized_without_exceeding_the_document_bound() {
    let bytes = vec![0xff; MAX_LOG_SEARCH_DOCUMENT_BYTES];
    let text = normalized_text(&bytes);
    assert!(text.len() <= MAX_LOG_SEARCH_DOCUMENT_BYTES);
    assert!(std::str::from_utf8(text.as_bytes()).is_ok());
  }

  #[test]
  fn settles_each_item_with_fresh_time_instead_of_the_batch_start() {
    run_ready(async {
      let owner = WorkerOwner::new("log-index:test").unwrap();
      let queue = Arc::new(FakeQueue {
        claims: Mutex::new(vec![claim(owner.clone(), time(30))]),
        completed: Mutex::new(Vec::new()),
        failed: Mutex::new(Vec::new()),
        reject_settlement: false,
      });
      let worker = LogIndexingWorker::with_clock(
        queue.clone(),
        Arc::new(FakeIndex::new(false)),
        Arc::new(NeverObjects),
        DurableRetryPolicy::new(3, 10, 100).unwrap(),
        Arc::new(FixedClock(time(20))),
      );

      worker.run_once(owner, time(10), time(30), 1).await.unwrap();

      assert_eq!(queue.completed.lock().unwrap()[0].completed_at, time(20));
    });
  }

  #[test]
  fn retry_delay_starts_at_the_actual_failure_time() {
    run_ready(async {
      let owner = WorkerOwner::new("log-index:test").unwrap();
      let queue = Arc::new(FakeQueue {
        claims: Mutex::new(vec![claim(owner.clone(), time(30))]),
        completed: Mutex::new(Vec::new()),
        failed: Mutex::new(Vec::new()),
        reject_settlement: false,
      });
      let worker = LogIndexingWorker::with_clock(
        queue.clone(),
        Arc::new(FakeIndex::new(true)),
        Arc::new(NeverObjects),
        DurableRetryPolicy::new(3, 10, 100).unwrap(),
        Arc::new(FixedClock(time(20))),
      );

      worker.run_once(owner, time(10), time(30), 1).await.unwrap();

      let failed = queue.failed.lock().unwrap();
      assert_eq!(failed[0].failed_at, time(20));
      assert_eq!(failed[0].retry_at, Some(time(30)));
    });
  }

  #[test]
  fn reads_verified_redacted_bytes_before_indexing_and_completing_work() {
    run_ready(async {
      let owner = WorkerOwner::new("log-index:test").unwrap();
      let bytes = b"safe [REDACTED] output".to_vec();
      let job_id = JobId::from_uuid(uuid::Uuid::from_u128(4)).unwrap();
      let manifest = octacity_server_store::LogChunkManifest::prepare(
        job_id,
        octacity_server_store::BuildLogStream::Stdout,
        1,
        2,
        &bytes,
      )
      .unwrap();
      let queue = Arc::new(FakeQueue {
        claims: Mutex::new(vec![LogIndexWorkClaim {
          work_id: manifest.indexing_work_id(),
          position: LogIndexPosition::new(1).unwrap(),
          project_id: ProjectId::from_uuid(uuid::Uuid::from_u128(2)).unwrap(),
          build_id: BuildId::from_uuid(uuid::Uuid::from_u128(3)).unwrap(),
          kind: LogIndexWorkKind::Index(LogIndexDocumentSource {
            manifest,
            attempt_id: AttemptId::from_uuid(uuid::Uuid::from_u128(5)).unwrap(),
            job_id,
            occurred_at: time(9),
          }),
          attempt: 1,
          owner: owner.clone(),
          claim_expires_at: time(30),
        }]),
        completed: Mutex::new(Vec::new()),
        failed: Mutex::new(Vec::new()),
        reject_settlement: false,
      });
      let index = Arc::new(FakeIndex::new(false));
      let worker = LogIndexingWorker::with_clock(
        queue.clone(),
        index.clone(),
        Arc::new(FakeObjects { bytes: bytes.clone() }),
        DurableRetryPolicy::new(3, 10, 100).unwrap(),
        Arc::new(FixedClock(time(20))),
      );

      let outcome = worker.run_once(owner, time(10), time(30), 1).await.unwrap();

      assert_eq!(outcome.completed, 1);
      assert!(queue.failed.lock().unwrap().is_empty());
      assert_eq!(queue.completed.lock().unwrap().len(), 1);
      let indexed = index.indexed.lock().unwrap();
      assert_eq!(indexed.len(), 1);
      assert_eq!(indexed[0].document.redacted_text.as_bytes(), bytes);
    });
  }

  #[test]
  fn a_claim_reset_during_projection_is_not_a_fatal_worker_failure() {
    run_ready(async {
      let owner = WorkerOwner::new("log-index:test").unwrap();
      let queue = Arc::new(FakeQueue {
        claims: Mutex::new(vec![claim(owner.clone(), time(30))]),
        completed: Mutex::new(Vec::new()),
        failed: Mutex::new(Vec::new()),
        reject_settlement: true,
      });
      let worker = LogIndexingWorker::with_clock(
        queue,
        Arc::new(FakeIndex::new(false)),
        Arc::new(NeverObjects),
        DurableRetryPolicy::new(3, 10, 100).unwrap(),
        Arc::new(FixedClock(time(20))),
      );

      let outcome = worker.run_once(owner, time(10), time(30), 1).await.unwrap();

      assert_eq!(outcome.lost_claims, 1);
      assert_eq!(outcome.completed, 0);
    });
  }

  struct FixedClock(Timestamp);

  impl LogIndexClock for FixedClock {
    fn now(&self) -> Result<Timestamp, LogIndexingWorkerError> {
      Ok(self.0)
    }
  }

  struct FakeQueue {
    claims: Mutex<Vec<LogIndexWorkClaim>>,
    completed: Mutex<Vec<CompleteLogIndexWork>>,
    failed: Mutex<Vec<FailLogIndexWork>>,
    reject_settlement: bool,
  }

  #[async_trait]
  impl LogIndexWorkQueue for FakeQueue {
    async fn claim_log_index_work(&self, _request: ClaimLogIndexWork) -> Result<Vec<LogIndexWorkClaim>, StoreError> {
      Ok(std::mem::take(&mut *self.claims.lock().unwrap()))
    }

    async fn complete_log_index_work(&self, request: CompleteLogIndexWork) -> Result<MutationDisposition, StoreError> {
      if self.reject_settlement {
        return Err(StoreError::Conflict {
          entity: EntityKind::LogIndexingWork,
        });
      }
      self.completed.lock().unwrap().push(request);
      Ok(MutationDisposition::Applied)
    }

    async fn fail_log_index_work(&self, request: FailLogIndexWork) -> Result<MutationDisposition, StoreError> {
      if self.reject_settlement {
        return Err(StoreError::Conflict {
          entity: EntityKind::LogIndexingWork,
        });
      }
      self.failed.lock().unwrap().push(request);
      Ok(MutationDisposition::Applied)
    }
  }

  struct FakeIndex {
    fail_delete: bool,
    indexed: Mutex<Vec<WriteLogSearchDocument>>,
  }

  impl FakeIndex {
    fn new(fail_delete: bool) -> Self {
      Self {
        fail_delete,
        indexed: Mutex::new(Vec::new()),
      }
    }
  }

  #[async_trait]
  impl LogSearchIndex for FakeIndex {
    async fn search(
      &self,
      _query: octacity_server_store::LogSearchQuery,
    ) -> Result<IndexedLogSearchPage, LogSearchError> {
      unreachable!("the worker does not search")
    }

    async fn index(&self, request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError> {
      self.indexed.lock().unwrap().push(request);
      Ok(LogSearchMutationDisposition::Applied)
    }

    async fn indexed_through(&self, _project_id: ProjectId) -> Result<Option<LogIndexPosition>, LogSearchError> {
      unreachable!("the worker does not query freshness")
    }

    async fn delete(&self, _request: DeleteLogSearchDocuments) -> Result<LogSearchMutationDisposition, LogSearchError> {
      if self.fail_delete {
        Err(LogSearchError::Unavailable)
      } else {
        Ok(LogSearchMutationDisposition::Applied)
      }
    }

    async fn rebuild(&self, _request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError> {
      unreachable!("the fixture contains deletion work")
    }
  }

  struct NeverObjects;

  #[async_trait]
  impl LogChunkStore for NeverObjects {
    async fn put_verified(
      &self,
      _manifest: &octacity_server_store::LogChunkManifest,
      _bytes: Vec<u8>,
    ) -> Result<LogChunkWrite, LogChunkStoreError> {
      unreachable!("the index worker never writes objects")
    }

    async fn read_verified(
      &self,
      _manifest: &octacity_server_store::LogChunkManifest,
    ) -> Result<Vec<u8>, LogChunkStoreError> {
      unreachable!("the fixture contains deletion work")
    }

    async fn delete_chunk(
      &self,
      _manifest: &octacity_server_store::LogChunkManifest,
    ) -> Result<(), LogChunkStoreError> {
      unreachable!("the index worker never deletes objects")
    }
  }

  struct FakeObjects {
    bytes: Vec<u8>,
  }

  #[async_trait]
  impl LogChunkStore for FakeObjects {
    async fn put_verified(
      &self,
      _manifest: &octacity_server_store::LogChunkManifest,
      _bytes: Vec<u8>,
    ) -> Result<LogChunkWrite, LogChunkStoreError> {
      unreachable!("the index worker never writes objects")
    }

    async fn read_verified(
      &self,
      manifest: &octacity_server_store::LogChunkManifest,
    ) -> Result<Vec<u8>, LogChunkStoreError> {
      manifest
        .verify(&self.bytes)
        .map_err(|_| LogChunkStoreError::Integrity)?;
      Ok(self.bytes.clone())
    }

    async fn delete_chunk(
      &self,
      _manifest: &octacity_server_store::LogChunkManifest,
    ) -> Result<(), LogChunkStoreError> {
      unreachable!("the index worker never deletes objects")
    }
  }

  fn claim(owner: WorkerOwner, claim_expires_at: Timestamp) -> LogIndexWorkClaim {
    LogIndexWorkClaim {
      work_id: LogIndexingWorkId::from_uuid(uuid::Uuid::from_u128(1)).unwrap(),
      position: LogIndexPosition::new(1).unwrap(),
      project_id: ProjectId::from_uuid(uuid::Uuid::from_u128(2)).unwrap(),
      build_id: BuildId::from_uuid(uuid::Uuid::from_u128(3)).unwrap(),
      kind: LogIndexWorkKind::DeleteBuild,
      attempt: 1,
      owner,
      claim_expires_at,
    }
  }

  fn time(milliseconds: i64) -> Timestamp {
    Timestamp::from_unix_millis(milliseconds).unwrap()
  }

  fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(value) => value,
      Poll::Pending => panic!("in-memory worker future unexpectedly yielded"),
    }
  }
}
