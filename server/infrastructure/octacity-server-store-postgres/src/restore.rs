use async_trait::async_trait;
use octacity_server_cache::{BlobDescriptor, CacheBlobObject};
use octacity_server_domain::{ArtifactId, JobId, LogChunkId, Timestamp};
use octacity_server_store::{
  BuildLogStream, LogChunkDigest, LogChunkManifest, MAX_RESTORE_RECONCILIATION_BATCH_SIZE, RestoreArtifactPage,
  RestoreCacheBlobPage, RestoreCacheCursor, RestoreFactoryArtifactPage, RestoreFactoryRecovery, RestoreInventoryStore,
  RestoreLogChunkPage, StoreError, StoredLogChunkManifest,
};
use sqlx::{FromRow, PgPool, types::Json};

use crate::{database::unavailable, store::PostgresStore};

#[async_trait]
impl RestoreInventoryStore for PostgresStore {
  async fn recover_restored_factory_state(
    &self,
    observed_at: Timestamp,
    limit: u16,
  ) -> Result<RestoreFactoryRecovery, StoreError> {
    crate::factory_run::recover_restored_ownership(self.pool(), observed_at, limit).await
  }

  async fn restore_artifact_page(
    &self,
    after: Option<ArtifactId>,
    limit: u16,
  ) -> Result<RestoreArtifactPage, StoreError> {
    crate::artifact::restore_page(self.pool(), after, limit).await
  }

  async fn restore_factory_artifact_page(
    &self,
    after: Option<ArtifactId>,
    limit: u16,
  ) -> Result<RestoreFactoryArtifactPage, StoreError> {
    crate::artifact::restore_factory_page(self.pool(), after, limit).await
  }

  async fn restore_log_chunk_page(
    &self,
    after: Option<LogChunkId>,
    limit: u16,
  ) -> Result<RestoreLogChunkPage, StoreError> {
    log_chunk_page(self.pool(), after, limit).await
  }

  async fn restore_cache_blob_page(
    &self,
    after: Option<RestoreCacheCursor>,
    limit: u16,
  ) -> Result<RestoreCacheBlobPage, StoreError> {
    cache_blob_page(self.pool(), after, limit).await
  }
}

#[derive(FromRow)]
struct LogChunkRow {
  id: uuid::Uuid,
  job_id: uuid::Uuid,
  stream: String,
  first_sequence: i64,
  last_sequence: i64,
  object_identity: String,
  byte_length: i64,
  sha256: Vec<u8>,
}

async fn log_chunk_page(
  pool: &PgPool,
  after: Option<LogChunkId>,
  limit: u16,
) -> Result<RestoreLogChunkPage, StoreError> {
  validate_limit(limit)?;
  let mut rows = sqlx::query_as::<_, LogChunkRow>(
    "SELECT manifest.id, manifest.job_id, manifest.stream, manifest.first_sequence, manifest.last_sequence, \
       manifest.object_identity, manifest.byte_length, manifest.sha256 \
     FROM log_chunk_manifests AS manifest \
     JOIN builds AS build ON build.id = manifest.build_id \
     WHERE manifest.visible AND manifest.deleted_at IS NULL AND build.logs_visible \
       AND ($1::UUID IS NULL OR manifest.id > $1) \
     ORDER BY manifest.id LIMIT $2",
  )
  .bind(after.map(LogChunkId::as_uuid))
  .bind(i64::from(limit) + 1)
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let has_more = rows.len() > usize::from(limit);
  rows.truncate(usize::from(limit));
  let items = rows
    .into_iter()
    .map(LogChunkRow::into_manifest)
    .collect::<Result<Vec<_>, StoreError>>()?;
  let next_after = has_more.then(|| {
    items
      .last()
      .expect("a restore page with more rows cannot be empty")
      .chunk_id()
  });
  Ok(RestoreLogChunkPage { items, next_after })
}

impl LogChunkRow {
  fn into_manifest(self) -> Result<LogChunkManifest, StoreError> {
    let stream = match self.stream.as_str() {
      "stdout" => BuildLogStream::Stdout,
      "stderr" => BuildLogStream::Stderr,
      _ => return Err(StoreError::Unavailable),
    };
    let digest: [u8; 32] = self.sha256.try_into().map_err(|_| StoreError::Unavailable)?;
    LogChunkManifest::restore(StoredLogChunkManifest {
      chunk_id: LogChunkId::from_uuid(self.id).map_err(|_| StoreError::Unavailable)?,
      job_id: JobId::from_uuid(self.job_id).map_err(|_| StoreError::Unavailable)?,
      stream,
      first_sequence: unsigned(self.first_sequence)?,
      last_sequence: unsigned(self.last_sequence)?,
      byte_length: unsigned(self.byte_length)?,
      digest: LogChunkDigest::from_bytes(digest),
      object_identity: self.object_identity,
    })
    .map_err(|_| StoreError::Unavailable)
  }
}

#[derive(FromRow)]
struct CacheBlobRow {
  scope_id: String,
  blob_key: String,
  descriptor: Json<BlobDescriptor>,
}

async fn cache_blob_page(
  pool: &PgPool,
  after: Option<RestoreCacheCursor>,
  limit: u16,
) -> Result<RestoreCacheBlobPage, StoreError> {
  validate_limit(limit)?;
  let after_scope = after.as_ref().map(|cursor| cursor.scope_id.as_str());
  let after_key = after.as_ref().map(|cursor| cursor.blob_key.as_str());
  let mut rows = sqlx::query_as::<_, CacheBlobRow>(
    "SELECT scope_id, blob_key, descriptor FROM cache_blobs \
     WHERE published_at IS NOT NULL AND (\
       $1::TEXT IS NULL OR (scope_id, blob_key) > ($1, $2)\
     ) ORDER BY scope_id, blob_key LIMIT $3",
  )
  .bind(after_scope)
  .bind(after_key)
  .bind(i64::from(limit) + 1)
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let has_more = rows.len() > usize::from(limit);
  rows.truncate(usize::from(limit));
  let next_after = has_more.then(|| {
    let row = rows.last().expect("a restore page with more rows cannot be empty");
    RestoreCacheCursor {
      scope_id: row.scope_id.clone(),
      blob_key: row.blob_key.clone(),
    }
  });
  let items = rows
    .into_iter()
    .map(|row| CacheBlobObject::new(row.scope_id, row.descriptor.0).map_err(|_| StoreError::Unavailable))
    .collect::<Result<Vec<_>, _>>()?;
  Ok(RestoreCacheBlobPage { items, next_after })
}

fn validate_limit(limit: u16) -> Result<(), StoreError> {
  if limit == 0 || limit > MAX_RESTORE_RECONCILIATION_BATCH_SIZE {
    Err(StoreError::Unavailable)
  } else {
    Ok(())
  }
}

fn unsigned(value: i64) -> Result<u64, StoreError> {
  u64::try_from(value).map_err(|_| StoreError::Unavailable)
}
