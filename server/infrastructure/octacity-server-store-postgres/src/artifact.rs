use std::str::FromStr as _;

use octacity_server_domain::{
  ArtifactId, ArtifactName, ArtifactUploadId, ArtifactVersion, AttemptId, BuildId, EntityKind, JobId, LeaseId,
  Timestamp,
};
use octacity_server_store::{
  ArtifactContentDigest, ArtifactEvent, ArtifactIdentity, ArtifactMediaType, ArtifactRecord, ArtifactReportFormat,
  ArtifactRetentionPolicy, ArtifactState, ArtifactTransitionAuthority, ArtifactType, ArtifactUploadRecord,
  ArtifactVerificationResult, BeginArtifactUpload, BeginArtifactUploadOutcome, IdempotencyKey, ListPublishedArtifacts,
  MutationDisposition, ReserveArtifact, StoreError, StoreInputError, StoreOperation, TransitionArtifact,
  VerifyArtifactUpload, artifact_authority_is_valid,
};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
  database::{classify, number, unavailable},
  lease,
};

#[derive(FromRow)]
struct ArtifactRow {
  id: Uuid,
  build_id: Uuid,
  attempt_id: Uuid,
  job_id: Uuid,
  lease_id: Uuid,
  logical_name: String,
  artifact_type: String,
  media_type: String,
  report_format: Option<String>,
  byte_length: i64,
  sha256: Vec<u8>,
  state: String,
  retention_until_millis: Option<i64>,
  version: i64,
  created_at_millis: i64,
  published_at_millis: Option<i64>,
  deleted_at_millis: Option<i64>,
}

#[derive(FromRow)]
struct ArtifactUploadRow {
  upload_id: Uuid,
  idempotency_key: String,
  capability_expires_at_millis: i64,
  transport_media_type: String,
  producer_run_id: String,
  producer_task_id: String,
  id: Uuid,
  build_id: Uuid,
  attempt_id: Uuid,
  job_id: Uuid,
  lease_id: Uuid,
  logical_name: String,
  artifact_type: String,
  media_type: String,
  report_format: Option<String>,
  byte_length: i64,
  sha256: Vec<u8>,
  state: String,
  retention_until_millis: Option<i64>,
  version: i64,
  created_at_millis: i64,
  published_at_millis: Option<i64>,
  deleted_at_millis: Option<i64>,
}

macro_rules! upload_query {
  ($predicate:literal) => {
    concat!(
      "SELECT upload.id AS upload_id, upload.idempotency_key, ",
      "FLOOR(EXTRACT(EPOCH FROM upload.expires_at) * 1000)::BIGINT AS capability_expires_at_millis, ",
      "upload.transport_media_type, ",
      "upload.producer_run_id, upload.producer_task_id, ",
      "artifact.id, artifact.build_id, artifact.attempt_id, artifact.job_id, artifact.lease_id, ",
      "artifact.logical_name, artifact.artifact_type, artifact.media_type, artifact.report_format, ",
      "artifact.byte_length, artifact.sha256, artifact.state, artifact.version, ",
      "FLOOR(EXTRACT(EPOCH FROM artifact.retention_until) * 1000)::BIGINT AS retention_until_millis, ",
      "FLOOR(EXTRACT(EPOCH FROM artifact.created_at) * 1000)::BIGINT AS created_at_millis, ",
      "FLOOR(EXTRACT(EPOCH FROM artifact.published_at) * 1000)::BIGINT AS published_at_millis, ",
      "FLOOR(EXTRACT(EPOCH FROM artifact.deleted_at) * 1000)::BIGINT AS deleted_at_millis ",
      "FROM artifact_uploads AS upload JOIN artifacts AS artifact ON artifact.id = upload.artifact_id WHERE ",
      $predicate
    )
  };
  ($predicate:literal, for_update) => {
    concat!(upload_query!($predicate), " FOR UPDATE OF upload, artifact")
  };
}

impl TryFrom<ArtifactUploadRow> for ArtifactUploadRecord {
  type Error = StoreError;

  fn try_from(row: ArtifactUploadRow) -> Result<Self, Self::Error> {
    let artifact = ArtifactRow {
      id: row.id,
      build_id: row.build_id,
      attempt_id: row.attempt_id,
      job_id: row.job_id,
      lease_id: row.lease_id,
      logical_name: row.logical_name,
      artifact_type: row.artifact_type,
      media_type: row.media_type,
      report_format: row.report_format,
      byte_length: row.byte_length,
      sha256: row.sha256,
      state: row.state,
      retention_until_millis: row.retention_until_millis,
      version: row.version,
      created_at_millis: row.created_at_millis,
      published_at_millis: row.published_at_millis,
      deleted_at_millis: row.deleted_at_millis,
    }
    .try_into()?;
    Ok(Self {
      upload_id: ArtifactUploadId::from_uuid(row.upload_id).map_err(|_| StoreError::Unavailable)?,
      idempotency_key: IdempotencyKey::new(row.idempotency_key).map_err(|_| StoreError::Unavailable)?,
      artifact,
      producer_run_id: row.producer_run_id.parse().map_err(|_| StoreError::Unavailable)?,
      producer_task_id: row.producer_task_id.parse().map_err(|_| StoreError::Unavailable)?,
      transport_media_type: ArtifactMediaType::new(row.transport_media_type).map_err(|_| StoreError::Unavailable)?,
      capability_expires_at: timestamp(row.capability_expires_at_millis)?,
    })
  }
}

impl TryFrom<ArtifactRow> for ArtifactRecord {
  type Error = StoreError;

  fn try_from(row: ArtifactRow) -> Result<Self, Self::Error> {
    let digest: [u8; 32] = row.sha256.try_into().map_err(|_| StoreError::Unavailable)?;
    let report_format = row
      .report_format
      .map(ArtifactReportFormat::new)
      .transpose()
      .map_err(|_| StoreError::Unavailable)?;
    let artifact_type = match (row.artifact_type.as_str(), report_format) {
      ("artifact", None) => ArtifactType::Artifact,
      ("report", Some(format)) => ArtifactType::Report(format),
      _ => return Err(StoreError::Unavailable),
    };
    let retention = row
      .retention_until_millis
      .map(timestamp)
      .transpose()?
      .map_or(ArtifactRetentionPolicy::Keep, ArtifactRetentionPolicy::DeleteAfter);
    let identity = ArtifactIdentity {
      artifact_id: ArtifactId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
      build_id: BuildId::from_uuid(row.build_id).map_err(|_| StoreError::Unavailable)?,
      attempt_id: AttemptId::from_uuid(row.attempt_id).map_err(|_| StoreError::Unavailable)?,
      job_id: JobId::from_uuid(row.job_id).map_err(|_| StoreError::Unavailable)?,
      lease_id: LeaseId::from_uuid(row.lease_id).map_err(|_| StoreError::Unavailable)?,
      logical_name: ArtifactName::new(row.logical_name).map_err(|_| StoreError::Unavailable)?,
      artifact_type,
      media_type: ArtifactMediaType::new(row.media_type).map_err(|_| StoreError::Unavailable)?,
      size_bytes: u64::try_from(row.byte_length).map_err(|_| StoreError::Unavailable)?,
      digest: ArtifactContentDigest::from_bytes(digest),
      retention,
    };
    ArtifactRecord::restore(
      identity,
      ArtifactState::from_str(&row.state).map_err(|_| StoreError::Unavailable)?,
      ArtifactVersion::new(u64::try_from(row.version).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
      timestamp(row.created_at_millis)?,
      row.published_at_millis.map(timestamp).transpose()?,
      row.deleted_at_millis.map(timestamp).transpose()?,
    )
    .map_err(|_| StoreError::Unavailable)
  }
}

pub(crate) async fn reserve(pool: &PgPool, request: ReserveArtifact) -> Result<ArtifactRecord, StoreError> {
  let record = request
    .pending_record()
    .map_err(|_| invalid(StoreOperation::ReserveArtifact))?;
  if request.lease.lease_id != request.identity.lease_id {
    return Err(invalid(StoreOperation::ReserveArtifact));
  }
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  require_lease_identity(
    &mut transaction,
    request.lease,
    request.reserved_at,
    &request.identity,
    StoreOperation::ReserveArtifact,
  )
  .await?;
  insert_artifact(&mut transaction, &record, StoreOperation::ReserveArtifact).await?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(record)
}

pub(crate) async fn read(pool: &PgPool, artifact_id: ArtifactId) -> Result<ArtifactRecord, StoreError> {
  let row = select_artifact(pool, artifact_id).await?;
  row.try_into()
}

pub(crate) async fn transition(pool: &PgPool, request: TransitionArtifact) -> Result<ArtifactRecord, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  if let ArtifactTransitionAuthority::Lease(access) = request.authority {
    require_lease_identity(
      &mut transaction,
      access,
      request.transitioned_at,
      &request.expected_identity,
      StoreOperation::TransitionArtifact,
    )
    .await?;
  }
  let current: ArtifactRecord = lock_artifact(&mut transaction, request.expected_identity.artifact_id)
    .await?
    .try_into()?;
  if !artifact_authority_is_valid(request.authority, current.identity(), current.state(), request.event) {
    return Err(invalid(StoreOperation::TransitionArtifact));
  }
  let updated = current
    .transition(
      &request.expected_identity,
      request.expected_version,
      request.event,
      request.transitioned_at,
    )
    .map_err(|_| StoreError::Conflict {
      entity: EntityKind::Artifact,
    })?;
  update_artifact(
    &mut transaction,
    &updated,
    current.version().get(),
    StoreOperation::TransitionArtifact,
  )
  .await?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(updated)
}

pub(crate) async fn begin_upload(
  pool: &PgPool,
  request: BeginArtifactUpload,
) -> Result<BeginArtifactUploadOutcome, StoreError> {
  if request.capability_expires_at <= request.reserved_at {
    return Err(invalid(StoreOperation::BeginArtifactUpload));
  }
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let lease_row = lease::load(&mut transaction, request.lease, StoreOperation::BeginArtifactUpload).await?;
  lease::require_current(&lease_row, request.lease, request.reserved_at)?;
  if lease_row.job_id != request.job_id.as_uuid()
    || lease_row.attempt_number != number(request.attempt.get(), StoreOperation::BeginArtifactUpload)?
  {
    return Err(StoreError::Fenced {
      lease: request.lease.lease_id,
    });
  }
  if let Some(row) = select_upload_by_key(
    &mut transaction,
    request.lease.lease_id,
    request.idempotency_key.as_str(),
  )
  .await?
  {
    let upload: ArtifactUploadRecord = row.try_into()?;
    if !upload_matches(&upload, &request) {
      return Err(StoreError::Conflict {
        entity: EntityKind::ArtifactUpload,
      });
    }
    transaction.commit().await.map_err(unavailable)?;
    return Ok(BeginArtifactUploadOutcome {
      upload,
      disposition: MutationDisposition::Replayed,
    });
  }
  let identity = ArtifactIdentity {
    artifact_id: request.artifact_id,
    build_id: BuildId::from_uuid(lease_row.build_id).map_err(|_| StoreError::Unavailable)?,
    attempt_id: AttemptId::from_uuid(lease_row.attempt_id).map_err(|_| StoreError::Unavailable)?,
    job_id: request.job_id,
    lease_id: request.lease.lease_id,
    logical_name: request.logical_name.clone(),
    artifact_type: request.artifact_type.clone(),
    media_type: request.media_type.clone(),
    size_bytes: request.size_bytes,
    digest: request.digest,
    retention: ArtifactRetentionPolicy::Keep,
  };
  let artifact =
    ArtifactRecord::pending(identity, request.reserved_at).map_err(|_| invalid(StoreOperation::BeginArtifactUpload))?;
  insert_artifact(&mut transaction, &artifact, StoreOperation::BeginArtifactUpload).await?;
  let identity = artifact.identity();
  sqlx::query(
    "INSERT INTO artifact_uploads \
       (id, artifact_id, job_id, lease_id, idempotency_key, logical_name, producer_run_id, producer_task_id, artifact_type, media_type, transport_media_type, report_format, \
        expected_size, expected_sha256, object_identity, object_generation, state, created_at, expires_at, completed_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NULL, NULL, 'pending', \
             to_timestamp($15::double precision / 1000.0), to_timestamp($16::double precision / 1000.0), NULL)",
  )
  .bind(request.upload_id.as_uuid())
  .bind(identity.artifact_id.as_uuid())
  .bind(identity.job_id.as_uuid())
  .bind(identity.lease_id.as_uuid())
  .bind(request.idempotency_key.as_str())
  .bind(identity.logical_name.as_str())
  .bind(request.producer_run_id.to_string())
  .bind(request.producer_task_id.to_string())
  .bind(identity.artifact_type.as_str())
  .bind(identity.media_type.as_str())
  .bind(request.transport_media_type.as_str())
  .bind(identity.artifact_type.report_format().map(ArtifactReportFormat::as_str))
  .bind(number(identity.size_bytes, StoreOperation::BeginArtifactUpload)?)
  .bind(identity.digest.as_bytes().to_vec())
  .bind(request.reserved_at.unix_millis())
  .bind(request.capability_expires_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::ArtifactUpload))?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(BeginArtifactUploadOutcome {
    upload: ArtifactUploadRecord {
      upload_id: request.upload_id,
      idempotency_key: request.idempotency_key,
      artifact,
      producer_run_id: request.producer_run_id,
      producer_task_id: request.producer_task_id,
      transport_media_type: request.transport_media_type,
      capability_expires_at: request.capability_expires_at,
    },
    disposition: MutationDisposition::Applied,
  })
}

pub(crate) async fn read_upload(
  pool: &PgPool,
  upload_id: ArtifactUploadId,
) -> Result<ArtifactUploadRecord, StoreError> {
  sqlx::query_as::<_, ArtifactUploadRow>(upload_query!("upload.id = $1"))
    .bind(upload_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::ArtifactUpload,
    })?
    .try_into()
}

pub(crate) async fn begin_verification(
  pool: &PgPool,
  request: VerifyArtifactUpload,
) -> Result<ArtifactUploadRecord, StoreError> {
  change_verification(pool, request, None).await
}

pub(crate) async fn finish_verification(
  pool: &PgPool,
  request: VerifyArtifactUpload,
  result: ArtifactVerificationResult,
) -> Result<ArtifactUploadRecord, StoreError> {
  change_verification(pool, request, Some(result)).await
}

pub(crate) async fn published(pool: &PgPool, artifact_id: ArtifactId) -> Result<ArtifactUploadRecord, StoreError> {
  sqlx::query_as::<_, ArtifactUploadRow>(upload_query!("artifact.id = $1 AND artifact.state = 'published'"))
    .bind(artifact_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Artifact,
    })?
    .try_into()
}

pub(crate) async fn list_published(
  pool: &PgPool,
  query: ListPublishedArtifacts,
) -> Result<Vec<ArtifactUploadRecord>, StoreError> {
  if query.limit == 0 || query.limit > octacity_server_store::MAX_ARTIFACT_PAGE_SIZE {
    return Err(invalid(StoreOperation::ListPublishedArtifacts));
  }
  sqlx::query_as::<_, ArtifactUploadRow>(upload_query!(
    "artifact.build_id = $1 AND artifact.state = 'published' ORDER BY artifact.published_at, artifact.id LIMIT $2"
  ))
  .bind(query.build_id.as_uuid())
  .bind(i64::from(query.limit))
  .fetch_all(pool)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(TryInto::try_into)
  .collect()
}

async fn change_verification(
  pool: &PgPool,
  request: VerifyArtifactUpload,
  result: Option<ArtifactVerificationResult>,
) -> Result<ArtifactUploadRecord, StoreError> {
  let operation = if result.is_some() {
    StoreOperation::FinishArtifactVerification
  } else {
    StoreOperation::BeginArtifactVerification
  };
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let lease_row = lease::load(&mut transaction, request.lease, operation).await?;
  lease::require_current(&lease_row, request.lease, request.observed_at)?;
  if lease_row.job_id != request.job_id.as_uuid()
    || lease_row.attempt_number != number(request.attempt.get(), operation)?
  {
    return Err(StoreError::Fenced {
      lease: request.lease.lease_id,
    });
  }
  let current: ArtifactUploadRecord =
    sqlx::query_as::<_, ArtifactUploadRow>(upload_query!("upload.id = $1", for_update))
      .bind(request.upload_id.as_uuid())
      .fetch_optional(&mut *transaction)
      .await
      .map_err(unavailable)?
      .ok_or(StoreError::NotFound {
        entity: EntityKind::ArtifactUpload,
      })?
      .try_into()?;
  if current.artifact.identity().lease_id != request.lease.lease_id
    || current.artifact.identity().job_id != request.job_id
  {
    return Err(StoreError::Fenced {
      lease: request.lease.lease_id,
    });
  }
  let event = match (result, current.artifact.state()) {
    (None, ArtifactState::Pending) => Some(ArtifactEvent::BeginVerification),
    (None, ArtifactState::Verifying | ArtifactState::Published) => None,
    (Some(ArtifactVerificationResult::Verified), ArtifactState::Verifying) => Some(ArtifactEvent::Publish),
    (Some(ArtifactVerificationResult::Verified), ArtifactState::Published) => None,
    (Some(ArtifactVerificationResult::Rejected), ArtifactState::Verifying) => Some(ArtifactEvent::VerificationFailed),
    (Some(ArtifactVerificationResult::Rejected), ArtifactState::Pending) => None,
    _ => {
      return Err(StoreError::Conflict {
        entity: EntityKind::ArtifactUpload,
      });
    }
  };
  let mut updated = current;
  if let Some(event) = event {
    updated.artifact = updated
      .artifact
      .transition(
        updated.artifact.identity(),
        updated.artifact.version(),
        event,
        request.observed_at,
      )
      .map_err(|_| StoreError::Conflict {
        entity: EntityKind::ArtifactUpload,
      })?;
    update_artifact_and_upload(&mut transaction, &updated, event, operation).await?;
  }
  transaction.commit().await.map_err(unavailable)?;
  Ok(updated)
}

async fn insert_artifact(
  transaction: &mut Transaction<'_, Postgres>,
  artifact: &ArtifactRecord,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  let identity = artifact.identity();
  let result = sqlx::query(
    "INSERT INTO artifacts \
       (id, build_id, attempt_id, job_id, lease_id, logical_name, artifact_type, media_type, report_format, \
        byte_length, sha256, object_identity, object_generation, state, retention_until, version, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NULL, NULL, 'pending', \
             CASE WHEN $12::BIGINT IS NULL THEN NULL ELSE to_timestamp($12::double precision / 1000.0) END, 1, \
             to_timestamp($13::double precision / 1000.0))",
  )
  .bind(identity.artifact_id.as_uuid())
  .bind(identity.build_id.as_uuid())
  .bind(identity.attempt_id.as_uuid())
  .bind(identity.job_id.as_uuid())
  .bind(identity.lease_id.as_uuid())
  .bind(identity.logical_name.as_str())
  .bind(identity.artifact_type.as_str())
  .bind(identity.media_type.as_str())
  .bind(identity.artifact_type.report_format().map(ArtifactReportFormat::as_str))
  .bind(number(identity.size_bytes, operation)?)
  .bind(identity.digest.as_bytes().to_vec())
  .bind(identity.retention.delete_after().map(Timestamp::unix_millis))
  .bind(artifact.created_at().unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Artifact))?;
  if result.rows_affected() != 1 {
    return Err(StoreError::Conflict {
      entity: EntityKind::Artifact,
    });
  }
  Ok(())
}

async fn update_artifact_and_upload(
  transaction: &mut Transaction<'_, Postgres>,
  upload: &ArtifactUploadRecord,
  event: ArtifactEvent,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  let artifact = &upload.artifact;
  let previous_version = artifact.version().get().checked_sub(1).ok_or(StoreError::Unavailable)?;
  update_artifact(transaction, artifact, previous_version, operation).await?;
  sqlx::query(
    "UPDATE artifact_uploads SET state = $1, \
       completed_at = CASE WHEN $2::BIGINT IS NULL THEN NULL ELSE to_timestamp($2::double precision / 1000.0) END \
     WHERE id = $3",
  )
  .bind(artifact.state().as_str())
  .bind(if event == ArtifactEvent::Publish {
    artifact.published_at().map(Timestamp::unix_millis)
  } else {
    None
  })
  .bind(upload.upload_id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::ArtifactUpload))?;
  Ok(())
}

async fn update_artifact(
  transaction: &mut Transaction<'_, Postgres>,
  artifact: &ArtifactRecord,
  previous_version: u64,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  let result = sqlx::query(
    "UPDATE artifacts SET state = $1, version = $2, \
       published_at = CASE WHEN $3::BIGINT IS NULL THEN NULL ELSE to_timestamp($3::double precision / 1000.0) END, \
       deleted_at = CASE WHEN $4::BIGINT IS NULL THEN NULL ELSE to_timestamp($4::double precision / 1000.0) END \
     WHERE id = $5 AND version = $6",
  )
  .bind(artifact.state().as_str())
  .bind(number(artifact.version().get(), operation)?)
  .bind(artifact.published_at().map(Timestamp::unix_millis))
  .bind(artifact.deleted_at().map(Timestamp::unix_millis))
  .bind(artifact.identity().artifact_id.as_uuid())
  .bind(number(previous_version, operation)?)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Artifact))?;
  if result.rows_affected() != 1 {
    return Err(StoreError::Conflict {
      entity: EntityKind::Artifact,
    });
  }
  Ok(())
}

async fn select_upload_by_key(
  transaction: &mut Transaction<'_, Postgres>,
  lease_id: LeaseId,
  idempotency_key: &str,
) -> Result<Option<ArtifactUploadRow>, StoreError> {
  sqlx::query_as::<_, ArtifactUploadRow>(upload_query!(
    "upload.lease_id = $1 AND upload.idempotency_key = $2",
    for_update
  ))
  .bind(lease_id.as_uuid())
  .bind(idempotency_key)
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)
}

fn upload_matches(existing: &ArtifactUploadRecord, request: &BeginArtifactUpload) -> bool {
  let identity = existing.artifact.identity();
  identity.lease_id == request.lease.lease_id
    && identity.job_id == request.job_id
    && identity.logical_name == request.logical_name
    && existing.producer_run_id == request.producer_run_id
    && existing.producer_task_id == request.producer_task_id
    && identity.artifact_type == request.artifact_type
    && identity.media_type == request.media_type
    && existing.transport_media_type == request.transport_media_type
    && identity.size_bytes == request.size_bytes
    && identity.digest == request.digest
}

async fn require_lease_identity(
  transaction: &mut Transaction<'_, Postgres>,
  access: octacity_server_store::LeaseAccess,
  observed_at: Timestamp,
  identity: &ArtifactIdentity,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  let row = lease::load(transaction, access, operation).await?;
  lease::require_current(&row, access, observed_at)?;
  if row.job_id != identity.job_id.as_uuid()
    || row.attempt_id != identity.attempt_id.as_uuid()
    || row.build_id != identity.build_id.as_uuid()
    || access.lease_id != identity.lease_id
  {
    return Err(StoreError::Fenced { lease: access.lease_id });
  }
  Ok(())
}

async fn select_artifact(pool: &PgPool, artifact_id: ArtifactId) -> Result<ArtifactRow, StoreError> {
  sqlx::query_as(artifact_select(false))
    .bind(artifact_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Artifact,
    })
}

async fn lock_artifact(
  transaction: &mut Transaction<'_, Postgres>,
  artifact_id: ArtifactId,
) -> Result<ArtifactRow, StoreError> {
  sqlx::query_as(artifact_select(true))
    .bind(artifact_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Artifact,
    })
}

fn artifact_select(for_update: bool) -> &'static str {
  if for_update {
    "SELECT id, build_id, attempt_id, job_id, lease_id, logical_name, artifact_type, media_type, report_format, \
       byte_length, sha256, state, version, \
       FLOOR(EXTRACT(EPOCH FROM retention_until) * 1000)::BIGINT AS retention_until_millis, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM published_at) * 1000)::BIGINT AS published_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM deleted_at) * 1000)::BIGINT AS deleted_at_millis \
     FROM artifacts WHERE id = $1 FOR UPDATE"
  } else {
    "SELECT id, build_id, attempt_id, job_id, lease_id, logical_name, artifact_type, media_type, report_format, \
       byte_length, sha256, state, version, \
       FLOOR(EXTRACT(EPOCH FROM retention_until) * 1000)::BIGINT AS retention_until_millis, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM published_at) * 1000)::BIGINT AS published_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM deleted_at) * 1000)::BIGINT AS deleted_at_millis \
     FROM artifacts WHERE id = $1"
  }
}

fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}

const fn invalid(operation: StoreOperation) -> StoreError {
  StoreError::InvalidInput {
    operation,
    source: StoreInputError::InvalidArtifact,
  }
}
