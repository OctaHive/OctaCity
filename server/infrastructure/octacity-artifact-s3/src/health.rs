use std::time::Instant;

use aws_sdk_s3::{primitives::ByteStream, types::MetadataDirective};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use super::{S3ArtifactStore, SHA256_METADATA, SIZE_METADATA};

#[derive(Debug, Default)]
pub(super) struct CapabilityState {
  pub(super) qualified_at: Option<Instant>,
}

impl CapabilityState {
  fn requires_full_probe(&self, interval: std::time::Duration) -> bool {
    self
      .qualified_at
      .is_none_or(|qualified_at| qualified_at.elapsed() >= interval)
  }
}

/// Safe classification returned by the object-store readiness check.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum S3ArtifactStoreHealthError {
  /// The object store did not answer before the configured operation deadline.
  #[error("S3 health check timed out")]
  TimedOut,
  /// The configured identity could not complete the required object lifecycle.
  #[error("S3 capability health check failed")]
  Unavailable,
}

impl S3ArtifactStore {
  /// Verifies object availability and periodically revalidates every capability
  /// required for safe artifact mutations.
  ///
  /// A full qualification creates a process-unique zero-byte marker and proves
  /// PUT, GET, COPY, GET, and DELETE. Checks before the configured
  /// requalification deadline issue `HeadObject` for that marker, which avoids
  /// requiring bucket-list permission. Failed artifact operations and failed
  /// probes invalidate the qualification.
  pub async fn health_check(&self) -> Result<(), S3ArtifactStoreHealthError> {
    match tokio::time::timeout(self.operation_timeout, self.check_health()).await {
      Ok(result) => result,
      Err(_) => {
        // Cancellation releases the guard when this call owned the probe. If
        // another caller owns it, that caller must publish its own result;
        // waiting here would let one health check exceed its stated deadline.
        if let Ok(mut state) = self.capability_state.try_lock() {
          state.qualified_at = None;
        }
        Err(S3ArtifactStoreHealthError::TimedOut)
      }
    }
  }

  async fn check_health(&self) -> Result<(), S3ArtifactStoreHealthError> {
    // Serialize qualification and invalidation so an older successful probe
    // cannot overwrite a concurrent object-operation failure.
    let mut state = self.capability_state.lock().await;
    let requires_full_probe = state.requires_full_probe(self.capability_recheck_interval);
    let result = if requires_full_probe {
      self.probe_capabilities().await
    } else {
      self.probe_availability().await
    };
    match result {
      Ok(()) => {
        if requires_full_probe {
          state.qualified_at = Some(Instant::now());
        }
        Ok(())
      }
      Err(error) => {
        state.qualified_at = None;
        Err(error)
      }
    }
  }

  pub(super) async fn invalidate_capabilities(&self) {
    self.capability_state.lock().await.qualified_at = None;
  }

  async fn probe_availability(&self) -> Result<(), S3ArtifactStoreHealthError> {
    self
      .client
      .head_object()
      .bucket(&self.bucket)
      .key(self.health_probe_source_key())
      .send()
      .await
      .map_err(|_| S3ArtifactStoreHealthError::Unavailable)?;
    Ok(())
  }

  async fn probe_capabilities(&self) -> Result<(), S3ArtifactStoreHealthError> {
    let source = self.health_probe_source_key();
    let copy = format!("{}/copy", self.health_probe_prefix);
    let empty_digest = Sha256::digest([]);
    let checksum = STANDARD.encode(empty_digest);
    let digest = format!("{empty_digest:x}");
    self
      .client
      .put_object()
      .bucket(&self.bucket)
      .key(&source)
      .content_length(0)
      .content_type("application/octet-stream")
      .checksum_sha256(&checksum)
      .metadata(SHA256_METADATA, &digest)
      .metadata(SIZE_METADATA, "0")
      .body(ByteStream::from_static(b""))
      .send()
      .await
      .map_err(|_| S3ArtifactStoreHealthError::Unavailable)?;

    let result = async {
      self.read_probe(&source).await?;
      self
        .client
        .copy_object()
        .bucket(&self.bucket)
        .key(&copy)
        .copy_source(format!("{}/{source}", self.bucket))
        .content_type("application/octet-stream")
        .metadata(SHA256_METADATA, &digest)
        .metadata(SIZE_METADATA, "0")
        .metadata_directive(MetadataDirective::Replace)
        .send()
        .await
        .map_err(|_| S3ArtifactStoreHealthError::Unavailable)?;
      self.read_probe(&copy).await?;
      self.delete_probe(&copy).await
    }
    .await;

    if result.is_err() {
      // Cleanup is best effort on failure; the original capability failure is
      // what readiness must report. Lifecycle policy removes any leftovers.
      let _ = self.delete_probe(&copy).await;
      let _ = self.delete_probe(&source).await;
    }
    result
  }

  fn health_probe_source_key(&self) -> String {
    format!("{}/source", self.health_probe_prefix)
  }

  async fn read_probe(&self, key: &str) -> Result<(), S3ArtifactStoreHealthError> {
    let object = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(key)
      .send()
      .await
      .map_err(|_| S3ArtifactStoreHealthError::Unavailable)?;
    let bytes = object
      .body
      .collect()
      .await
      .map_err(|_| S3ArtifactStoreHealthError::Unavailable)?;
    if bytes.into_bytes().is_empty() {
      Ok(())
    } else {
      Err(S3ArtifactStoreHealthError::Unavailable)
    }
  }

  async fn delete_probe(&self, key: &str) -> Result<(), S3ArtifactStoreHealthError> {
    self
      .client
      .delete_object()
      .bucket(&self.bucket)
      .key(key)
      .send()
      .await
      .map_err(|_| S3ArtifactStoreHealthError::Unavailable)?;
    Ok(())
  }
}
