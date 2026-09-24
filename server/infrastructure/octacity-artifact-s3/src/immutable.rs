use aws_sdk_s3::{
  error::{ProvideErrorMetadata as _, SdkError},
  operation::put_object::PutObjectError,
};

use super::S3ArtifactStore;

pub(super) enum ImmutableObjectError {
  NotFound,
  Unavailable,
}

impl S3ArtifactStore {
  pub(super) async fn read_immutable(&self, key: String) -> Result<Vec<u8>, ImmutableObjectError> {
    let response = tokio::time::timeout(
      self.operation_timeout,
      self.client.get_object().bucket(&self.bucket).key(key).send(),
    )
    .await
    .map_err(|_| ImmutableObjectError::Unavailable)?
    .map_err(|error| {
      if error.as_service_error().and_then(|value| value.code()) == Some("NoSuchKey") {
        ImmutableObjectError::NotFound
      } else {
        ImmutableObjectError::Unavailable
      }
    })?;
    response
      .body
      .collect()
      .await
      .map_err(|_| ImmutableObjectError::Unavailable)
      .map(|bytes| bytes.into_bytes().to_vec())
  }

  pub(super) async fn delete_immutable(&self, key: String) -> Result<(), ImmutableObjectError> {
    tokio::time::timeout(
      self.operation_timeout,
      self.client.delete_object().bucket(&self.bucket).key(key).send(),
    )
    .await
    .map_err(|_| ImmutableObjectError::Unavailable)?
    .map_err(|_| ImmutableObjectError::Unavailable)?;
    Ok(())
  }
}

pub(super) fn is_conditional_conflict(error: &SdkError<PutObjectError>) -> bool {
  matches!(
    error.as_service_error().and_then(|value| value.code()),
    Some("PreconditionFailed" | "ConditionalRequestConflict")
  )
}
