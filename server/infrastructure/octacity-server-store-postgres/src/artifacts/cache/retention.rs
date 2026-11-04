use octacity_server_cache::CacheBlobObject;
use octacity_server_store::{CacheDataAccess, CacheRetentionOutcome, StoreError};
use sqlx::{PgPool, types::Json};

use super::data_plane::authorize_data;
use crate::database::unavailable;

pub(crate) async fn prune(pool: &PgPool, access: CacheDataAccess) -> Result<CacheRetentionOutcome, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let scope = authorize_data(&mut transaction, &access, None).await?;
  sqlx::query(
    "DELETE FROM cache_actions
     WHERE scope_id = $1 AND retention_until <= to_timestamp($2::double precision / 1000.0)",
  )
  .bind(&scope.scope_id)
  .bind(access.observed_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let deleted: Vec<Json<octacity_server_cache::BlobDescriptor>> = sqlx::query_scalar(
    "DELETE FROM cache_blobs AS blob
     WHERE blob.scope_id = $1
       AND blob.retention_until <= to_timestamp($2::double precision / 1000.0)
       AND NOT EXISTS (
         SELECT 1 FROM cache_actions AS action
         WHERE action.scope_id = blob.scope_id AND action.blob_key = blob.blob_key
       )
     RETURNING descriptor",
  )
  .bind(&scope.scope_id)
  .bind(access.observed_at.unix_millis())
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  let deleted_blobs = deleted
    .into_iter()
    .map(|blob| CacheBlobObject::new(scope.scope_id.clone(), blob.0).map_err(|_| StoreError::Unavailable))
    .collect::<Result<_, _>>()?;
  Ok(CacheRetentionOutcome { deleted_blobs })
}
