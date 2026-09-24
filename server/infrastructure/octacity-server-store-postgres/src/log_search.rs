use async_trait::async_trait;
use futures_util::TryStreamExt as _;
use octacity_server_domain::{AttemptId, BuildId, JobId, LogChunkId, LogIndexingWorkId, ProjectId, Timestamp};
use octacity_server_store::{
  BuildLogStream, DeleteLogSearchDocuments, IndexedLogSearchPage, LogIndexPosition, LogSearchCursor, LogSearchDocument,
  LogSearchError, LogSearchHit, LogSearchIndex, LogSearchMode, LogSearchMutationDisposition, LogSearchOperation,
  LogSearchQuery, WriteLogSearchDocument, bounded_log_search_snippet,
};
use sha2::{Digest as _, Sha256};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};

/// Rebuildable PostgreSQL projection implementing backend-neutral Build-log search.
#[derive(Clone)]
pub struct PostgresLogSearchIndex {
  pool: PgPool,
}

impl PostgresLogSearchIndex {
  /// Creates a search projection over a migrated PostgreSQL pool.
  #[must_use]
  pub const fn new(pool: PgPool) -> Self {
    Self { pool }
  }

  /// Clears one Project's derived search state and requeues its authoritative
  /// log-index work for bounded processing by the running worker.
  ///
  /// Deletion work is retained and its tombstones are reconstructed before
  /// any document can be replayed, so rebuilding cannot make deleted Build
  /// logs visible again.
  pub async fn start_rebuild(&self, project_id: ProjectId) -> Result<LogSearchRebuildSummary, LogSearchError> {
    let mut transaction = self.pool.begin().await.map_err(unavailable)?;
    let committed_through: Option<i64> =
      sqlx::query_scalar("SELECT committed_through FROM log_index_project_positions WHERE project_id = $1 FOR UPDATE")
        .bind(project_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(unavailable)?;
    lock_project(&mut transaction, project_id).await?;

    sqlx::query("DELETE FROM log_search_documents WHERE project_id = $1")
      .bind(project_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
    sqlx::query("DELETE FROM log_search_applied_work WHERE project_id = $1")
      .bind(project_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
    sqlx::query("UPDATE log_search_project_positions SET indexed_through = 0 WHERE project_id = $1")
      .bind(project_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
    sqlx::query(
      "INSERT INTO log_search_build_tombstones (project_id, build_id, deleted_at_position) \
       SELECT project_id, build_id, MAX(position) FROM log_indexing_work \
       WHERE project_id = $1 AND operation = 'delete_build' GROUP BY project_id, build_id \
       ON CONFLICT (project_id, build_id) DO UPDATE SET deleted_at_position = \
         GREATEST(log_search_build_tombstones.deleted_at_position, EXCLUDED.deleted_at_position)",
    )
    .bind(project_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;

    let queued = match committed_through {
      Some(committed_through) => sqlx::query(
        "UPDATE log_indexing_work SET state = 'pending', \
           operation = CASE WHEN operation = 'index' THEN 'rebuild' ELSE operation END, \
           attempt_count = 0, available_at = now(), \
           claim_owner = NULL, claim_expires_at = NULL, completed_at = NULL, last_error_code = NULL, \
           last_claim_owner = NULL, last_failure_at = NULL, last_retry_at = NULL \
         WHERE project_id = $1 AND position <= $2 AND operation IN ('index', 'rebuild', 'delete_build')",
      )
      .bind(project_id.as_uuid())
      .bind(committed_through)
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?
      .rows_affected(),
      None => 0,
    };
    let committed_through = committed_through
      .map(|position| {
        u64::try_from(position)
          .ok()
          .and_then(|position| LogIndexPosition::new(position).ok())
          .ok_or(LogSearchError::Unavailable)
      })
      .transpose()?;
    transaction.commit().await.map_err(unavailable)?;
    Ok(LogSearchRebuildSummary {
      project_id,
      queued,
      committed_through,
    })
  }
}

/// Durable work prepared by one operator-initiated projection rebuild.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogSearchRebuildSummary {
  /// Project whose derived search projection was reset.
  pub project_id: ProjectId,
  /// Existing authoritative work items made eligible for bounded replay.
  pub queued: u64,
  /// Authoritative position through which work was requeued.
  pub committed_through: Option<LogIndexPosition>,
}

#[async_trait]
impl LogSearchIndex for PostgresLogSearchIndex {
  async fn search(&self, query: LogSearchQuery) -> Result<IndexedLogSearchPage, LogSearchError> {
    query
      .validate()
      .map_err(|source| LogSearchError::invalid(LogSearchOperation::Search, source))?;
    let mut transaction = self.pool.begin().await.map_err(unavailable)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;

    let mut sql = QueryBuilder::<Postgres>::new(
      "SELECT chunk_id, build_id, attempt_id, job_id, stream, first_sequence, last_sequence, \
       FLOOR(EXTRACT(EPOCH FROM occurred_at) * 1000)::BIGINT AS occurred_at_millis, normalized_text \
       FROM log_search_documents WHERE project_id = ",
    );
    sql.push_bind(query.project_id.as_uuid());
    match query.mode {
      LogSearchMode::FullText => {
        sql.push(" AND search_vector @@ plainto_tsquery('simple', ");
        sql.push_bind(&query.text);
        sql.push(")");
      }
      LogSearchMode::Literal => {
        sql.push(" AND normalized_text LIKE ");
        sql.push_bind(literal_pattern(&query.text));
        sql.push(" ESCAPE '\\'");
      }
    }
    if let Some(build_id) = query.build_id {
      sql.push(" AND build_id = ").push_bind(build_id.as_uuid());
    }
    if let Some(attempt_id) = query.attempt_id {
      sql.push(" AND attempt_id = ").push_bind(attempt_id.as_uuid());
    }
    if let Some(job_id) = query.job_id {
      sql.push(" AND job_id = ").push_bind(job_id.as_uuid());
    }
    if let Some(stream) = query.stream {
      sql.push(" AND stream = ").push_bind(stream.as_str());
    }
    if let Some(from) = query.occurred_from {
      sql
        .push(" AND occurred_at >= to_timestamp(")
        .push_bind(from.unix_millis())
        .push("::double precision / 1000.0)");
    }
    if let Some(through) = query.occurred_through {
      sql
        .push(" AND occurred_at <= to_timestamp(")
        .push_bind(through.unix_millis())
        .push("::double precision / 1000.0)");
    }
    if let Some(after) = query.after {
      sql
        .push(" AND (occurred_at, chunk_id) < (to_timestamp(")
        .push_bind(after.occurred_at.unix_millis())
        .push("::double precision / 1000.0), ")
        .push_bind(after.chunk_id.as_uuid())
        .push(")");
    }
    sql
      .push(" ORDER BY occurred_at DESC, chunk_id DESC LIMIT ")
      .push_bind(i64::from(query.limit) + 1);

    let mut rows = sql.build_query_as::<SearchRow>().fetch(&mut *transaction);
    let mut hits = Vec::with_capacity(usize::from(query.limit));
    let mut has_more = false;
    while let Some(row) = rows.try_next().await.map_err(unavailable)? {
      if hits.len() == usize::from(query.limit) {
        has_more = true;
        break;
      }
      hits.push(row.into_hit(&query)?);
    }
    drop(rows);
    let indexed_through = read_indexed_through(&mut *transaction, query.project_id).await?;
    transaction.commit().await.map_err(unavailable)?;

    let next_cursor = has_more.then(|| {
      let last = hits.last().expect("a page with more results cannot be empty");
      LogSearchCursor {
        occurred_at: last.occurred_at,
        chunk_id: last.chunk_id,
      }
    });
    Ok(IndexedLogSearchPage {
      hits,
      next_cursor,
      indexed_through,
    })
  }

  async fn index(&self, request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError> {
    apply_document(&self.pool, request, AppliedOperation::Index).await
  }

  async fn indexed_through(&self, project_id: ProjectId) -> Result<Option<LogIndexPosition>, LogSearchError> {
    read_indexed_through(&self.pool, project_id).await
  }

  async fn delete(&self, request: DeleteLogSearchDocuments) -> Result<LogSearchMutationDisposition, LogSearchError> {
    let digest = delete_digest(&request);
    let mut transaction = self.pool.begin().await.map_err(unavailable)?;
    lock_project(&mut transaction, request.project_id).await?;
    if let Some(disposition) = replay(&mut transaction, request.work_id, AppliedOperation::Delete, &digest).await? {
      transaction.commit().await.map_err(unavailable)?;
      return Ok(disposition);
    }
    sqlx::query(
      "INSERT INTO log_search_build_tombstones (project_id, build_id, deleted_at_position) VALUES ($1, $2, $3) \
       ON CONFLICT (project_id, build_id) DO UPDATE SET deleted_at_position = \
         GREATEST(log_search_build_tombstones.deleted_at_position, EXCLUDED.deleted_at_position)",
    )
    .bind(request.project_id.as_uuid())
    .bind(request.build_id.as_uuid())
    .bind(position_number(request.position)?)
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    sqlx::query("DELETE FROM log_search_documents WHERE project_id = $1 AND build_id = $2")
      .bind(request.project_id.as_uuid())
      .bind(request.build_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
    record_work(
      &mut transaction,
      AppliedWork {
        work_id: request.work_id,
        project_id: request.project_id,
        position: request.position,
        operation: AppliedOperation::Delete,
        digest: &digest,
        disposition: LogSearchMutationDisposition::Applied,
      },
    )
    .await?;
    advance_freshness(&mut transaction, request.project_id).await?;
    transaction.commit().await.map_err(unavailable)?;
    Ok(LogSearchMutationDisposition::Applied)
  }

  async fn rebuild(&self, request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError> {
    apply_document(&self.pool, request, AppliedOperation::Rebuild).await
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppliedOperation {
  Index,
  Delete,
  Rebuild,
}

impl AppliedOperation {
  const fn as_str(self) -> &'static str {
    match self {
      Self::Index => "index",
      Self::Delete => "delete_build",
      Self::Rebuild => "rebuild",
    }
  }

  const fn log_operation(self) -> LogSearchOperation {
    match self {
      Self::Index => LogSearchOperation::Index,
      Self::Delete => LogSearchOperation::Delete,
      Self::Rebuild => LogSearchOperation::Rebuild,
    }
  }

  fn replays(self, recorded: &str) -> bool {
    recorded == self.as_str() || matches!(self, Self::Index | Self::Rebuild) && matches!(recorded, "index" | "rebuild")
  }
}

async fn apply_document(
  pool: &PgPool,
  request: WriteLogSearchDocument,
  operation: AppliedOperation,
) -> Result<LogSearchMutationDisposition, LogSearchError> {
  request
    .document
    .validate()
    .map_err(|source| LogSearchError::invalid(operation.log_operation(), source))?;
  let digest = document_digest(&request);
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  lock_project(&mut transaction, request.document.project_id).await?;
  if let Some(disposition) = replay(&mut transaction, request.work_id, operation, &digest).await? {
    transaction.commit().await.map_err(unavailable)?;
    return Ok(disposition);
  }

  let tombstoned: bool = sqlx::query_scalar(
    "SELECT EXISTS(SELECT 1 FROM log_search_build_tombstones WHERE project_id = $1 AND build_id = $2)",
  )
  .bind(request.document.project_id.as_uuid())
  .bind(request.document.build_id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let disposition = if tombstoned {
    LogSearchMutationDisposition::Superseded
  } else {
    persist_document(&mut transaction, &request.document).await?;
    LogSearchMutationDisposition::Applied
  };
  record_work(
    &mut transaction,
    AppliedWork {
      work_id: request.work_id,
      project_id: request.document.project_id,
      position: request.position,
      operation,
      digest: &digest,
      disposition,
    },
  )
  .await?;
  advance_freshness(&mut transaction, request.document.project_id).await?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(disposition)
}

async fn persist_document(
  transaction: &mut sqlx::Transaction<'_, Postgres>,
  document: &LogSearchDocument,
) -> Result<(), LogSearchError> {
  let inserted = sqlx::query(
    "INSERT INTO log_search_documents \
       (id, chunk_id, project_id, build_id, attempt_id, job_id, stream, first_sequence, last_sequence, \
        occurred_at, normalized_text, indexed_at) \
     VALUES ($1, $1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9::double precision / 1000.0), $10, now()) \
     ON CONFLICT (chunk_id) DO NOTHING",
  )
  .bind(document.chunk_id.as_uuid())
  .bind(document.project_id.as_uuid())
  .bind(document.build_id.as_uuid())
  .bind(document.attempt_id.as_uuid())
  .bind(document.job_id.as_uuid())
  .bind(document.stream.as_str())
  .bind(sequence_number(document.first_sequence)?)
  .bind(sequence_number(document.last_sequence)?)
  .bind(document.occurred_at.unix_millis())
  .bind(&document.redacted_text)
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if inserted.rows_affected() == 1 {
    return Ok(());
  }
  let existing = sqlx::query_as::<_, SearchRow>(
    "SELECT chunk_id, build_id, attempt_id, job_id, stream, first_sequence, last_sequence, \
     FLOOR(EXTRACT(EPOCH FROM occurred_at) * 1000)::BIGINT AS occurred_at_millis, normalized_text \
     FROM log_search_documents WHERE chunk_id = $1",
  )
  .bind(document.chunk_id.as_uuid())
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if existing.into_document(document.project_id)? == *document {
    Ok(())
  } else {
    Err(LogSearchError::DocumentConflict {
      chunk_id: document.chunk_id,
    })
  }
}

async fn lock_project(
  transaction: &mut sqlx::Transaction<'_, Postgres>,
  project_id: ProjectId,
) -> Result<(), LogSearchError> {
  sqlx::query(
    "INSERT INTO log_search_project_positions (project_id, indexed_through) VALUES ($1, 0) \
     ON CONFLICT (project_id) DO NOTHING",
  )
  .bind(project_id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  sqlx::query_scalar::<_, i64>(
    "SELECT indexed_through FROM log_search_project_positions WHERE project_id = $1 FOR UPDATE",
  )
  .bind(project_id.as_uuid())
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn replay(
  transaction: &mut sqlx::Transaction<'_, Postgres>,
  work_id: LogIndexingWorkId,
  operation: AppliedOperation,
  digest: &[u8; 32],
) -> Result<Option<LogSearchMutationDisposition>, LogSearchError> {
  let recorded: Option<(String, Vec<u8>, String)> =
    sqlx::query_as("SELECT operation, request_digest, disposition FROM log_search_applied_work WHERE work_id = $1")
      .bind(work_id.as_uuid())
      .fetch_optional(&mut **transaction)
      .await
      .map_err(unavailable)?;
  let Some((recorded_operation, recorded_digest, disposition)) = recorded else {
    return Ok(None);
  };
  if !operation.replays(&recorded_operation) || recorded_digest.as_slice() != digest {
    return Err(LogSearchError::WorkConflict { work_id });
  }
  match disposition.as_str() {
    "applied" | "superseded" => Ok(Some(LogSearchMutationDisposition::Replayed)),
    _ => Err(LogSearchError::Unavailable),
  }
}

struct AppliedWork<'a> {
  work_id: LogIndexingWorkId,
  project_id: ProjectId,
  position: LogIndexPosition,
  operation: AppliedOperation,
  digest: &'a [u8; 32],
  disposition: LogSearchMutationDisposition,
}

async fn record_work(
  transaction: &mut sqlx::Transaction<'_, Postgres>,
  work: AppliedWork<'_>,
) -> Result<(), LogSearchError> {
  let disposition = match work.disposition {
    LogSearchMutationDisposition::Applied => "applied",
    LogSearchMutationDisposition::Superseded => "superseded",
    LogSearchMutationDisposition::Replayed => return Err(LogSearchError::Unavailable),
  };
  let result = sqlx::query(
    "INSERT INTO log_search_applied_work \
       (work_id, project_id, position, operation, request_digest, disposition) VALUES ($1, $2, $3, $4, $5, $6)",
  )
  .bind(work.work_id.as_uuid())
  .bind(work.project_id.as_uuid())
  .bind(position_number(work.position)?)
  .bind(work.operation.as_str())
  .bind(work.digest.as_slice())
  .bind(disposition)
  .execute(&mut **transaction)
  .await;
  match result {
    Ok(_) => Ok(()),
    Err(error) if error.as_database_error().and_then(|error| error.code()).as_deref() == Some("23505") => {
      Err(LogSearchError::WorkConflict { work_id: work.work_id })
    }
    Err(error) => Err(unavailable(error)),
  }
}

async fn advance_freshness(
  transaction: &mut sqlx::Transaction<'_, Postgres>,
  project_id: ProjectId,
) -> Result<(), LogSearchError> {
  let current: i64 =
    sqlx::query_scalar("SELECT indexed_through FROM log_search_project_positions WHERE project_id = $1 FOR UPDATE")
      .bind(project_id.as_uuid())
      .fetch_one(&mut **transaction)
      .await
      .map_err(unavailable)?;
  let advanced: i64 = sqlx::query_scalar(
    "WITH ordered AS ( \
       SELECT position, ROW_NUMBER() OVER (ORDER BY position) AS ordinal \
       FROM log_search_applied_work WHERE project_id = $1 AND position > $2 \
     ), first_gap AS ( \
       SELECT $2 + ordinal - 1 AS through FROM ordered WHERE position <> $2 + ordinal ORDER BY position LIMIT 1 \
     ) \
     SELECT COALESCE((SELECT through FROM first_gap), \
       (SELECT MAX(position) FROM log_search_applied_work WHERE project_id = $1 AND position > $2), $2)",
  )
  .bind(project_id.as_uuid())
  .bind(current)
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if advanced > current {
    sqlx::query("UPDATE log_search_project_positions SET indexed_through = $1 WHERE project_id = $2")
      .bind(advanced)
      .bind(project_id.as_uuid())
      .execute(&mut **transaction)
      .await
      .map_err(unavailable)?;
  }
  Ok(())
}

async fn read_indexed_through<'e, E>(
  executor: E,
  project_id: ProjectId,
) -> Result<Option<LogIndexPosition>, LogSearchError>
where
  E: sqlx::Executor<'e, Database = Postgres>,
{
  let value: Option<i64> = sqlx::query_scalar(
    "SELECT indexed_through FROM log_search_project_positions WHERE project_id = $1 AND indexed_through > 0",
  )
  .bind(project_id.as_uuid())
  .fetch_optional(executor)
  .await
  .map_err(unavailable)?;
  value
    .map(|value| {
      u64::try_from(value)
        .ok()
        .and_then(|value| LogIndexPosition::new(value).ok())
        .ok_or(LogSearchError::Unavailable)
    })
    .transpose()
}

#[derive(FromRow)]
struct SearchRow {
  chunk_id: uuid::Uuid,
  build_id: uuid::Uuid,
  attempt_id: uuid::Uuid,
  job_id: uuid::Uuid,
  stream: String,
  first_sequence: i64,
  last_sequence: i64,
  occurred_at_millis: i64,
  normalized_text: String,
}

impl SearchRow {
  fn into_document(self, project_id: ProjectId) -> Result<LogSearchDocument, LogSearchError> {
    Ok(LogSearchDocument {
      chunk_id: LogChunkId::from_uuid(self.chunk_id).map_err(|_| LogSearchError::Unavailable)?,
      project_id,
      build_id: BuildId::from_uuid(self.build_id).map_err(|_| LogSearchError::Unavailable)?,
      attempt_id: AttemptId::from_uuid(self.attempt_id).map_err(|_| LogSearchError::Unavailable)?,
      job_id: JobId::from_uuid(self.job_id).map_err(|_| LogSearchError::Unavailable)?,
      stream: parse_stream(&self.stream)?,
      first_sequence: u64::try_from(self.first_sequence).map_err(|_| LogSearchError::Unavailable)?,
      last_sequence: u64::try_from(self.last_sequence).map_err(|_| LogSearchError::Unavailable)?,
      occurred_at: Timestamp::from_unix_millis(self.occurred_at_millis).map_err(|_| LogSearchError::Unavailable)?,
      redacted_text: self.normalized_text,
    })
  }

  fn into_hit(self, query: &LogSearchQuery) -> Result<LogSearchHit, LogSearchError> {
    let document = self.into_document(query.project_id)?;
    Ok(LogSearchHit {
      chunk_id: document.chunk_id,
      build_id: document.build_id,
      attempt_id: document.attempt_id,
      job_id: document.job_id,
      stream: document.stream,
      first_sequence: document.first_sequence,
      last_sequence: document.last_sequence,
      occurred_at: document.occurred_at,
      snippet: bounded_log_search_snippet(&document.redacted_text, &query.text, query.mode),
    })
  }
}

fn parse_stream(value: &str) -> Result<BuildLogStream, LogSearchError> {
  match value {
    "stdout" => Ok(BuildLogStream::Stdout),
    "stderr" => Ok(BuildLogStream::Stderr),
    _ => Err(LogSearchError::Unavailable),
  }
}

fn literal_pattern(value: &str) -> String {
  let mut pattern = String::with_capacity(value.len() + 2);
  pattern.push('%');
  for character in value.chars() {
    if matches!(character, '\\' | '%' | '_') {
      pattern.push('\\');
    }
    pattern.push(character);
  }
  pattern.push('%');
  pattern
}

fn document_digest(request: &WriteLogSearchDocument) -> [u8; 32] {
  let document = &request.document;
  let mut digest = Sha256::new();
  digest.update(b"octacity.log-search-document.v1\0");
  digest.update(request.position.get().to_be_bytes());
  for id in [
    document.chunk_id.as_uuid(),
    document.project_id.as_uuid(),
    document.build_id.as_uuid(),
    document.attempt_id.as_uuid(),
    document.job_id.as_uuid(),
  ] {
    digest.update(id.as_bytes());
  }
  digest.update(document.stream.as_str().as_bytes());
  digest.update(document.first_sequence.to_be_bytes());
  digest.update(document.last_sequence.to_be_bytes());
  digest.update(document.occurred_at.unix_millis().to_be_bytes());
  update_bytes(&mut digest, document.redacted_text.as_bytes());
  digest.finalize().into()
}

fn delete_digest(request: &DeleteLogSearchDocuments) -> [u8; 32] {
  let mut digest = Sha256::new();
  digest.update(b"octacity.log-search-work.v1\0delete_build");
  digest.update(request.position.get().to_be_bytes());
  digest.update(request.project_id.as_uuid().as_bytes());
  digest.update(request.build_id.as_uuid().as_bytes());
  digest.finalize().into()
}

fn update_bytes(digest: &mut Sha256, value: &[u8]) {
  digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
  digest.update(value);
}

fn position_number(position: LogIndexPosition) -> Result<i64, LogSearchError> {
  i64::try_from(position.get()).map_err(|_| LogSearchError::Unavailable)
}

fn sequence_number(sequence: u64) -> Result<i64, LogSearchError> {
  i64::try_from(sequence).map_err(|_| LogSearchError::Unavailable)
}

fn unavailable(_: sqlx::Error) -> LogSearchError {
  LogSearchError::Unavailable
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn literal_patterns_escape_sql_wildcards() {
    assert_eq!(literal_pattern(r"100%_done\path"), r"%100\%\_done\\path%");
  }
}
