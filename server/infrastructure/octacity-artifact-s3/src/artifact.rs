use std::{future::Future, time::Duration};

use async_trait::async_trait;
use aws_sdk_s3::{
  error::SdkError, operation::head_object::HeadObjectError, presigning::PresigningConfig, types::ChecksumMode,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_artifact_store::{
  ArtifactIntegrityError, ArtifactObject, ArtifactStore, ArtifactStoreError, ArtifactStoreOperation,
  DownloadAuthorization, InvalidArtifactStoreRequest, UploadAuthorization,
};
use sha2::{Digest as _, Sha256};

use super::{MAX_PRESIGNED_LIFETIME, MAX_SINGLE_OBJECT_BYTES, S3ArtifactStore, SHA256_METADATA, SIZE_METADATA};

impl S3ArtifactStore {
  pub(super) async fn verified_etag(
    &self,
    key: &str,
    object: &ArtifactObject,
    operation: ArtifactStoreOperation,
  ) -> Result<String, ArtifactStoreError> {
    let expected_checksum = STANDARD.encode(object.sha256_bytes());
    let head = self
      .client
      .head_object()
      .bucket(&self.bucket)
      .key(key)
      .checksum_mode(ChecksumMode::Enabled)
      .send()
      .await
      .map_err(|source| map_head_error(source, operation))?;
    let head_etag = head
      .e_tag()
      .ok_or(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::MissingGeneration,
      })?
      .to_owned();
    if head.content_length() != Some(object.size_bytes() as i64) {
      return Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::SizeMismatch,
      });
    }
    match head.checksum_sha256() {
      Some(checksum) if checksum == expected_checksum => Ok(head_etag),
      Some(_) => Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::DigestMismatch,
      }),
      None => {
        self.verify_body(key, &head_etag, object, operation).await?;
        Ok(head_etag)
      }
    }
  }

  /// Verifies one immutable object generation without buffering it in memory.
  ///
  /// Some compatible stores enforce the checksum supplied on PUT but omit it
  /// from HEAD. The conditional GET binds this fallback to the inspected
  /// generation, and the byte count rejects oversized responses early.
  async fn verify_body(
    &self,
    key: &str,
    etag: &str,
    object: &ArtifactObject,
    operation: ArtifactStoreOperation,
  ) -> Result<(), ArtifactStoreError> {
    let response = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(key)
      .if_match(etag)
      .send()
      .await
      .map_err(|source| backend(operation, source))?;
    let mut body = response.body;
    let mut verifier = BodyVerifier::new(object);
    while let Some(chunk) = body.next().await {
      let chunk = chunk.map_err(|source| backend(operation, source))?;
      verifier.update(&chunk)?;
    }
    verifier.finish()
  }

  pub(super) async fn within_operation<T>(
    &self,
    operation: ArtifactStoreOperation,
    future: impl Future<Output = Result<T, ArtifactStoreError>>,
  ) -> Result<T, ArtifactStoreError> {
    let result = match tokio::time::timeout(self.operation_timeout, future).await {
      Ok(result) => result,
      Err(_) => Err(ArtifactStoreError::TimedOut { operation }),
    };
    if matches!(
      result,
      Err(ArtifactStoreError::Backend { .. } | ArtifactStoreError::TimedOut { .. })
    ) {
      self.invalidate_capabilities().await;
    }
    result
  }

  async fn published_is_valid(
    &self,
    object: &ArtifactObject,
    operation: ArtifactStoreOperation,
  ) -> Result<bool, ArtifactStoreError> {
    let response = match self
      .client
      .head_object()
      .bucket(&self.bucket)
      .key(self.published_key(object))
      .send()
      .await
    {
      Ok(response) => response,
      Err(error) => match map_head_error(error, operation) {
        ArtifactStoreError::NotFound => return Ok(false),
        error => return Err(error),
      },
    };
    let metadata = response.metadata();
    let expected_size = object.size_bytes().to_string();
    if response.content_length() != Some(object.size_bytes() as i64)
      || metadata
        .and_then(|values| values.get(SHA256_METADATA))
        .map(String::as_str)
        != Some(object.sha256())
      || metadata
        .and_then(|values| values.get(SIZE_METADATA))
        .map(String::as_str)
        != Some(expected_size.as_str())
    {
      Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::PublishedMetadataMismatch,
      })
    } else {
      Ok(true)
    }
  }

  async fn delete_pending(
    &self,
    object: &ArtifactObject,
    operation: ArtifactStoreOperation,
  ) -> Result<(), ArtifactStoreError> {
    self
      .client
      .delete_object()
      .bucket(&self.bucket)
      .key(self.pending_key(object))
      .send()
      .await
      .map_err(|source| backend(operation, source))?;
    Ok(())
  }
}

/// Incremental verifier shared by the network fallback and deterministic unit
/// tests. It rejects excess bytes immediately and accepts a stream only after
/// both its final length and digest match the server-authorized object.
pub(super) struct BodyVerifier {
  expected_size: u64,
  expected_sha256: [u8; 32],
  bytes: u64,
  digest: Sha256,
}

impl BodyVerifier {
  pub(super) fn new(object: &ArtifactObject) -> Self {
    Self {
      expected_size: object.size_bytes(),
      expected_sha256: object.sha256_bytes(),
      bytes: 0,
      digest: Sha256::new(),
    }
  }

  pub(super) fn update(&mut self, chunk: &[u8]) -> Result<(), ArtifactStoreError> {
    self.bytes = self
      .bytes
      .checked_add(chunk.len() as u64)
      .ok_or(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::SizeOverflow,
      })?;
    if self.bytes > self.expected_size {
      return Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::SizeMismatch,
      });
    }
    self.digest.update(chunk);
    Ok(())
  }

  pub(super) fn finish(self) -> Result<(), ArtifactStoreError> {
    if self.bytes != self.expected_size || self.digest.finalize().as_slice() != self.expected_sha256 {
      return Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::BodyMismatch,
      });
    }
    Ok(())
  }
}

#[async_trait]
impl ArtifactStore for S3ArtifactStore {
  async fn authorize_upload(
    &self,
    object: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<UploadAuthorization, ArtifactStoreError> {
    self
      .within_operation(ArtifactStoreOperation::AuthorizeUpload, async {
        validate_s3_object(object)?;
        let presigning = presigning(expires_in, ArtifactStoreOperation::AuthorizeUpload)?;
        let checksum = STANDARD.encode(object.sha256_bytes());
        let request = self
          .client
          .put_object()
          .bucket(&self.bucket)
          .key(self.pending_key(object))
          .content_length(object.size_bytes() as i64)
          .content_type(object.content_type())
          .checksum_sha256(checksum)
          .metadata(SHA256_METADATA, object.sha256())
          .metadata(SIZE_METADATA, object.size_bytes().to_string())
          .presigned(presigning)
          .await
          .map_err(|source| backend(ArtifactStoreOperation::AuthorizeUpload, source))?;
        let required_headers = request
          .headers()
          .filter(|(name, _)| !name.eq_ignore_ascii_case("host") && !name.eq_ignore_ascii_case("content-length"))
          .map(|(name, value)| (name.to_owned(), value.to_owned()))
          .collect();
        Ok(UploadAuthorization {
          url: request.uri().to_owned(),
          required_headers,
          expires_in,
        })
      })
      .await
  }

  async fn complete_upload(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    self
      .within_operation(ArtifactStoreOperation::CompleteUpload, async {
        validate_s3_object(object)?;
        // A retry may observe publication from an earlier request whose
        // response was lost. Re-verify its bytes before accepting the replay.
        match self
          .verified_etag(
            &self.published_key(object),
            object,
            ArtifactStoreOperation::CompleteUpload,
          )
          .await
        {
          Ok(_) => {
            self
              .delete_pending(object, ArtifactStoreOperation::CompleteUpload)
              .await?;
            return Ok(());
          }
          Err(ArtifactStoreError::NotFound) => {}
          Err(error) => return Err(error),
        }
        let pending = self.pending_key(object);
        let etag = self
          .verified_etag(&pending, object, ArtifactStoreOperation::CompleteUpload)
          .await?;
        self
          .client
          .copy_object()
          .bucket(&self.bucket)
          .key(self.published_key(object))
          .copy_source(format!("{}/{pending}", self.bucket))
          .copy_source_if_match(etag)
          .content_type(object.content_type())
          .metadata(SHA256_METADATA, object.sha256())
          .metadata(SIZE_METADATA, object.size_bytes().to_string())
          .metadata_directive(aws_sdk_s3::types::MetadataDirective::Replace)
          .send()
          .await
          .map_err(|source| backend(ArtifactStoreOperation::CompleteUpload, source))?;
        if !self
          .published_is_valid(object, ArtifactStoreOperation::CompleteUpload)
          .await?
        {
          return Err(ArtifactStoreError::Integrity {
            reason: ArtifactIntegrityError::PublicationMismatch,
          });
        }
        self
          .delete_pending(object, ArtifactStoreOperation::CompleteUpload)
          .await
      })
      .await
  }

  async fn authorize_download(
    &self,
    object: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<DownloadAuthorization, ArtifactStoreError> {
    self
      .within_operation(ArtifactStoreOperation::AuthorizeDownload, async {
        validate_s3_object(object)?;
        if !self
          .published_is_valid(object, ArtifactStoreOperation::AuthorizeDownload)
          .await?
        {
          return Err(ArtifactStoreError::NotFound);
        }
        let request = self
          .client
          .get_object()
          .bucket(&self.bucket)
          .key(self.published_key(object))
          .presigned(presigning(expires_in, ArtifactStoreOperation::AuthorizeDownload)?)
          .await
          .map_err(|source| backend(ArtifactStoreOperation::AuthorizeDownload, source))?;
        Ok(DownloadAuthorization {
          url: request.uri().to_owned(),
          expires_in,
        })
      })
      .await
  }

  async fn delete(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    self
      .within_operation(ArtifactStoreOperation::Delete, async {
        validate_s3_object(object)?;
        self.delete_pending(object, ArtifactStoreOperation::Delete).await?;
        self
          .client
          .delete_object()
          .bucket(&self.bucket)
          .key(self.published_key(object))
          .send()
          .await
          .map_err(|source| backend(ArtifactStoreOperation::Delete, source))?;
        Ok(())
      })
      .await
  }
}

fn validate_s3_object(object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
  if object.size_bytes() > MAX_SINGLE_OBJECT_BYTES {
    return Err(ArtifactStoreError::Invalid {
      reason: InvalidArtifactStoreRequest::UnsupportedObjectSize,
    });
  }
  Ok(())
}

fn presigning(expires_in: Duration, operation: ArtifactStoreOperation) -> Result<PresigningConfig, ArtifactStoreError> {
  if expires_in.is_zero() || expires_in > MAX_PRESIGNED_LIFETIME {
    return Err(ArtifactStoreError::Invalid {
      reason: InvalidArtifactStoreRequest::InvalidAuthorizationLifetime,
    });
  }
  PresigningConfig::expires_in(expires_in).map_err(|source| backend(operation, source))
}

fn map_head_error(error: SdkError<HeadObjectError>, operation: ArtifactStoreOperation) -> ArtifactStoreError {
  if error.as_service_error().is_some_and(HeadObjectError::is_not_found) {
    ArtifactStoreError::NotFound
  } else {
    backend(operation, error)
  }
}

pub(super) fn backend(operation: ArtifactStoreOperation, _source: impl std::error::Error) -> ArtifactStoreError {
  ArtifactStoreError::Backend { operation }
}
