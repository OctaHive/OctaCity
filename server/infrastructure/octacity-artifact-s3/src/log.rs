use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_artifact_store::{LogChunkStore, LogChunkStoreError, LogChunkWrite};
use octacity_server_artifacts::LogChunkManifest;

use super::{
  S3ArtifactStore, SHA256_METADATA, SIZE_METADATA,
  immutable::{ImmutableObjectError, is_conditional_conflict},
};

#[async_trait]
impl LogChunkStore for S3ArtifactStore {
  async fn put_verified(
    &self,
    manifest: &LogChunkManifest,
    bytes: Vec<u8>,
  ) -> Result<LogChunkWrite, LogChunkStoreError> {
    manifest.verify(&bytes).map_err(|_| LogChunkStoreError::Integrity)?;
    let key = self.log_chunk_key(manifest);
    let result = tokio::time::timeout(
      self.operation_timeout,
      self
        .client
        .put_object()
        .bucket(&self.bucket)
        .key(&key)
        .if_none_match("*")
        .content_length(manifest.byte_length() as i64)
        .content_type("text/plain; charset=utf-8")
        .checksum_sha256(STANDARD.encode(manifest.digest().as_bytes()))
        .metadata(SHA256_METADATA, manifest.digest().to_string())
        .metadata(SIZE_METADATA, manifest.byte_length().to_string())
        .body(ByteStream::from(bytes.clone()))
        .send(),
    )
    .await
    .map_err(|_| LogChunkStoreError::Unavailable)?;
    let disposition = match result {
      Ok(_) => LogChunkWrite::Written,
      Err(error) if is_conditional_conflict(&error) => LogChunkWrite::AlreadyPresent,
      Err(_) => {
        self.invalidate_capabilities().await;
        return Err(LogChunkStoreError::Unavailable);
      }
    };
    let stored = self.read_verified(manifest).await?;
    if stored != bytes {
      return Err(LogChunkStoreError::Integrity);
    }
    Ok(disposition)
  }

  async fn read_verified(&self, manifest: &LogChunkManifest) -> Result<Vec<u8>, LogChunkStoreError> {
    let bytes = self
      .read_immutable(self.log_chunk_key(manifest))
      .await
      .map_err(map_error)?;
    manifest.verify(&bytes).map_err(|_| LogChunkStoreError::Integrity)?;
    Ok(bytes)
  }

  async fn delete_chunk(&self, manifest: &LogChunkManifest) -> Result<(), LogChunkStoreError> {
    self
      .delete_immutable(self.log_chunk_key(manifest))
      .await
      .map_err(map_error)
  }
}

fn map_error(error: ImmutableObjectError) -> LogChunkStoreError {
  match error {
    ImmutableObjectError::NotFound => LogChunkStoreError::NotFound,
    ImmutableObjectError::Unavailable => LogChunkStoreError::Unavailable,
  }
}
