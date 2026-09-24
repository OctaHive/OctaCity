use octacity_server_domain::{JobId, LogChunkId, Timestamp};
use octacity_server_store::{
  BuildLogStream, ClaimOrphanLogChunks, CompleteOrphanLogChunk, FailOrphanLogChunk, LogChunkDigest, LogChunkManifest,
  OrphanLogChunkClaim, StageOrphanLogChunk, StoreError, StoredLogChunkManifest, WorkerOwner,
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::database::unavailable;

use super::{FailureReplayRow, conflict};

pub(crate) async fn stage(pool: &PgPool, request: StageOrphanLogChunk) -> Result<(), StoreError> {
  let manifest = &request.manifest;
  if restore_manifest(StoredLogChunkManifest {
    chunk_id: manifest.chunk_id(),
    job_id: request.job_id,
    stream: manifest.stream(),
    first_sequence: manifest.first_sequence(),
    last_sequence: manifest.last_sequence(),
    byte_length: manifest.byte_length(),
    digest: manifest.digest(),
    object_identity: manifest.object_identity().to_owned(),
  })
  .map_err(|_| invalid_stage())?
    != *manifest
  {
    return Err(invalid_stage());
  }
  let inserted = sqlx::query(
    "INSERT INTO orphan_log_chunk_work \
       (chunk_id, job_id, stream, first_sequence, last_sequence, object_identity, byte_length, sha256, available_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9::double precision / 1000.0)) \
     ON CONFLICT (chunk_id) DO NOTHING",
  )
  .bind(manifest.chunk_id().as_uuid())
  .bind(request.job_id.as_uuid())
  .bind(manifest.stream().as_str())
  .bind(i64::try_from(manifest.first_sequence()).map_err(|_| StoreError::Unavailable)?)
  .bind(i64::try_from(manifest.last_sequence()).map_err(|_| StoreError::Unavailable)?)
  .bind(manifest.object_identity())
  .bind(i64::try_from(manifest.byte_length()).map_err(|_| StoreError::Unavailable)?)
  .bind(manifest.digest().as_bytes().to_vec())
  .bind(request.cleanup_after.unix_millis())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if inserted.rows_affected() == 1 {
    return Ok(());
  }
  let existing = sqlx::query_as::<_, OrphanRow>(
    "SELECT chunk_id, job_id, stream, first_sequence, last_sequence, object_identity, byte_length, sha256, \
       attempt_count, claim_owner FROM orphan_log_chunk_work WHERE chunk_id = $1",
  )
  .bind(manifest.chunk_id().as_uuid())
  .fetch_one(pool)
  .await
  .map_err(unavailable)?;
  if existing.manifest()? == *manifest {
    Ok(())
  } else {
    Err(conflict())
  }
}

pub(crate) async fn claim(
  pool: &PgPool,
  request: ClaimOrphanLogChunks,
) -> Result<Vec<OrphanLogChunkClaim>, StoreError> {
  request.validate()?;
  let rows = sqlx::query_as::<_, OrphanRow>(
    "WITH candidates AS ( \
       SELECT chunk_id FROM orphan_log_chunk_work \
       WHERE state IN ('pending', 'retry_scheduled', 'claimed') \
         AND available_at <= to_timestamp($1::double precision / 1000.0) \
         AND (state <> 'claimed' OR claim_expires_at <= to_timestamp($1::double precision / 1000.0)) \
       ORDER BY available_at, chunk_id FOR UPDATE SKIP LOCKED LIMIT $2 \
     ), claimed AS ( \
       UPDATE orphan_log_chunk_work AS work SET state = 'claimed', claim_owner = $3, \
         claim_expires_at = to_timestamp($4::double precision / 1000.0), attempt_count = attempt_count + 1 \
       FROM candidates WHERE work.chunk_id = candidates.chunk_id RETURNING work.* \
     ) SELECT chunk_id, job_id, stream, first_sequence, last_sequence, object_identity, byte_length, sha256, \
       attempt_count, claim_owner FROM claimed ORDER BY chunk_id",
  )
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  rows
    .into_iter()
    .map(|row| {
      Ok(OrphanLogChunkClaim {
        manifest: row.manifest()?,
        attempt: u16::try_from(row.attempt_count).map_err(|_| StoreError::Unavailable)?,
        owner: WorkerOwner::new(row.claim_owner.ok_or(StoreError::Unavailable)?)?,
      })
    })
    .collect()
}

pub(crate) async fn complete(pool: &PgPool, request: CompleteOrphanLogChunk) -> Result<(), StoreError> {
  let updated = sqlx::query(
    "UPDATE orphan_log_chunk_work SET state = 'completed', completed_at = to_timestamp($1::double precision / 1000.0), \
       last_claim_owner = claim_owner, claim_owner = NULL, claim_expires_at = NULL \
     WHERE chunk_id = $2 AND state = 'claimed' AND claim_owner = $3 \
       AND claim_expires_at > to_timestamp($1::double precision / 1000.0)",
  )
  .bind(request.completed_at.unix_millis())
  .bind(request.chunk_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() == 1 {
    return Ok(());
  }
  let replay: Option<(String, Option<String>, Option<i64>)> = sqlx::query_as(
    "SELECT state, last_claim_owner, FLOOR(EXTRACT(EPOCH FROM completed_at) * 1000)::BIGINT \
     FROM orphan_log_chunk_work WHERE chunk_id = $1",
  )
  .bind(request.chunk_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  if replay.is_some_and(|(state, owner, completed_at)| {
    state == "completed"
      && owner.as_deref() == Some(request.owner.as_str())
      && completed_at == Some(request.completed_at.unix_millis())
  }) {
    Ok(())
  } else {
    Err(conflict())
  }
}

pub(crate) async fn fail(pool: &PgPool, request: FailOrphanLogChunk) -> Result<(), StoreError> {
  if request.error_code.is_empty()
    || request.error_code.len() > octacity_server_store::MAX_RETENTION_FAILURE_CODE_BYTES
    || request.error_code.chars().any(char::is_whitespace)
    || request.retry_at.is_some_and(|retry_at| retry_at <= request.failed_at)
  {
    return Err(StoreError::InvalidInput {
      operation: octacity_server_store::StoreOperation::FailOrphanLogChunk,
      source: octacity_server_store::StoreInputError::InvalidWorkerClaim,
    });
  }
  let state = if request.retry_at.is_some() {
    "retry_scheduled"
  } else {
    "dead_letter"
  };
  let updated = sqlx::query(
    "UPDATE orphan_log_chunk_work SET state = $1, \
       available_at = COALESCE(to_timestamp($2::double precision / 1000.0), available_at), \
       last_claim_owner = claim_owner, last_error_code = $3, \
       last_failure_at = to_timestamp($4::double precision / 1000.0), \
       last_retry_at = to_timestamp($2::double precision / 1000.0), claim_owner = NULL, claim_expires_at = NULL \
     WHERE chunk_id = $5 AND state = 'claimed' AND claim_owner = $6 \
       AND claim_expires_at > to_timestamp($4::double precision / 1000.0)",
  )
  .bind(state)
  .bind(request.retry_at.map(Timestamp::unix_millis))
  .bind(&request.error_code)
  .bind(request.failed_at.unix_millis())
  .bind(request.chunk_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() == 1 {
    return Ok(());
  }
  let replay = sqlx::query_as::<_, FailureReplayRow>(
    "SELECT state, last_claim_owner AS owner, last_error_code AS error_code, \
       FLOOR(EXTRACT(EPOCH FROM last_failure_at) * 1000)::BIGINT AS failed_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM last_retry_at) * 1000)::BIGINT AS retry_at_millis \
     FROM orphan_log_chunk_work WHERE chunk_id = $1",
  )
  .bind(request.chunk_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  if replay.is_some_and(|row| {
    row.matches(
      Some(state),
      &request.owner,
      &request.error_code,
      request.failed_at,
      request.retry_at,
    )
  }) {
    Ok(())
  } else {
    Err(conflict())
  }
}

#[derive(FromRow)]
struct OrphanRow {
  chunk_id: Uuid,
  job_id: Uuid,
  stream: String,
  first_sequence: i64,
  last_sequence: i64,
  object_identity: String,
  byte_length: i64,
  sha256: Vec<u8>,
  attempt_count: i64,
  claim_owner: Option<String>,
}

impl OrphanRow {
  fn manifest(&self) -> Result<LogChunkManifest, StoreError> {
    let stream = match self.stream.as_str() {
      "stdout" => BuildLogStream::Stdout,
      "stderr" => BuildLogStream::Stderr,
      _ => return Err(StoreError::Unavailable),
    };
    let digest: [u8; 32] = self.sha256.as_slice().try_into().map_err(|_| StoreError::Unavailable)?;
    restore_manifest(StoredLogChunkManifest {
      chunk_id: LogChunkId::from_uuid(self.chunk_id).map_err(|_| StoreError::Unavailable)?,
      job_id: JobId::from_uuid(self.job_id).map_err(|_| StoreError::Unavailable)?,
      stream,
      first_sequence: u64::try_from(self.first_sequence).map_err(|_| StoreError::Unavailable)?,
      last_sequence: u64::try_from(self.last_sequence).map_err(|_| StoreError::Unavailable)?,
      byte_length: u64::try_from(self.byte_length).map_err(|_| StoreError::Unavailable)?,
      digest: LogChunkDigest::from_bytes(digest),
      object_identity: self.object_identity.clone(),
    })
  }
}

fn restore_manifest(stored: StoredLogChunkManifest) -> Result<LogChunkManifest, StoreError> {
  LogChunkManifest::restore(stored).map_err(|_| StoreError::Unavailable)
}

const fn invalid_stage() -> StoreError {
  StoreError::InvalidInput {
    operation: octacity_server_store::StoreOperation::StageOrphanLogChunk,
    source: octacity_server_store::StoreInputError::InvalidLogChunkManifest,
  }
}
