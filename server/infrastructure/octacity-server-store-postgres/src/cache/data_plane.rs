use std::collections::HashSet;

use octacity_server_cache::{
  ActionResultV1, BlobDescriptor, CacheBlobObject, CacheCredentialDigest, CacheNamespace, CacheOperation, Digest,
};
use octacity_server_domain::Timestamp;
use octacity_server_store::{
  CacheBlobPreparationOutcome, CacheDataAccess, CachePublicationOutcome, PublishCacheAction, PublishCacheBlob,
  StoreError, StoreInputError, StoreOperation, cache_action_key, cache_blob_key,
};
use sqlx::{PgPool, types::Json};
use uuid::Uuid;

use super::records::{DataAuthorizationRow, current_lease_state, digest, number, timestamp, unsigned};
use crate::database::unavailable;

pub(crate) async fn find_missing_blobs(
  pool: &PgPool,
  access: CacheDataAccess,
  blobs: Vec<BlobDescriptor>,
) -> Result<Vec<BlobDescriptor>, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let scope = authorize_data(&mut transaction, &access, Some(CacheOperation::Read)).await?;
  let keys = blobs
    .iter()
    .map(|blob| {
      blob
        .validate()
        .map_err(|_| invalid_cache_data(StoreOperation::ReadCacheBlob))?;
      Ok(cache_blob_key(blob))
    })
    .collect::<Result<Vec<_>, StoreError>>()?;
  let present: HashSet<String> = sqlx::query_scalar(
    "SELECT blob.blob_key FROM cache_blobs AS blob
     WHERE blob.scope_id = $1 AND blob.blob_key = ANY($2)
       AND blob.published_at IS NOT NULL
       AND (blob.retention_until > to_timestamp($3::double precision / 1000.0)
         OR EXISTS (
           SELECT 1 FROM cache_actions AS action
           WHERE action.scope_id = blob.scope_id AND action.blob_key = blob.blob_key
             AND action.retention_until > to_timestamp($3::double precision / 1000.0)
         ))",
  )
  .bind(&scope.scope_id)
  .bind(&keys)
  .bind(access.observed_at.unix_millis())
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .collect();
  let mut emitted = HashSet::new();
  let missing = blobs
    .into_iter()
    .zip(keys)
    .filter_map(|(blob, key)| (!present.contains(&key) && emitted.insert(key)).then_some(blob))
    .collect();
  transaction.commit().await.map_err(unavailable)?;
  Ok(missing)
}

pub(crate) async fn resolve_namespace(
  pool: &PgPool,
  credential_digest: CacheCredentialDigest,
  observed_at: Timestamp,
) -> Result<CacheNamespace, StoreError> {
  let namespace: Option<String> = sqlx::query_scalar("SELECT namespace FROM cache_sessions WHERE credential_hash = $1")
    .bind(credential_digest.as_bytes().to_vec())
    .fetch_optional(pool)
    .await
    .map_err(unavailable)?;
  let namespace = CacheNamespace::new(namespace.ok_or(StoreError::CredentialRejected)?)
    .map_err(|_| invalid_cache_data(StoreOperation::ReadCacheBlob))?;
  let access = CacheDataAccess {
    credential_digest,
    namespace: namespace.clone(),
    observed_at,
  };
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  authorize_data(&mut transaction, &access, None).await?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(namespace)
}

pub(crate) async fn read_blob(
  pool: &PgPool,
  access: CacheDataAccess,
  blob: BlobDescriptor,
) -> Result<Option<CacheBlobObject>, StoreError> {
  blob
    .validate()
    .map_err(|_| invalid_cache_data(StoreOperation::ReadCacheBlob))?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let scope = authorize_data(&mut transaction, &access, Some(CacheOperation::Read)).await?;
  let present: bool = sqlx::query_scalar(
    "SELECT EXISTS(
       SELECT 1 FROM cache_blobs AS blob
       WHERE blob.scope_id = $1 AND blob.blob_key = $2
         AND blob.published_at IS NOT NULL
         AND (blob.retention_until > to_timestamp($3::double precision / 1000.0)
           OR EXISTS (
             SELECT 1 FROM cache_actions AS action
             WHERE action.scope_id = blob.scope_id AND action.blob_key = blob.blob_key
               AND action.retention_until > to_timestamp($3::double precision / 1000.0)
           ))
     )",
  )
  .bind(&scope.scope_id)
  .bind(cache_blob_key(&blob))
  .bind(access.observed_at.unix_millis())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  present
    .then(|| CacheBlobObject::new(scope.scope_id, blob).map_err(|_| invalid_cache_data(StoreOperation::ReadCacheBlob)))
    .transpose()
}

pub(crate) async fn prepare_blob(
  pool: &PgPool,
  access: CacheDataAccess,
  blob: BlobDescriptor,
) -> Result<CacheBlobPreparationOutcome, StoreError> {
  blob
    .validate()
    .map_err(|_| invalid_cache_data(StoreOperation::PublishCacheBlob))?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let scope = authorize_data(&mut transaction, &access, Some(CacheOperation::Write)).await?;
  lock_project(&mut transaction, scope.project_id).await?;
  let key = cache_blob_key(&blob);
  let existing: Option<(Json<BlobDescriptor>, bool)> = sqlx::query_as(
    "SELECT descriptor, published_at IS NOT NULL
     FROM cache_blobs WHERE scope_id = $1 AND blob_key = $2 FOR UPDATE",
  )
  .bind(&scope.scope_id)
  .bind(&key)
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let object = CacheBlobObject::new(scope.scope_id.clone(), blob.clone())
    .map_err(|_| invalid_cache_data(StoreOperation::PublishCacheBlob))?;
  if let Some((existing, published)) = existing {
    if existing.0 != blob {
      return Err(StoreError::Unavailable);
    }
    sqlx::query(
      "UPDATE cache_blobs SET retention_until = GREATEST(retention_until, to_timestamp($3::double precision / 1000.0))
       WHERE scope_id = $1 AND blob_key = $2",
    )
    .bind(&scope.scope_id)
    .bind(&key)
    .bind(scope.retention_until.unix_millis())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    transaction.commit().await.map_err(unavailable)?;
    return Ok(if published {
      CacheBlobPreparationOutcome::AlreadyPresent
    } else {
      CacheBlobPreparationOutcome::Upload(object)
    });
  }
  if !quota_allows(
    &mut transaction,
    scope.project_id,
    scope.quota_bytes,
    blob.encoded_size_bytes,
    access.observed_at,
  )
  .await?
  {
    return Ok(CacheBlobPreparationOutcome::QuotaExceeded);
  }
  sqlx::query(
    "INSERT INTO cache_blobs (
       scope_id, project_id, namespace, blob_key, descriptor, charged_bytes, retention_until, reserved_at, published_at
     ) VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0),
       to_timestamp($8::double precision / 1000.0), NULL)",
  )
  .bind(&scope.scope_id)
  .bind(scope.project_id)
  .bind(access.namespace.as_str())
  .bind(key)
  .bind(Json(blob))
  .bind(number(object.descriptor().encoded_size_bytes)?)
  .bind(scope.retention_until.unix_millis())
  .bind(access.observed_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(CacheBlobPreparationOutcome::Upload(object))
}

pub(crate) async fn publish_blob(
  pool: &PgPool,
  request: PublishCacheBlob,
) -> Result<CachePublicationOutcome, StoreError> {
  request
    .blob
    .validate()
    .map_err(|_| invalid_cache_data(StoreOperation::PublishCacheBlob))?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let scope = authorize_data(&mut transaction, &request.access, Some(CacheOperation::Write)).await?;
  lock_project(&mut transaction, scope.project_id).await?;
  let key = cache_blob_key(&request.blob);
  let existing: Option<(Json<BlobDescriptor>, bool)> = sqlx::query_as(
    "SELECT descriptor, published_at IS NOT NULL FROM cache_blobs WHERE scope_id = $1 AND blob_key = $2 FOR UPDATE",
  )
  .bind(&scope.scope_id)
  .bind(&key)
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let Some((existing, published)) = existing else {
    return Ok(CachePublicationOutcome::Conflict);
  };
  if existing.0 != request.blob {
    return Ok(CachePublicationOutcome::Conflict);
  }
  if published {
    transaction.commit().await.map_err(unavailable)?;
    return Ok(CachePublicationOutcome::AlreadyPresent);
  }
  sqlx::query(
    "UPDATE cache_blobs
     SET published_at = to_timestamp($3::double precision / 1000.0),
         retention_until = GREATEST(retention_until, to_timestamp($4::double precision / 1000.0))
     WHERE scope_id = $1 AND blob_key = $2 AND published_at IS NULL",
  )
  .bind(&scope.scope_id)
  .bind(&key)
  .bind(request.access.observed_at.unix_millis())
  .bind(scope.retention_until.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(CachePublicationOutcome::Published)
}

pub(crate) async fn read_action(
  pool: &PgPool,
  access: CacheDataAccess,
  action: Digest,
) -> Result<Option<ActionResultV1>, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let scope = authorize_data(&mut transaction, &access, Some(CacheOperation::Read)).await?;
  let result: Option<Json<ActionResultV1>> = sqlx::query_scalar(
    "SELECT result FROM cache_actions
     WHERE scope_id = $1 AND action_key = $2
       AND retention_until > to_timestamp($3::double precision / 1000.0)",
  )
  .bind(&scope.scope_id)
  .bind(cache_action_key(action))
  .bind(access.observed_at.unix_millis())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  result
    .map(|value| {
      value
        .0
        .validate()
        .map_err(|_| invalid_cache_data(StoreOperation::ReadCacheAction))?;
      Ok(value.0)
    })
    .transpose()
}

pub(crate) async fn publish_action(
  pool: &PgPool,
  request: PublishCacheAction,
) -> Result<CachePublicationOutcome, StoreError> {
  request
    .result
    .validate()
    .map_err(|_| invalid_cache_data(StoreOperation::PublishCacheAction))?;
  if request.result.action != request.action || request.wire_size_bytes == 0 {
    return Err(invalid_cache_data(StoreOperation::PublishCacheAction));
  }
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let scope = authorize_data(&mut transaction, &request.access, Some(CacheOperation::Write)).await?;
  lock_project(&mut transaction, scope.project_id).await?;
  let action_key = cache_action_key(request.action);
  let existing: Option<Json<ActionResultV1>> =
    sqlx::query_scalar("SELECT result FROM cache_actions WHERE scope_id = $1 AND action_key = $2")
      .bind(&scope.scope_id)
      .bind(&action_key)
      .fetch_optional(&mut *transaction)
      .await
      .map_err(unavailable)?;
  if let Some(existing) = existing {
    if existing.0 != request.result {
      return Ok(CachePublicationOutcome::Conflict);
    }
    sqlx::query(
      "UPDATE cache_actions SET retention_until = GREATEST(retention_until, to_timestamp($3::double precision / 1000.0))
       WHERE scope_id = $1 AND action_key = $2",
    )
    .bind(&scope.scope_id)
    .bind(&action_key)
    .bind(scope.retention_until.unix_millis())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    transaction.commit().await.map_err(unavailable)?;
    return Ok(CachePublicationOutcome::AlreadyPresent);
  }
  let blob_key = request.result.output_bundle.as_ref().map(cache_blob_key);
  if let Some(blob_key) = &blob_key {
    let updated = sqlx::query(
      "UPDATE cache_blobs
       SET retention_until = GREATEST(retention_until, to_timestamp($3::double precision / 1000.0))
       WHERE scope_id = $1 AND blob_key = $2 AND published_at IS NOT NULL",
    )
    .bind(&scope.scope_id)
    .bind(blob_key)
    .bind(scope.retention_until.unix_millis())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    if updated.rows_affected() == 0 {
      return Ok(CachePublicationOutcome::Conflict);
    }
  }
  if !quota_allows(
    &mut transaction,
    scope.project_id,
    scope.quota_bytes,
    request.wire_size_bytes,
    request.access.observed_at,
  )
  .await?
  {
    return Ok(CachePublicationOutcome::QuotaExceeded);
  }
  sqlx::query(
    "INSERT INTO cache_actions (
       scope_id, project_id, namespace, action_key, result, blob_key, charged_bytes, retention_until, published_at
     ) VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0),
       to_timestamp($9::double precision / 1000.0))",
  )
  .bind(&scope.scope_id)
  .bind(scope.project_id)
  .bind(request.access.namespace.as_str())
  .bind(action_key)
  .bind(Json(request.result))
  .bind(blob_key)
  .bind(number(request.wire_size_bytes)?)
  .bind(scope.retention_until.unix_millis())
  .bind(request.access.observed_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(CachePublicationOutcome::Published)
}

pub(super) struct DataScope {
  pub(super) scope_id: String,
  project_id: Uuid,
  quota_bytes: u64,
  retention_until: Timestamp,
}

pub(super) async fn authorize_data(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  access: &CacheDataAccess,
  operation: Option<CacheOperation>,
) -> Result<DataScope, StoreError> {
  let row: Option<DataAuthorizationRow> = sqlx::query_as(
    "SELECT session.project_id, session.scope_id, session.permissions, session.credential_hash,
              session.quota_bytes, session.state,
              FLOOR(EXTRACT(EPOCH FROM session.expires_at) * 1000)::BIGINT AS session_expires_at_millis,
              FLOOR(EXTRACT(EPOCH FROM session.retention_until) * 1000)::BIGINT AS retention_until_millis,
              lease.state AS lease_state,
              FLOOR(EXTRACT(EPOCH FROM lease.expires_at) * 1000)::BIGINT AS lease_expires_at_millis,
              registration.revoked_at IS NOT NULL AS registration_revoked,
              FLOOR(EXTRACT(EPOCH FROM registration.expires_at) * 1000)::BIGINT AS registration_expires_at_millis,
              session.namespace
       FROM cache_sessions AS session
       JOIN leases AS lease ON lease.id = session.lease_id AND lease.registration_id = session.registration_id
       JOIN agent_registrations AS registration ON registration.id = session.registration_id
       WHERE session.credential_hash = $1",
  )
  .bind(access.credential_digest.as_bytes().to_vec())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let Some(row) = row else {
    return Err(StoreError::CredentialRejected);
  };
  let now = access.observed_at.unix_millis();
  let permitted = operation.map_or(row.permissions.0.read || row.permissions.0.write, |value| {
    row.permissions.0.allows(value)
  });
  if row.state != "active"
    || row.namespace != access.namespace.as_str()
    || !permitted
    || !digest(&row.credential_hash)?.matches(access.credential_digest)
    || now >= row.session_expires_at_millis
    || !current_lease_state(&row.lease_state)
    || now >= row.lease_expires_at_millis
    || row.registration_revoked
    || now >= row.registration_expires_at_millis
  {
    return Err(StoreError::CredentialRejected);
  }
  Ok(DataScope {
    scope_id: row.scope_id,
    project_id: row.project_id,
    quota_bytes: unsigned(row.quota_bytes)?,
    retention_until: timestamp(row.retention_until_millis)?,
  })
}

async fn lock_project(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  project_id: Uuid,
) -> Result<(), StoreError> {
  sqlx::query_scalar::<_, Uuid>("SELECT id FROM projects WHERE id = $1 FOR UPDATE")
    .bind(project_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  Ok(())
}

async fn quota_allows(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  project_id: Uuid,
  quota_bytes: u64,
  additional: u64,
  now: Timestamp,
) -> Result<bool, StoreError> {
  let used: i64 = sqlx::query_scalar(
    "SELECT (COALESCE((SELECT SUM(charged_bytes) FROM cache_actions
                       WHERE project_id = $1 AND retention_until > to_timestamp($2::double precision / 1000.0)), 0)
          + COALESCE((SELECT SUM(blob.charged_bytes) FROM cache_blobs AS blob
                       WHERE blob.project_id = $1 AND (
                         blob.retention_until > to_timestamp($2::double precision / 1000.0)
                         OR EXISTS (SELECT 1 FROM cache_actions AS action
                           WHERE action.scope_id = blob.scope_id AND action.blob_key = blob.blob_key
                             AND action.retention_until > to_timestamp($2::double precision / 1000.0)))), 0))::BIGINT",
  )
  .bind(project_id)
  .bind(now.unix_millis())
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(
    unsigned(used)?
      .checked_add(additional)
      .is_some_and(|total| total <= quota_bytes),
  )
}

fn invalid_cache_data(operation: StoreOperation) -> StoreError {
  StoreError::InvalidInput {
    operation,
    source: StoreInputError::InvalidCacheData,
  }
}
