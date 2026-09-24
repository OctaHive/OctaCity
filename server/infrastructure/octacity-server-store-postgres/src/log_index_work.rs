use async_trait::async_trait;
use octacity_server_domain::{
  AttemptId, BuildId, EntityKind, JobId, LogChunkId, LogIndexingWorkId, ProjectId, Timestamp,
};
use octacity_server_store::{
  BuildLogStream, ClaimLogIndexWork, CompleteLogIndexWork, FailLogIndexWork, LogChunkDigest, LogChunkManifest,
  LogIndexDocumentSource, LogIndexPosition, LogIndexWorkClaim, LogIndexWorkKind, LogIndexWorkQueue, LogIndexWorkStore,
  MutationDisposition, StoreError, StoredLogChunkManifest, WorkerOwner,
};
use sqlx::{FromRow, PgPool};

use crate::{database::unavailable, store::PostgresStore};

#[async_trait]
impl LogIndexWorkStore for PostgresStore {
  async fn committed_log_index_position(&self, project_id: ProjectId) -> Result<Option<LogIndexPosition>, StoreError> {
    let position: Option<i64> =
      sqlx::query_scalar("SELECT committed_through FROM log_index_project_positions WHERE project_id = $1")
        .bind(project_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
    position
      .map(|position| {
        u64::try_from(position)
          .ok()
          .and_then(|position| LogIndexPosition::new(position).ok())
          .ok_or(StoreError::Unavailable)
      })
      .transpose()
  }
}

#[async_trait]
impl LogIndexWorkQueue for PostgresStore {
  async fn claim_log_index_work(&self, request: ClaimLogIndexWork) -> Result<Vec<LogIndexWorkClaim>, StoreError> {
    claim(self.pool(), request).await
  }

  async fn complete_log_index_work(&self, request: CompleteLogIndexWork) -> Result<MutationDisposition, StoreError> {
    complete(self.pool(), request).await
  }

  async fn fail_log_index_work(&self, request: FailLogIndexWork) -> Result<MutationDisposition, StoreError> {
    fail(self.pool(), request).await
  }
}

pub(crate) async fn claim(pool: &PgPool, request: ClaimLogIndexWork) -> Result<Vec<LogIndexWorkClaim>, StoreError> {
  request.validate()?;
  let rows = sqlx::query_as::<_, WorkRow>(
    "WITH candidates AS ( \
       SELECT id FROM log_indexing_work \
       WHERE state IN ('pending', 'retry_scheduled', 'claimed') \
         AND available_at <= to_timestamp($1::double precision / 1000.0) \
         AND (state <> 'claimed' OR claim_expires_at <= to_timestamp($1::double precision / 1000.0)) \
       ORDER BY project_id, position, id FOR UPDATE SKIP LOCKED LIMIT $2 \
     ), claimed AS ( \
       UPDATE log_indexing_work AS work SET state = 'claimed', claim_owner = $3, \
         claim_expires_at = to_timestamp($4::double precision / 1000.0), attempt_count = attempt_count + 1 \
       FROM candidates WHERE work.id = candidates.id RETURNING work.* \
     ) \
     SELECT claimed.id, claimed.project_id, claimed.position, claimed.build_id, claimed.operation, \
       claimed.attempt_count, claimed.claim_owner, \
       FLOOR(EXTRACT(EPOCH FROM claimed.claim_expires_at) * 1000)::BIGINT AS claim_expires_at_millis, \
       manifest.id AS chunk_id, manifest.attempt_id, manifest.job_id, manifest.stream, manifest.first_sequence, \
       manifest.last_sequence, manifest.object_identity, manifest.byte_length, manifest.sha256, \
       FLOOR(EXTRACT(EPOCH FROM manifest.created_at) * 1000)::BIGINT AS occurred_at_millis \
     FROM claimed LEFT JOIN log_chunk_manifests AS manifest ON manifest.id = claimed.chunk_id \
     ORDER BY claimed.project_id, claimed.position, claimed.id",
  )
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  rows.into_iter().map(WorkRow::into_claim).collect()
}

pub(crate) async fn complete(pool: &PgPool, request: CompleteLogIndexWork) -> Result<MutationDisposition, StoreError> {
  let updated = sqlx::query(
    "UPDATE log_indexing_work SET state = 'completed', completed_at = to_timestamp($1::double precision / 1000.0), \
       last_claim_owner = claim_owner, claim_owner = NULL, claim_expires_at = NULL \
     WHERE id = $2 AND state = 'claimed' AND claim_owner = $3 \
       AND claim_expires_at > to_timestamp($1::double precision / 1000.0)",
  )
  .bind(request.completed_at.unix_millis())
  .bind(request.work_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() == 1 {
    return Ok(MutationDisposition::Applied);
  }
  let replay: Option<(String, Option<String>, Option<i64>)> = sqlx::query_as(
    "SELECT state, last_claim_owner, \
       FLOOR(EXTRACT(EPOCH FROM completed_at) * 1000)::BIGINT FROM log_indexing_work WHERE id = $1",
  )
  .bind(request.work_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  match replay {
    Some((state, owner, completed_at))
      if state == "completed"
        && owner.as_deref() == Some(request.owner.as_str())
        && completed_at == Some(request.completed_at.unix_millis()) =>
    {
      Ok(MutationDisposition::Replayed)
    }
    Some(_) => Err(StoreError::Conflict {
      entity: EntityKind::LogIndexingWork,
    }),
    None => Err(StoreError::NotFound {
      entity: EntityKind::LogIndexingWork,
    }),
  }
}

pub(crate) async fn fail(pool: &PgPool, request: FailLogIndexWork) -> Result<MutationDisposition, StoreError> {
  request.validate()?;
  let state = if request.retry_at.is_some() {
    "retry_scheduled"
  } else {
    "dead_letter"
  };
  let updated = sqlx::query(
    "UPDATE log_indexing_work SET state = $1, available_at = COALESCE( \
       to_timestamp($2::double precision / 1000.0), available_at), last_error_code = $3, \
       last_claim_owner = claim_owner, last_failure_at = to_timestamp($4::double precision / 1000.0), \
       last_retry_at = to_timestamp($2::double precision / 1000.0), claim_owner = NULL, claim_expires_at = NULL \
     WHERE id = $5 AND state = 'claimed' AND claim_owner = $6 \
       AND claim_expires_at > to_timestamp($4::double precision / 1000.0)",
  )
  .bind(state)
  .bind(request.retry_at.map(Timestamp::unix_millis))
  .bind(&request.error_code)
  .bind(request.failed_at.unix_millis())
  .bind(request.work_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() == 1 {
    return Ok(MutationDisposition::Applied);
  }
  let replay = sqlx::query_as::<_, FailureReplay>(
    "SELECT state, last_claim_owner, last_error_code, \
       FLOOR(EXTRACT(EPOCH FROM last_failure_at) * 1000)::BIGINT AS failed_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM last_retry_at) * 1000)::BIGINT AS retry_at_millis \
     FROM log_indexing_work WHERE id = $1",
  )
  .bind(request.work_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  match replay {
    Some(replay)
      if replay.state == state
        && replay.last_claim_owner.as_deref() == Some(request.owner.as_str())
        && replay.last_error_code.as_deref() == Some(request.error_code.as_str())
        && replay.failed_at_millis == Some(request.failed_at.unix_millis())
        && replay.retry_at_millis == request.retry_at.map(Timestamp::unix_millis) =>
    {
      Ok(MutationDisposition::Replayed)
    }
    Some(_) => Err(StoreError::Conflict {
      entity: EntityKind::LogIndexingWork,
    }),
    None => Err(StoreError::NotFound {
      entity: EntityKind::LogIndexingWork,
    }),
  }
}

#[derive(FromRow)]
struct FailureReplay {
  state: String,
  last_claim_owner: Option<String>,
  last_error_code: Option<String>,
  failed_at_millis: Option<i64>,
  retry_at_millis: Option<i64>,
}

#[derive(FromRow)]
struct WorkRow {
  id: uuid::Uuid,
  project_id: uuid::Uuid,
  position: i64,
  build_id: uuid::Uuid,
  operation: String,
  attempt_count: i64,
  claim_owner: Option<String>,
  claim_expires_at_millis: Option<i64>,
  chunk_id: Option<uuid::Uuid>,
  attempt_id: Option<uuid::Uuid>,
  job_id: Option<uuid::Uuid>,
  stream: Option<String>,
  first_sequence: Option<i64>,
  last_sequence: Option<i64>,
  object_identity: Option<String>,
  byte_length: Option<i64>,
  sha256: Option<Vec<u8>>,
  occurred_at_millis: Option<i64>,
}

impl WorkRow {
  fn into_claim(self) -> Result<LogIndexWorkClaim, StoreError> {
    let work_id = LogIndexingWorkId::from_uuid(self.id).map_err(|_| StoreError::Unavailable)?;
    let project_id = ProjectId::from_uuid(self.project_id).map_err(|_| StoreError::Unavailable)?;
    let build_id = BuildId::from_uuid(self.build_id).map_err(|_| StoreError::Unavailable)?;
    let position = u64::try_from(self.position)
      .ok()
      .and_then(|value| LogIndexPosition::new(value).ok())
      .ok_or(StoreError::Unavailable)?;
    let attempt = u16::try_from(self.attempt_count).map_err(|_| StoreError::Unavailable)?;
    let kind = match self.operation.as_str() {
      "index" => LogIndexWorkKind::Index(self.document_source()?),
      "rebuild" => LogIndexWorkKind::Rebuild(self.document_source()?),
      "delete_build" if self.chunk_id.is_none() => LogIndexWorkKind::DeleteBuild,
      _ => return Err(StoreError::Unavailable),
    };
    let owner = WorkerOwner::new(self.claim_owner.ok_or(StoreError::Unavailable)?)?;
    let claim_expires_at = timestamp(self.claim_expires_at_millis)?;
    Ok(LogIndexWorkClaim {
      work_id,
      position,
      project_id,
      build_id,
      kind,
      attempt,
      owner,
      claim_expires_at,
    })
  }

  fn document_source(&self) -> Result<LogIndexDocumentSource, StoreError> {
    let chunk_id =
      LogChunkId::from_uuid(self.chunk_id.ok_or(StoreError::Unavailable)?).map_err(|_| StoreError::Unavailable)?;
    let job_id = JobId::from_uuid(self.job_id.ok_or(StoreError::Unavailable)?).map_err(|_| StoreError::Unavailable)?;
    let stream = match self.stream.as_deref() {
      Some("stdout") => BuildLogStream::Stdout,
      Some("stderr") => BuildLogStream::Stderr,
      _ => return Err(StoreError::Unavailable),
    };
    let digest: [u8; 32] = self
      .sha256
      .as_deref()
      .ok_or(StoreError::Unavailable)?
      .try_into()
      .map_err(|_| StoreError::Unavailable)?;
    let manifest = LogChunkManifest::restore(StoredLogChunkManifest {
      chunk_id,
      job_id,
      stream,
      first_sequence: unsigned(self.first_sequence)?,
      last_sequence: unsigned(self.last_sequence)?,
      byte_length: unsigned(self.byte_length)?,
      digest: LogChunkDigest::from_bytes(digest),
      object_identity: self.object_identity.clone().ok_or(StoreError::Unavailable)?,
    })
    .map_err(|_| StoreError::Unavailable)?;
    Ok(LogIndexDocumentSource {
      manifest,
      attempt_id: AttemptId::from_uuid(self.attempt_id.ok_or(StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
      job_id,
      occurred_at: timestamp(self.occurred_at_millis)?,
    })
  }
}

fn unsigned(value: Option<i64>) -> Result<u64, StoreError> {
  u64::try_from(value.ok_or(StoreError::Unavailable)?).map_err(|_| StoreError::Unavailable)
}

fn timestamp(value: Option<i64>) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value.ok_or(StoreError::Unavailable)?).map_err(|_| StoreError::Unavailable)
}
