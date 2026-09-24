use octacity_server_domain::{
  BuildId, EntityKind, JobId, LogChunkId, LogIndexingWorkId, ProjectId, RetentionWorkId, Timestamp,
};
use octacity_server_store::{
  BuildLogStream, BuildResultComponent, ClaimRetentionWork, CompleteRetentionObject, CompleteRetentionSearch,
  DeleteLogSearchDocuments, FailRetentionWork, FinishRetentionPass, LogChunkDigest, LogChunkManifest, LogIndexPosition,
  PrepareRetentionWork, RetentionObject, RetentionObjectIdentity, RetentionPassOutcome, RetentionPhase,
  RetentionPreparation, RetentionWorkClaim, StoreError, StoredLogChunkManifest, WorkerOwner,
};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{artifact, database::unavailable};

mod orphan;

pub(crate) use orphan::{
  claim as claim_orphans, complete as complete_orphan, fail as fail_orphan, stage as stage_orphan,
};

#[derive(FromRow)]
struct FailureReplayRow {
  state: String,
  owner: Option<String>,
  error_code: Option<String>,
  failed_at_millis: Option<i64>,
  retry_at_millis: Option<i64>,
}

impl FailureReplayRow {
  fn matches(
    &self,
    expected_state: Option<&str>,
    owner: &WorkerOwner,
    error_code: &str,
    failed_at: Timestamp,
    retry_at: Option<Timestamp>,
  ) -> bool {
    expected_state.is_none_or(|expected| self.state == expected)
      && self.owner.as_deref() == Some(owner.as_str())
      && self.error_code.as_deref() == Some(error_code)
      && self.failed_at_millis == Some(failed_at.unix_millis())
      && self.retry_at_millis == retry_at.map(Timestamp::unix_millis)
  }
}

#[derive(FromRow)]
struct ClaimRow {
  id: Uuid,
  project_id: Uuid,
  build_id: Uuid,
  resource_kind: String,
  phase: String,
  deadline_millis: i64,
  attempt_count: i64,
  claim_owner: Option<String>,
  claim_expires_at_millis: Option<i64>,
}

impl ClaimRow {
  fn into_claim(self) -> Result<RetentionWorkClaim, StoreError> {
    Ok(RetentionWorkClaim {
      work_id: RetentionWorkId::from_uuid(self.id).map_err(|_| StoreError::Unavailable)?,
      project_id: ProjectId::from_uuid(self.project_id).map_err(|_| StoreError::Unavailable)?,
      build_id: BuildId::from_uuid(self.build_id).map_err(|_| StoreError::Unavailable)?,
      component: component(&self.resource_kind)?,
      deadline: timestamp(self.deadline_millis)?,
      phase: phase(&self.phase)?,
      attempt: u16::try_from(self.attempt_count).map_err(|_| StoreError::Unavailable)?,
      owner: WorkerOwner::new(self.claim_owner.ok_or(StoreError::Unavailable)?)?,
      claim_expires_at: timestamp(self.claim_expires_at_millis.ok_or(StoreError::Unavailable)?)?,
    })
  }
}

pub(crate) async fn claim(pool: &PgPool, request: ClaimRetentionWork) -> Result<Vec<RetentionWorkClaim>, StoreError> {
  request.validate()?;
  sqlx::query_as::<_, ClaimRow>(
    "WITH candidates AS (\
       SELECT id FROM retention_work WHERE build_id IS NOT NULL AND completed_at IS NULL \
         AND available_at <= to_timestamp($1::double precision / 1000.0) \
         AND (claim_owner IS NULL OR claim_expires_at <= to_timestamp($1::double precision / 1000.0)) \
       ORDER BY available_at, id FOR UPDATE SKIP LOCKED LIMIT $2\
     ), claimed AS (\
       UPDATE retention_work AS work SET claim_owner = $3, \
         claim_expires_at = to_timestamp($4::double precision / 1000.0), attempt_count = attempt_count + 1 \
       FROM candidates WHERE work.id = candidates.id \
       RETURNING work.id, work.project_id, work.build_id, work.resource_kind, work.phase, work.deadline_at, \
         work.attempt_count, work.claim_owner, work.claim_expires_at\
     ) SELECT id, project_id, build_id, resource_kind, phase, \
       FLOOR(EXTRACT(EPOCH FROM deadline_at) * 1000)::BIGINT AS deadline_millis, attempt_count, claim_owner, \
       FLOOR(EXTRACT(EPOCH FROM claim_expires_at) * 1000)::BIGINT AS claim_expires_at_millis \
     FROM claimed ORDER BY deadline_millis, id",
  )
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(ClaimRow::into_claim)
  .collect()
}

pub(crate) async fn prepare(pool: &PgPool, request: PrepareRetentionWork) -> Result<RetentionPreparation, StoreError> {
  request.validate()?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let mut work = lock_work(&mut transaction, request.work_id, &request.owner, request.observed_at).await?;
  if phase(&work.phase)? == RetentionPhase::Pending {
    hide_component(&mut transaction, &mut work, request.observed_at).await?;
  }
  let current_phase = phase(&work.phase)?;
  let search_deletion = if work.component == BuildResultComponent::Logs && current_phase == RetentionPhase::Hidden {
    Some(DeleteLogSearchDocuments {
      work_id: LogIndexingWorkId::from_uuid(work.search_work_id.ok_or(StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
      position: log_position(work.search_position)?,
      project_id: work.project_id,
      build_id: work.build_id,
    })
  } else {
    None
  };
  let objects = if work.component == BuildResultComponent::Logs && current_phase != RetentionPhase::SearchDeleted {
    Vec::new()
  } else {
    retention_objects(&mut transaction, &work, request.object_limit.get(), request.observed_at).await?
  };
  transaction.commit().await.map_err(unavailable)?;
  Ok(RetentionPreparation {
    search_deletion,
    objects,
  })
}

pub(crate) async fn complete_search(pool: &PgPool, request: CompleteRetentionSearch) -> Result<(), StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let work = lock_work(&mut transaction, request.work_id, &request.owner, request.completed_at).await?;
  if work.component != BuildResultComponent::Logs {
    return Err(conflict());
  }
  if phase(&work.phase)? == RetentionPhase::Hidden {
    let search_work_id = work.search_work_id.ok_or(StoreError::Unavailable)?;
    let search_position = work.search_position.ok_or(StoreError::Unavailable)?;
    sqlx::query(
      "UPDATE log_indexing_work SET state = 'completed', \
       completed_at = COALESCE(completed_at, to_timestamp($1::double precision / 1000.0)), \
       last_claim_owner = COALESCE(claim_owner, last_claim_owner), claim_owner = NULL, claim_expires_at = NULL \
       WHERE project_id = $2 AND build_id = $3 AND position < $4",
    )
    .bind(request.completed_at.unix_millis())
    .bind(work.project_id.as_uuid())
    .bind(work.build_id.as_uuid())
    .bind(search_position)
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    sqlx::query(
      "UPDATE log_indexing_work SET state = 'completed', completed_at = to_timestamp($1::double precision / 1000.0), \
       last_claim_owner = COALESCE(claim_owner, last_claim_owner), claim_owner = NULL, claim_expires_at = NULL \
       WHERE id = $2",
    )
    .bind(request.completed_at.unix_millis())
    .bind(search_work_id)
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    sqlx::query("UPDATE retention_work SET phase = 'search_deleted' WHERE id = $1")
      .bind(request.work_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
  }
  transaction.commit().await.map_err(unavailable)
}

pub(crate) async fn complete_object(pool: &PgPool, request: CompleteRetentionObject) -> Result<(), StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let work = lock_work(&mut transaction, request.work_id, &request.owner, request.completed_at).await?;
  match (work.component, request.object) {
    (BuildResultComponent::Logs, RetentionObjectIdentity::Log(chunk_id)) => {
      let updated = sqlx::query(
        "UPDATE log_chunk_manifests SET deleted_at = to_timestamp($1::double precision / 1000.0) \
         WHERE id = $2 AND build_id = $3 AND NOT visible AND deleted_at IS NULL",
      )
      .bind(request.completed_at.unix_millis())
      .bind(chunk_id.as_uuid())
      .bind(work.build_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
      if updated.rows_affected() == 0 && !log_is_deleted(&mut transaction, chunk_id, work.build_id).await? {
        return Err(conflict());
      }
    }
    (BuildResultComponent::Artifacts | BuildResultComponent::Reports, RetentionObjectIdentity::Output(artifact_id)) => {
      artifact::complete_retention(&mut transaction, artifact_id, request.completed_at).await?;
    }
    _ => return Err(conflict()),
  }
  transaction.commit().await.map_err(unavailable)
}

pub(crate) async fn finish_pass(
  pool: &PgPool,
  request: FinishRetentionPass,
) -> Result<RetentionPassOutcome, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let work = lock_work(&mut transaction, request.work_id, &request.owner, request.finished_at).await?;
  let search_ready =
    work.component != BuildResultComponent::Logs || phase(&work.phase)? == RetentionPhase::SearchDeleted;
  let remaining = remaining_objects(&mut transaction, &work).await?;
  let outcome = if search_ready && !remaining {
    sqlx::query(
      "UPDATE retention_work SET phase = 'bytes_deleted', completed_at = to_timestamp($1::double precision / 1000.0), \
       claim_owner = NULL, claim_expires_at = NULL WHERE id = $2",
    )
    .bind(request.finished_at.unix_millis())
    .bind(request.work_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    RetentionPassOutcome::Completed
  } else {
    sqlx::query(
      "UPDATE retention_work SET available_at = to_timestamp($1::double precision / 1000.0), \
       claim_owner = NULL, claim_expires_at = NULL WHERE id = $2",
    )
    .bind(request.finished_at.unix_millis())
    .bind(request.work_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    RetentionPassOutcome::Pending
  };
  transaction.commit().await.map_err(unavailable)?;
  Ok(outcome)
}

pub(crate) async fn fail(pool: &PgPool, request: FailRetentionWork) -> Result<(), StoreError> {
  if request.error_code.is_empty()
    || request.error_code.len() > octacity_server_store::MAX_RETENTION_FAILURE_CODE_BYTES
    || request.error_code.chars().any(char::is_whitespace)
    || request.retry_at.is_some_and(|retry_at| retry_at <= request.failed_at)
  {
    return Err(StoreError::InvalidInput {
      operation: octacity_server_store::StoreOperation::FailRetentionWork,
      source: octacity_server_store::StoreInputError::InvalidWorkerClaim,
    });
  }
  let updated = sqlx::query(
    "UPDATE retention_work SET \
       available_at = COALESCE(to_timestamp($1::double precision / 1000.0), available_at), \
       phase = CASE WHEN $1::BIGINT IS NULL THEN 'dead_letter' ELSE phase END, \
       completed_at = CASE WHEN $1::BIGINT IS NULL THEN to_timestamp($2::double precision / 1000.0) ELSE NULL END, \
       last_error_code = $3, last_failure_at = to_timestamp($2::double precision / 1000.0), \
       last_claim_owner = claim_owner, last_retry_at = to_timestamp($1::double precision / 1000.0), \
       claim_owner = NULL, claim_expires_at = NULL WHERE id = $4 AND claim_owner = $5 \
       AND claim_expires_at > to_timestamp($2::double precision / 1000.0)",
  )
  .bind(request.retry_at.map(Timestamp::unix_millis))
  .bind(request.failed_at.unix_millis())
  .bind(&request.error_code)
  .bind(request.work_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() == 1 {
    return Ok(());
  }
  let replay = sqlx::query_as::<_, FailureReplayRow>(
    "SELECT phase AS state, last_claim_owner AS owner, last_error_code AS error_code, \
       FLOOR(EXTRACT(EPOCH FROM last_failure_at) * 1000)::BIGINT AS failed_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM last_retry_at) * 1000)::BIGINT AS retry_at_millis \
     FROM retention_work WHERE id = $1",
  )
  .bind(request.work_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  let expected_phase = if request.retry_at.is_some() {
    None
  } else {
    Some("dead_letter")
  };
  if replay.is_some_and(|row| {
    row.matches(
      expected_phase,
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
struct LockedWorkRow {
  project_id: Uuid,
  build_id: Uuid,
  resource_kind: String,
  phase: String,
  search_work_id: Option<Uuid>,
  search_position: Option<i64>,
}

struct LockedWork {
  work_id: RetentionWorkId,
  project_id: ProjectId,
  build_id: BuildId,
  component: BuildResultComponent,
  phase: String,
  search_work_id: Option<Uuid>,
  search_position: Option<i64>,
}

async fn lock_work(
  transaction: &mut Transaction<'_, Postgres>,
  work_id: RetentionWorkId,
  owner: &WorkerOwner,
  observed_at: Timestamp,
) -> Result<LockedWork, StoreError> {
  let row = sqlx::query_as::<_, LockedWorkRow>(
    "SELECT project_id, build_id, resource_kind, phase, search_work_id, search_position \
     FROM retention_work WHERE id = $1 AND claim_owner = $2 AND completed_at IS NULL \
       AND claim_expires_at > to_timestamp($3::double precision / 1000.0) FOR UPDATE",
  )
  .bind(work_id.as_uuid())
  .bind(owner.as_str())
  .bind(observed_at.unix_millis())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .ok_or_else(conflict)?;
  Ok(LockedWork {
    work_id,
    project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
    build_id: BuildId::from_uuid(row.build_id).map_err(|_| StoreError::Unavailable)?,
    component: component(&row.resource_kind)?,
    phase: row.phase,
    search_work_id: row.search_work_id,
    search_position: row.search_position,
  })
}

async fn hide_component(
  transaction: &mut Transaction<'_, Postgres>,
  work: &mut LockedWork,
  observed_at: Timestamp,
) -> Result<(), StoreError> {
  match work.component {
    BuildResultComponent::Metadata => {
      sqlx::query(
        "UPDATE builds SET metadata_visible = false, metadata_deleted_at = COALESCE(metadata_deleted_at, \
         to_timestamp($1::double precision / 1000.0)) WHERE id = $2",
      )
      .bind(observed_at.unix_millis())
      .bind(work.build_id.as_uuid())
      .execute(&mut **transaction)
      .await
      .map_err(unavailable)?;
    }
    BuildResultComponent::Logs => {
      hide_build_component(transaction, work.build_id, BuildResultComponent::Logs, observed_at).await?;
      sqlx::query(
        "UPDATE log_chunk_manifests SET visible = false, hidden_at = COALESCE(hidden_at, \
         to_timestamp($1::double precision / 1000.0)) WHERE build_id = $2 AND visible",
      )
      .bind(observed_at.unix_millis())
      .bind(work.build_id.as_uuid())
      .execute(&mut **transaction)
      .await
      .map_err(unavailable)?;
      allocate_search_deletion(transaction, work, observed_at).await?;
    }
    BuildResultComponent::Artifacts | BuildResultComponent::Reports => {
      hide_build_component(transaction, work.build_id, work.component, observed_at).await?;
    }
  }
  sqlx::query("UPDATE retention_work SET phase = 'hidden' WHERE id = $1")
    .bind(work.work_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  work.phase = "hidden".to_owned();
  Ok(())
}

async fn hide_build_component(
  transaction: &mut Transaction<'_, Postgres>,
  build_id: BuildId,
  component: BuildResultComponent,
  observed_at: Timestamp,
) -> Result<(), StoreError> {
  let query = match component {
    BuildResultComponent::Logs => {
      "UPDATE builds SET logs_visible = false, logs_hidden_at = COALESCE(logs_hidden_at, \
       to_timestamp($1::double precision / 1000.0)) WHERE id = $2"
    }
    BuildResultComponent::Artifacts => {
      "UPDATE builds SET artifacts_visible = false, artifacts_hidden_at = COALESCE(artifacts_hidden_at, \
       to_timestamp($1::double precision / 1000.0)) WHERE id = $2"
    }
    BuildResultComponent::Reports => {
      "UPDATE builds SET reports_visible = false, reports_hidden_at = COALESCE(reports_hidden_at, \
       to_timestamp($1::double precision / 1000.0)) WHERE id = $2"
    }
    BuildResultComponent::Metadata => return Err(StoreError::Unavailable),
  };
  let updated = sqlx::query(query)
    .bind(observed_at.unix_millis())
    .bind(build_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(StoreError::NotFound {
      entity: EntityKind::Build,
    });
  }
  Ok(())
}

async fn allocate_search_deletion(
  transaction: &mut Transaction<'_, Postgres>,
  work: &mut LockedWork,
  observed_at: Timestamp,
) -> Result<(), StoreError> {
  if work.search_work_id.is_some() {
    return Ok(());
  }
  sqlx::query(
    "INSERT INTO log_index_project_positions (project_id, committed_through) VALUES ($1, 0) \
     ON CONFLICT (project_id) DO NOTHING",
  )
  .bind(work.project_id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let current: i64 =
    sqlx::query_scalar("SELECT committed_through FROM log_index_project_positions WHERE project_id = $1 FOR UPDATE")
      .bind(work.project_id.as_uuid())
      .fetch_one(&mut **transaction)
      .await
      .map_err(unavailable)?;
  let position = current.checked_add(1).ok_or(StoreError::Unavailable)?;
  let search_work_id = Uuid::new_v5(&work.build_id.as_uuid(), b"retention:delete-build-logs");
  sqlx::query(
    "INSERT INTO log_indexing_work \
       (id, project_id, position, build_id, chunk_id, operation, state, attempt_count, available_at, created_at) \
     VALUES ($1, $2, $3, $4, NULL, 'delete_build', 'pending', 0, \
       to_timestamp($5::double precision / 1000.0), to_timestamp($5::double precision / 1000.0)) \
     ON CONFLICT (id) DO NOTHING",
  )
  .bind(search_work_id)
  .bind(work.project_id.as_uuid())
  .bind(position)
  .bind(work.build_id.as_uuid())
  .bind(observed_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  sqlx::query(
    "UPDATE log_index_project_positions SET committed_through = GREATEST(committed_through, $1) WHERE project_id = $2",
  )
  .bind(position)
  .bind(work.project_id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  sqlx::query("UPDATE retention_work SET search_work_id = $1, search_position = $2 WHERE id = $3")
    .bind(search_work_id)
    .bind(position)
    .bind(work.work_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  work.search_work_id = Some(search_work_id);
  work.search_position = Some(position);
  Ok(())
}

async fn retention_objects(
  transaction: &mut Transaction<'_, Postgres>,
  work: &LockedWork,
  limit: u16,
  observed_at: Timestamp,
) -> Result<Vec<RetentionObject>, StoreError> {
  let objects = match work.component {
    BuildResultComponent::Metadata => Vec::new(),
    BuildResultComponent::Logs => log_retention_page(transaction, work.build_id, limit).await?,
    BuildResultComponent::Artifacts => artifact::retention_page(transaction, work.build_id, false, limit, observed_at)
      .await?
      .into_iter()
      .map(|upload| RetentionObject::Output(Box::new(upload)))
      .collect(),
    BuildResultComponent::Reports => artifact::retention_page(transaction, work.build_id, true, limit, observed_at)
      .await?
      .into_iter()
      .map(|upload| RetentionObject::Output(Box::new(upload)))
      .collect(),
  };
  Ok(objects)
}

#[derive(FromRow)]
struct LogObjectRow {
  id: Uuid,
  job_id: Uuid,
  stream: String,
  first_sequence: i64,
  last_sequence: i64,
  object_identity: String,
  byte_length: i64,
  sha256: Vec<u8>,
}

async fn log_retention_page(
  transaction: &mut Transaction<'_, Postgres>,
  build_id: BuildId,
  limit: u16,
) -> Result<Vec<RetentionObject>, StoreError> {
  sqlx::query_as::<_, LogObjectRow>(
    "SELECT id, job_id, stream, first_sequence, last_sequence, object_identity, byte_length, sha256 \
     FROM log_chunk_manifests WHERE build_id = $1 AND NOT visible AND deleted_at IS NULL ORDER BY id LIMIT $2",
  )
  .bind(build_id.as_uuid())
  .bind(i64::from(limit))
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(|row| {
    let digest: [u8; 32] = row.sha256.try_into().map_err(|_| StoreError::Unavailable)?;
    let stream = match row.stream.as_str() {
      "stdout" => BuildLogStream::Stdout,
      "stderr" => BuildLogStream::Stderr,
      _ => return Err(StoreError::Unavailable),
    };
    let manifest = LogChunkManifest::restore(StoredLogChunkManifest {
      chunk_id: LogChunkId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
      job_id: JobId::from_uuid(row.job_id).map_err(|_| StoreError::Unavailable)?,
      stream,
      first_sequence: unsigned(row.first_sequence)?,
      last_sequence: unsigned(row.last_sequence)?,
      byte_length: unsigned(row.byte_length)?,
      digest: LogChunkDigest::from_bytes(digest),
      object_identity: row.object_identity,
    })
    .map_err(|_| StoreError::Unavailable)?;
    Ok(RetentionObject::Log(manifest))
  })
  .collect()
}

async fn remaining_objects(transaction: &mut Transaction<'_, Postgres>, work: &LockedWork) -> Result<bool, StoreError> {
  let exists = match work.component {
    BuildResultComponent::Metadata => false,
    BuildResultComponent::Logs => sqlx::query_scalar(
      "SELECT EXISTS(SELECT 1 FROM log_chunk_manifests WHERE build_id = $1 AND NOT visible AND deleted_at IS NULL)",
    )
    .bind(work.build_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?,
    BuildResultComponent::Artifacts | BuildResultComponent::Reports => {
      let kind = work.component.as_str().trim_end_matches('s');
      sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM artifacts WHERE build_id = $1 AND artifact_type = $2 \
         AND NOT visible AND deleted_at IS NULL)",
      )
      .bind(work.build_id.as_uuid())
      .bind(kind)
      .fetch_one(&mut **transaction)
      .await
      .map_err(unavailable)?
    }
  };
  Ok(exists)
}

async fn log_is_deleted(
  transaction: &mut Transaction<'_, Postgres>,
  chunk_id: LogChunkId,
  build_id: BuildId,
) -> Result<bool, StoreError> {
  sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM log_chunk_manifests WHERE id = $1 AND build_id = $2")
    .bind(chunk_id.as_uuid())
    .bind(build_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::LogChunk,
    })
}

fn component(value: &str) -> Result<BuildResultComponent, StoreError> {
  match value {
    "metadata" => Ok(BuildResultComponent::Metadata),
    "logs" => Ok(BuildResultComponent::Logs),
    "artifacts" => Ok(BuildResultComponent::Artifacts),
    "reports" => Ok(BuildResultComponent::Reports),
    _ => Err(StoreError::Unavailable),
  }
}

fn phase(value: &str) -> Result<RetentionPhase, StoreError> {
  match value {
    "pending" => Ok(RetentionPhase::Pending),
    "hidden" => Ok(RetentionPhase::Hidden),
    "search_deleted" => Ok(RetentionPhase::SearchDeleted),
    "bytes_deleted" => Ok(RetentionPhase::BytesDeleted),
    "dead_letter" => Ok(RetentionPhase::DeadLetter),
    _ => Err(StoreError::Unavailable),
  }
}

fn log_position(value: Option<i64>) -> Result<LogIndexPosition, StoreError> {
  value
    .and_then(|value| u64::try_from(value).ok())
    .and_then(|value| LogIndexPosition::new(value).ok())
    .ok_or(StoreError::Unavailable)
}

fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}

fn unsigned(value: i64) -> Result<u64, StoreError> {
  u64::try_from(value).map_err(|_| StoreError::Unavailable)
}

const fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::RetentionWork,
  }
}
