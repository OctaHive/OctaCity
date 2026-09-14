//! Server-side storage boundary for immutable job artifacts and reports.
//!
//! Coordinator code identifies an object by opaque OctaCity identifiers and
//! expected content metadata. Implementations alone decide bucket names and
//! physical object keys. This keeps S3 concepts and credentials out of both
//! the server-agent protocol and the server's upload state machine.

#![warn(missing_docs)]

use std::{collections::BTreeMap, time::Duration};

use async_trait::async_trait;
use thiserror::Error;

mod s3;

pub use s3::{S3ArtifactStore, S3ArtifactStoreConfig};

const MAX_ARTIFACT_IDENTIFIER_BYTES: usize = 256;
const MAX_CONTENT_TYPE_BYTES: usize = 256;

/// Immutable identity and expected bytes of one output upload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactObject {
  /// Opaque server-generated artifact or report identifier.
  pub artifact_id: String,
  /// Opaque server-generated upload-attempt identifier.
  pub upload_id: String,
  /// Exact number of bytes authorized for the single PUT.
  pub size_bytes: u64,
  /// Lowercase hexadecimal SHA-256 of the exact uploaded bytes.
  pub sha256: String,
  /// Transport media type, not the plugin-defined report format.
  pub content_type: String,
}

impl ArtifactObject {
  /// Rejects values that cannot safely identify a bounded immutable object.
  pub fn validate(&self) -> Result<(), ArtifactStoreError> {
    validate_identifier("artifact_id", &self.artifact_id)?;
    validate_identifier("upload_id", &self.upload_id)?;
    if self.size_bytes > i64::MAX as u64 {
      return Err(ArtifactStoreError::Invalid(
        "artifact size must fit a non-negative signed 64-bit content length".to_owned(),
      ));
    }
    validate_sha256(&self.sha256)?;
    if self.content_type.is_empty()
      || self.content_type.len() > MAX_CONTENT_TYPE_BYTES
      || self.content_type.chars().any(char::is_control)
    {
      return Err(ArtifactStoreError::Invalid(
        "artifact content type is invalid".to_owned(),
      ));
    }
    Ok(())
  }
}

/// Short-lived direct-upload capability returned to an agent.
#[derive(Clone, Eq, PartialEq)]
pub struct UploadAuthorization {
  /// Exact presigned PUT URL.
  pub url: String,
  /// Headers covered by the signature and required by the object store.
  pub required_headers: BTreeMap<String, String>,
  /// Actual authorization lifetime used by the storage implementation.
  pub expires_in: Duration,
}

/// Short-lived direct-download capability returned to an authorized client.
#[derive(Clone, Eq, PartialEq)]
pub struct DownloadAuthorization {
  /// Exact presigned GET URL.
  pub url: String,
  /// Actual authorization lifetime used by the storage implementation.
  pub expires_in: Duration,
}

/// Storage failure with no provider credentials or presigned URLs in display.
#[derive(Error)]
pub enum ArtifactStoreError {
  /// Domain input or adapter configuration is invalid.
  #[error("invalid artifact storage request: {0}")]
  Invalid(String),
  /// The uploaded bytes do not match the server-authorized object.
  #[error("uploaded artifact failed integrity verification: {0}")]
  Integrity(String),
  /// The requested uploaded or published object does not exist.
  #[error("artifact object was not found")]
  NotFound,
  /// A complete storage operation exceeded its server-owned deadline.
  #[error("artifact storage operation '{operation}' timed out")]
  TimedOut {
    /// Bounded operation name without endpoints, keys, or credentials.
    operation: &'static str,
  },
  /// The S3-compatible service rejected or failed an operation.
  #[error("artifact storage operation '{operation}' failed")]
  Backend {
    /// Bounded operation name without endpoints, keys, or credentials.
    operation: &'static str,
    /// Provider diagnostic retained for error inspection but not displayed.
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>,
  },
}

impl std::fmt::Debug for ArtifactStoreError {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    // SDK errors may retain a signed request URI. Keep Debug as safe as
    // Display so test panics and diagnostic wrappers cannot expose it.
    formatter
      .debug_tuple("ArtifactStoreError")
      .field(&self.to_string())
      .finish()
  }
}

/// Server-owned persistence port for artifact bytes.
///
/// `complete_upload` is the publication boundary: before it returns, the
/// implementation must independently verify the exact size and SHA-256 and
/// make future downloads refer only to immutable published bytes.
#[async_trait]
pub trait ArtifactStore: Send + Sync {
  /// Creates a short-lived single-PUT capability for a pending upload.
  async fn authorize_upload(
    &self,
    object: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<UploadAuthorization, ArtifactStoreError>;

  /// Verifies and atomically publishes an uploaded object, idempotently.
  async fn complete_upload(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError>;

  /// Creates a short-lived GET capability for an already published object.
  async fn authorize_download(
    &self,
    object: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<DownloadAuthorization, ArtifactStoreError>;

  /// Removes pending and published bytes for an object, idempotently.
  async fn delete(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError>;
}

impl std::fmt::Debug for UploadAuthorization {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("UploadAuthorization")
      .field("url", &"<redacted>")
      .field(
        "required_header_names",
        &self.required_headers.keys().collect::<Vec<_>>(),
      )
      .field("expires_in", &self.expires_in)
      .finish()
  }
}

impl std::fmt::Debug for DownloadAuthorization {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("DownloadAuthorization")
      .field("url", &"<redacted>")
      .field("expires_in", &self.expires_in)
      .finish()
  }
}

fn validate_identifier(name: &str, value: &str) -> Result<(), ArtifactStoreError> {
  if value.is_empty()
    || value.len() > MAX_ARTIFACT_IDENTIFIER_BYTES
    || !value
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
  {
    return Err(ArtifactStoreError::Invalid(format!("{name} is invalid")));
  }
  Ok(())
}

fn validate_sha256(value: &str) -> Result<(), ArtifactStoreError> {
  if value.len() != 64
    || !value
      .bytes()
      .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
  {
    return Err(ArtifactStoreError::Invalid(
      "artifact SHA-256 must be 64 lowercase hexadecimal characters".to_owned(),
    ));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn object_validation_rejects_unsafe_keys_and_unbounded_metadata() {
    let valid = ArtifactObject {
      artifact_id: "artifact_1".to_owned(),
      upload_id: "upload-1".to_owned(),
      size_bytes: 1,
      sha256: "a".repeat(64),
      content_type: "application/octet-stream".to_owned(),
    };
    assert!(valid.validate().is_ok());
    assert!(
      ArtifactObject {
        size_bytes: 0,
        ..valid.clone()
      }
      .validate()
      .is_ok()
    );

    for invalid in [
      ArtifactObject {
        artifact_id: "../escape".to_owned(),
        ..valid.clone()
      },
      ArtifactObject {
        size_bytes: u64::MAX,
        ..valid.clone()
      },
      ArtifactObject {
        sha256: "A".repeat(64),
        ..valid.clone()
      },
      ArtifactObject {
        content_type: "text/plain\nsecret".to_owned(),
        ..valid
      },
    ] {
      assert!(invalid.validate().is_err());
    }
  }

  #[test]
  fn debug_output_redacts_presigned_capabilities() {
    let upload = UploadAuthorization {
      url: "https://storage.example/object?signature=upload-secret".to_owned(),
      required_headers: BTreeMap::from([("x-private".to_owned(), "header-secret".to_owned())]),
      expires_in: Duration::from_secs(60),
    };
    let download = DownloadAuthorization {
      url: "https://storage.example/object?signature=download-secret".to_owned(),
      expires_in: Duration::from_secs(60),
    };

    let rendered = format!("{upload:?} {download:?}");
    assert!(!rendered.contains("upload-secret"));
    assert!(!rendered.contains("download-secret"));
    assert!(!rendered.contains("header-secret"));
    assert!(rendered.contains("x-private"));

    let backend = ArtifactStoreError::Backend {
      operation: "upload",
      source: Box::new(std::io::Error::other(
        "https://storage.example/object?signature=backend-secret",
      )),
    };
    assert!(!format!("{backend:?}").contains("backend-secret"));
  }
}
