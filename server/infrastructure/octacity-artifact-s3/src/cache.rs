use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use octacity_server_cache::{
  CacheBlobObject, CacheBlobStore, CacheBlobStoreError, CacheBlobWrite, CacheIntegrityError, verify_blob_bytes,
};

use super::{
  MAX_SINGLE_OBJECT_BYTES, S3ArtifactStore,
  immutable::{ImmutableObjectError, is_conditional_conflict},
};

#[async_trait]
impl CacheBlobStore for S3ArtifactStore {
  async fn put_if_absent(
    &self,
    object: &CacheBlobObject,
    bytes: Vec<u8>,
  ) -> Result<CacheBlobWrite, CacheBlobStoreError> {
    verify_blob_bytes(object.descriptor(), &bytes)?;
    if object.descriptor().encoded_size_bytes > MAX_SINGLE_OBJECT_BYTES {
      return Err(CacheBlobStoreError::Unavailable);
    }
    let key = self.cache_key(object);
    let result = tokio::time::timeout(
      self.operation_timeout,
      self
        .client
        .put_object()
        .bucket(&self.bucket)
        .key(&key)
        .if_none_match("*")
        .content_type(octacity_server_cache::REMOTE_CACHE_BLOB_CONTENT_TYPE)
        .body(ByteStream::from(bytes.clone()))
        .send(),
    )
    .await
    .map_err(|_| CacheBlobStoreError::Unavailable)?;
    match result {
      Ok(_) => Ok(CacheBlobWrite::Written),
      Err(error) if is_conditional_conflict(&error) => {
        let existing = self.read(object).await?;
        if existing == bytes {
          Ok(CacheBlobWrite::AlreadyPresent)
        } else {
          Err(CacheBlobStoreError::Integrity(CacheIntegrityError::Digest))
        }
      }
      Err(_) => Err(CacheBlobStoreError::Unavailable),
    }
  }

  async fn read(&self, object: &CacheBlobObject) -> Result<Vec<u8>, CacheBlobStoreError> {
    let bytes = self.read_immutable(self.cache_key(object)).await.map_err(map_error)?;
    verify_blob_bytes(object.descriptor(), &bytes)?;
    Ok(bytes)
  }

  async fn delete(&self, object: &CacheBlobObject) -> Result<(), CacheBlobStoreError> {
    self.delete_immutable(self.cache_key(object)).await.map_err(map_error)
  }
}

fn map_error(error: ImmutableObjectError) -> CacheBlobStoreError {
  match error {
    ImmutableObjectError::NotFound => CacheBlobStoreError::NotFound,
    ImmutableObjectError::Unavailable => CacheBlobStoreError::Unavailable,
  }
}
