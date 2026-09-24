#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::sync::Arc;

use octacity_server_domain::{AttemptId, BuildId, JobId, LogChunkId, LogIndexingWorkId, ProjectId, Timestamp};
use octacity_server_store::{
  BuildLogStream, ClaimLogIndexWork, CompleteLogIndexWork, DeleteLogSearchDocuments, FailLogIndexWork,
  LogChunkManifest, LogIndexPosition, LogIndexWorkKind, LogIndexWorkQueue as _, LogSearchDocument, LogSearchIndex as _,
  LogSearchMode, LogSearchMutationDisposition, LogSearchQuery, MutationDisposition, TriggerAcceptanceStore as _,
  WorkerOwner, WriteLogSearchDocument,
  testing::{authoritative_store_contract_fixture, verify_log_search_index_contract},
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresLogSearchIndex, PostgresStore};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_satisfies_the_log_search_index_contract() {
  let database = TestDatabase::migrated().await;
  verify_log_search_index_contract(Arc::new(PostgresLogSearchIndex::new(database.pool.clone()))).await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_search_uses_expected_indexes_and_matches_technical_and_mixed_language_text() {
  let database = TestDatabase::migrated().await;
  let indexes: Vec<String> = sqlx::query_scalar(
    "SELECT indexname FROM pg_indexes WHERE schemaname = 'public' AND tablename = 'log_search_documents'",
  )
  .fetch_all(&database.pool)
  .await
  .unwrap();
  assert!(indexes.iter().any(|name| name == "log_search_documents_full_text_idx"));
  assert!(
    indexes
      .iter()
      .any(|name| name == "log_search_documents_literal_trgm_idx")
  );
  let extension: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_trgm')")
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert!(extension);

  let index = PostgresLogSearchIndex::new(database.pool.clone());
  let project_id = id(700);
  let text = "Ошибка компиляции E0425 at /workspace/src/main.rs sha256:0123456789abcdef [REDACTED]";
  assert_eq!(
    index
      .index(write(701, 1, document(702, project_id, text)))
      .await
      .unwrap(),
    LogSearchMutationDisposition::Applied
  );
  for (query, mode) in [
    ("Ошибка компиляции", LogSearchMode::FullText),
    ("E0425", LogSearchMode::Literal),
    ("/workspace/src/main.rs", LogSearchMode::Literal),
    ("sha256:0123456789abcdef", LogSearchMode::Literal),
  ] {
    let page = index.search(search(project_id, query, mode)).await.unwrap();
    assert_eq!(page.hits.len(), 1, "query {query:?} must match");
    assert!(page.hits[0].snippet.contains("[REDACTED]"));
  }
  let stored: String = sqlx::query_scalar("SELECT normalized_text FROM log_search_documents WHERE project_id = $1")
    .bind(project_id.as_uuid())
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(stored, text);
  assert!(!stored.contains("secret-value"));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn search_never_returns_documents_for_authoritatively_hidden_build_logs() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  authoritative_fixture::seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer())
    .accept_trigger(fixture.request.clone())
    .await
    .unwrap();

  let project_id = fixture.request.build.project_id;
  let build_id = fixture.request.build.id;
  let mut indexed_document = document(750, project_id, "content hidden by retention");
  indexed_document.build_id = build_id;
  let index = PostgresLogSearchIndex::new(database.pool.clone());
  index.index(write(751, 1, indexed_document)).await.unwrap();
  assert_eq!(
    index
      .search(search(project_id, "hidden", LogSearchMode::FullText))
      .await
      .unwrap()
      .hits
      .len(),
    1
  );

  sqlx::query("UPDATE builds SET logs_visible = false, logs_hidden_at = to_timestamp(2) WHERE id = $1")
    .bind(build_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  assert!(
    index
      .search(search(project_id, "hidden", LogSearchMode::FullText))
      .await
      .unwrap()
      .hits
      .is_empty()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn document_rebuild_is_idempotent_after_projection_loss() {
  let database = TestDatabase::migrated().await;
  let project_id = id(800);
  let request = write(801, 1, document(802, project_id, "safe [REDACTED] output"));
  let first = PostgresLogSearchIndex::new(database.pool.clone());
  first.index(request.clone()).await.unwrap();

  sqlx::query(
    "TRUNCATE log_search_documents, log_search_applied_work, log_search_project_positions, \
     log_search_build_tombstones",
  )
  .execute(&database.pool)
  .await
  .unwrap();

  let restarted = PostgresLogSearchIndex::new(database.pool.clone());
  assert_eq!(
    restarted.rebuild(request.clone()).await.unwrap(),
    LogSearchMutationDisposition::Applied
  );
  assert_eq!(
    restarted.rebuild(request).await.unwrap(),
    LogSearchMutationDisposition::Replayed
  );
  let page = restarted
    .search(search(project_id, "safe", LogSearchMode::FullText))
    .await
    .unwrap();
  assert_eq!(page.hits.len(), 1);
  assert_eq!(page.indexed_through, LogIndexPosition::new(1).ok());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn operator_rebuild_requeues_authoritative_work_and_restores_deletion_tombstones() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  authoritative_fixture::seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer())
    .accept_trigger(fixture.request.clone())
    .await
    .unwrap();
  let project_id = fixture.request.build.project_id;
  let build_id = fixture.request.build.id;
  let job_id = fixture.request.jobs[0].id;
  let manifest = LogChunkManifest::prepare(job_id, BuildLogStream::Stdout, 1, 1, b"retained redacted log").unwrap();
  sqlx::query(
    "INSERT INTO log_chunk_manifests \
       (id, build_id, attempt_id, job_id, stream, first_sequence, last_sequence, object_identity, byte_length, \
        sha256, visible, created_at) \
     VALUES ($1, $2, $3, $4, 'stdout', 1, 1, $5, $6, $7, true, to_timestamp(1))",
  )
  .bind(manifest.chunk_id().as_uuid())
  .bind(build_id.as_uuid())
  .bind(fixture.request.attempt_id.as_uuid())
  .bind(job_id.as_uuid())
  .bind(manifest.object_identity())
  .bind(i64::try_from(manifest.byte_length()).unwrap())
  .bind(manifest.digest().as_bytes().to_vec())
  .execute(&database.pool)
  .await
  .unwrap();
  let delete_work_id = id::<LogIndexingWorkId>(999);
  sqlx::query("INSERT INTO log_index_project_positions (project_id, committed_through) VALUES ($1, 2)")
    .bind(project_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO log_indexing_work \
       (id, project_id, position, build_id, chunk_id, operation, state, attempt_count, available_at, created_at, completed_at) \
     VALUES ($1, $2, 1, $3, $4, 'index', 'completed', 1, to_timestamp(1), to_timestamp(1), to_timestamp(2)), \
            ($5, $2, 2, $3, NULL, 'delete_build', 'completed', 1, to_timestamp(1), to_timestamp(1), to_timestamp(2))",
  )
  .bind(manifest.indexing_work_id().as_uuid())
  .bind(project_id.as_uuid())
  .bind(build_id.as_uuid())
  .bind(manifest.chunk_id().as_uuid())
  .bind(delete_work_id.as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();

  let index = PostgresLogSearchIndex::new(database.pool.clone());
  let summary = index.start_rebuild(project_id).await.unwrap();
  assert_eq!(summary.queued, 2);
  assert_eq!(summary.committed_through, LogIndexPosition::new(2).ok());
  let tombstoned: bool = sqlx::query_scalar(
    "SELECT EXISTS(SELECT 1 FROM log_search_build_tombstones WHERE project_id = $1 AND build_id = $2)",
  )
  .bind(project_id.as_uuid())
  .bind(build_id.as_uuid())
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert!(tombstoned);

  let owner = WorkerOwner::new("log-index:rebuild-test").unwrap();
  let now = system_time();
  let expires = Timestamp::from_unix_millis(now.unix_millis() + 60_000).unwrap();
  let claims = PostgresStore::new(database.pool.clone())
    .claim_log_index_work(ClaimLogIndexWork::new(owner, now, expires, 2).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 2);
  assert!(matches!(claims[0].kind, LogIndexWorkKind::Rebuild(_)));
  assert!(matches!(claims[1].kind, LogIndexWorkKind::DeleteBuild));
  for claim in claims {
    let document_work = || WriteLogSearchDocument {
      work_id: claim.work_id,
      position: claim.position,
      document: LogSearchDocument {
        chunk_id: manifest.chunk_id(),
        project_id,
        build_id,
        attempt_id: fixture.request.attempt_id,
        job_id,
        stream: BuildLogStream::Stdout,
        first_sequence: 1,
        last_sequence: 1,
        occurred_at: time(1),
        redacted_text: "retained redacted log".to_owned(),
      },
    };
    let disposition = match claim.kind {
      LogIndexWorkKind::Index(_) => index.index(document_work()).await.unwrap(),
      LogIndexWorkKind::Rebuild(_) => index.rebuild(document_work()).await.unwrap(),
      LogIndexWorkKind::DeleteBuild => index
        .delete(DeleteLogSearchDocuments {
          work_id: claim.work_id,
          position: claim.position,
          project_id,
          build_id,
        })
        .await
        .unwrap(),
    };
    assert!(matches!(
      disposition,
      LogSearchMutationDisposition::Applied | LogSearchMutationDisposition::Superseded
    ));
  }
  let page = index
    .search(search(project_id, "retained", LogSearchMode::FullText))
    .await
    .unwrap();
  assert!(page.hits.is_empty());
  assert_eq!(page.indexed_through, LogIndexPosition::new(2).ok());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn indexing_retry_and_completion_survive_adapter_restart() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  authoritative_fixture::seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer())
    .accept_trigger(fixture.request.clone())
    .await
    .unwrap();
  let job_id = fixture.request.jobs[0].id;
  let bytes = b"durable redacted log";
  let manifest = LogChunkManifest::prepare(job_id, BuildLogStream::Stdout, 1, 1, bytes).unwrap();
  sqlx::query(
    "INSERT INTO log_chunk_manifests \
       (id, build_id, attempt_id, job_id, stream, first_sequence, last_sequence, object_identity, byte_length, \
        sha256, visible, created_at) \
     VALUES ($1, $2, $3, $4, 'stdout', 1, 1, $5, $6, $7, true, to_timestamp(1))",
  )
  .bind(manifest.chunk_id().as_uuid())
  .bind(fixture.request.build.id.as_uuid())
  .bind(fixture.request.attempt_id.as_uuid())
  .bind(job_id.as_uuid())
  .bind(manifest.object_identity())
  .bind(i64::try_from(manifest.byte_length()).unwrap())
  .bind(manifest.digest().as_bytes().to_vec())
  .execute(&database.pool)
  .await
  .unwrap();
  sqlx::query("INSERT INTO log_index_project_positions (project_id, committed_through) VALUES ($1, 1)")
    .bind(fixture.request.build.project_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO log_indexing_work \
       (id, project_id, position, build_id, chunk_id, operation, state, attempt_count, available_at, created_at) \
     VALUES ($1, $2, 1, $3, $4, 'index', 'pending', 0, to_timestamp(1), to_timestamp(1))",
  )
  .bind(manifest.indexing_work_id().as_uuid())
  .bind(fixture.request.build.project_id.as_uuid())
  .bind(fixture.request.build.id.as_uuid())
  .bind(manifest.chunk_id().as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();

  let owner = WorkerOwner::new("log-index:test").unwrap();
  let first = PostgresStore::new(database.pool.clone())
    .claim_log_index_work(ClaimLogIndexWork::new(owner.clone(), time(2_000), time(5_000), 1).unwrap())
    .await
    .unwrap()
    .pop()
    .unwrap();
  assert_eq!(first.attempt, 1);
  let failed = FailLogIndexWork {
    work_id: first.work_id,
    owner: first.owner,
    error_code: "index_unavailable".to_owned(),
    failed_at: time(2_500),
    retry_at: Some(time(3_000)),
  };
  let first_store = PostgresStore::new(database.pool.clone());
  assert_eq!(
    first_store.fail_log_index_work(failed.clone()).await.unwrap(),
    MutationDisposition::Applied
  );
  assert_eq!(
    first_store.fail_log_index_work(failed).await.unwrap(),
    MutationDisposition::Replayed
  );

  let restarted = PostgresStore::new(database.pool.clone());
  assert!(
    restarted
      .claim_log_index_work(ClaimLogIndexWork::new(owner.clone(), time(2_999), time(6_000), 1).unwrap())
      .await
      .unwrap()
      .is_empty()
  );
  let second = restarted
    .claim_log_index_work(ClaimLogIndexWork::new(owner, time(3_000), time(6_000), 1).unwrap())
    .await
    .unwrap()
    .pop()
    .unwrap();
  assert_eq!(second.attempt, 2);
  let completed = CompleteLogIndexWork {
    work_id: second.work_id,
    owner: second.owner,
    completed_at: time(3_500),
  };
  assert_eq!(
    restarted.complete_log_index_work(completed.clone()).await.unwrap(),
    MutationDisposition::Applied
  );
  assert_eq!(
    restarted.complete_log_index_work(completed).await.unwrap(),
    MutationDisposition::Replayed
  );
  database.cleanup().await;
}

fn document(value: u128, project_id: ProjectId, text: &str) -> LogSearchDocument {
  LogSearchDocument {
    chunk_id: id(value),
    project_id,
    build_id: id(value + 1),
    attempt_id: id(value + 2),
    job_id: id(value + 3),
    stream: BuildLogStream::Stderr,
    first_sequence: 1,
    last_sequence: 2,
    occurred_at: Timestamp::from_unix_millis(i64::try_from(value).unwrap()).unwrap(),
    redacted_text: text.to_owned(),
  }
}

fn write(work: u128, position: u64, document: LogSearchDocument) -> WriteLogSearchDocument {
  WriteLogSearchDocument {
    work_id: id(work),
    position: LogIndexPosition::new(position).unwrap(),
    document,
  }
}

fn search(project_id: ProjectId, text: &str, mode: LogSearchMode) -> LogSearchQuery {
  LogSearchQuery {
    project_id,
    text: text.to_owned(),
    mode,
    build_id: None,
    attempt_id: None,
    job_id: None,
    stream: None,
    occurred_from: None,
    occurred_through: None,
    after: None,
    limit: 10,
  }
}

trait FromUuid: Sized {
  fn from_uuid(value: uuid::Uuid) -> Option<Self>;
}

macro_rules! impl_from_uuid {
  ($($kind:ty),+ $(,)?) => {$(
    impl FromUuid for $kind {
      fn from_uuid(value: uuid::Uuid) -> Option<Self> {
        <$kind>::from_uuid(value).ok()
      }
    }
  )+};
}

impl_from_uuid!(ProjectId, BuildId, AttemptId, JobId, LogChunkId, LogIndexingWorkId);

fn id<T: FromUuid>(value: u128) -> T {
  T::from_uuid(uuid::Uuid::from_u128(value)).expect("test UUID is non-nil")
}

fn time(milliseconds: i64) -> Timestamp {
  Timestamp::from_unix_millis(milliseconds).unwrap()
}

fn system_time() -> Timestamp {
  let milliseconds = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis();
  Timestamp::from_unix_millis(i64::try_from(milliseconds).unwrap()).unwrap()
}
