//! S3-compatible artifact adapter with a private pending-to-published boundary.
//!
//! Agents upload only to a temporary key with a signed SHA-256 checksum. When
//! the object store exposes that verified checksum, completion validates it
//! with a metadata request. Compatible stores that omit checksums from HEAD
//! are verified by streaming the exact object generation through SHA-256; the
//! expected metadata is never treated as proof of the stored bytes. The exact
//! generation is then conditionally copied to a key derived from the immutable
//! artifact identity and digest. A still-live PUT capability therefore cannot
//! mutate a published artifact.

use std::{future::Future, sync::Arc, time::Duration};

use async_trait::async_trait;
use aws_sdk_s3::{
  Client,
  config::{BehaviorVersion, Credentials, Region},
  error::SdkError,
  operation::head_object::HeadObjectError,
  presigning::PresigningConfig,
  types::ChecksumMode,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest as _, Sha256};

mod config;
mod health;

use config::validate_config;
pub use config::{S3ArtifactStoreConfig, S3ArtifactStoreConfigError, validate_s3_settings};
pub use health::S3ArtifactStoreHealthError;

use octacity_artifact_store::{
  ArtifactIntegrityError, ArtifactObject, ArtifactStore, ArtifactStoreError, ArtifactStoreOperation,
  DownloadAuthorization, InvalidArtifactStoreRequest, UploadAuthorization,
};

const SHA256_METADATA: &str = "octacity-sha256";
const SIZE_METADATA: &str = "octacity-size";
const MAX_PRESIGNED_LIFETIME: Duration = Duration::from_secs(60 * 60);
/// Maximum object size supported by one S3 PUT and one S3 CopyObject request.
const MAX_SINGLE_OBJECT_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// S3-compatible object store that never exposes credentials or physical keys.
#[derive(Clone)]
pub struct S3ArtifactStore {
  client: Client,
  bucket: String,
  prefix: String,
  health_probe_prefix: String,
  operation_timeout: Duration,
  capability_recheck_interval: Duration,
  capability_state: Arc<tokio::sync::Mutex<health::CapabilityState>>,
}

impl S3ArtifactStore {
  /// Validates configuration and creates an isolated S3 client.
  pub fn new(config: S3ArtifactStoreConfig) -> Result<Self, S3ArtifactStoreConfigError> {
    validate_config(&config)?;
    let credentials = Credentials::new(
      config.access_key.to_string(),
      config.secret_key.to_string(),
      None,
      None,
      "octacity-server",
    );
    let sdk_config = aws_sdk_s3::Config::builder()
      .behavior_version(BehaviorVersion::latest())
      .endpoint_url(config.endpoint)
      .region(Region::new(config.region))
      .credentials_provider(credentials)
      .force_path_style(config.force_path_style)
      .build();
    let health_probe_prefix = physical_key(
      &config.prefix,
      &format!("health/readiness-{}", uuid::Uuid::new_v4().simple()),
    );
    Ok(Self {
      client: Client::from_conf(sdk_config),
      bucket: config.bucket,
      prefix: config.prefix,
      health_probe_prefix,
      operation_timeout: config.operation_timeout,
      capability_recheck_interval: config.capability_recheck_interval,
      capability_state: Arc::new(tokio::sync::Mutex::new(health::CapabilityState::default())),
    })
  }

  fn pending_key(&self, object: &ArtifactObject) -> String {
    self.key(&format!("uploads/{}", object.upload_id()))
  }

  fn published_key(&self, object: &ArtifactObject) -> String {
    self.key(&format!("objects/{}/{}", object.artifact_id(), object.sha256()))
  }

  fn key(&self, suffix: &str) -> String {
    physical_key(&self.prefix, suffix)
  }

  async fn verified_etag(
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
  /// Some S3-compatible stores enforce the checksum supplied on PUT but omit
  /// it from HEAD. The conditional GET binds this fallback to the generation
  /// inspected above, while the byte count prevents an oversized response from
  /// consuming unbounded network bandwidth before failure is reported.
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

  async fn within_operation<T>(
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
}

fn physical_key(prefix: &str, suffix: &str) -> String {
  if prefix.is_empty() {
    suffix.to_owned()
  } else {
    format!("{prefix}/{suffix}")
  }
}

/// Incremental verifier shared by the network fallback and deterministic unit
/// tests. It rejects excess bytes immediately and accepts a stream only after
/// both its final length and digest match the server-authorized object.
struct BodyVerifier {
  expected_size: u64,
  expected_sha256: [u8; 32],
  bytes: u64,
  digest: Sha256,
}

impl BodyVerifier {
  fn new(object: &ArtifactObject) -> Self {
    Self {
      expected_size: object.size_bytes(),
      expected_sha256: object.sha256_bytes(),
      bytes: 0,
      digest: Sha256::new(),
    }
  }

  fn update(&mut self, chunk: &[u8]) -> Result<(), ArtifactStoreError> {
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

  fn finish(self) -> Result<(), ArtifactStoreError> {
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
        // A retry may observe an object copied by an earlier request whose
        // response was lost. Re-verify its bytes before accepting it as the
        // publication boundary; metadata alone is sufficient only after this
        // method has established the immutable object.
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

impl S3ArtifactStore {
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

fn backend(operation: ArtifactStoreOperation, _source: impl std::error::Error) -> ArtifactStoreError {
  ArtifactStoreError::Backend { operation }
}

#[cfg(test)]
mod tests;
