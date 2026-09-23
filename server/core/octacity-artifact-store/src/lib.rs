//! Server-side storage boundary for immutable job artifacts and reports.
//!
//! Coordinator code identifies an object by opaque OctaCity identifiers and
//! expected content metadata. Implementations alone decide storage namespaces
//! and physical locators. This keeps provider concepts and credentials out of
//! both the server-agent protocol and the server's upload state machine.

#![warn(missing_docs)]

use std::{collections::BTreeMap, time::Duration};

use async_trait::async_trait;
use octacity_server_artifacts::{ArtifactContentDigest, ArtifactMediaType};
pub use octacity_server_domain::{ArtifactId, ArtifactUploadId};
use thiserror::Error;

/// Immutable identity and expected bytes of one output upload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactObject {
  artifact_id: ArtifactId,
  upload_id: ArtifactUploadId,
  size_bytes: u64,
  sha256: String,
  digest: ArtifactContentDigest,
  content_type: ArtifactMediaType,
}

impl ArtifactObject {
  /// Creates a bounded immutable object description.
  pub fn new(
    artifact_id: ArtifactId,
    upload_id: ArtifactUploadId,
    size_bytes: u64,
    sha256: impl Into<String>,
    content_type: impl Into<String>,
  ) -> Result<Self, ArtifactObjectError> {
    let sha256 = sha256.into();
    let digest = ArtifactContentDigest::from_lower_hex(&sha256).map_err(|_| ArtifactObjectError::InvalidSha256)?;
    let content_type = ArtifactMediaType::new(content_type).map_err(|_| ArtifactObjectError::InvalidContentType)?;

    Ok(Self {
      artifact_id,
      upload_id,
      size_bytes,
      sha256,
      digest,
      content_type,
    })
  }

  /// Returns the server-owned logical artifact identity.
  #[must_use]
  pub const fn artifact_id(&self) -> ArtifactId {
    self.artifact_id
  }

  /// Returns the server-owned upload-attempt identity.
  #[must_use]
  pub const fn upload_id(&self) -> ArtifactUploadId {
    self.upload_id
  }

  /// Returns the exact authorized byte length.
  #[must_use]
  pub const fn size_bytes(&self) -> u64 {
    self.size_bytes
  }

  /// Returns the lowercase hexadecimal SHA-256 content identity.
  #[must_use]
  pub fn sha256(&self) -> &str {
    &self.sha256
  }

  /// Returns the decoded SHA-256 content identity.
  #[must_use]
  pub const fn sha256_bytes(&self) -> [u8; 32] {
    self.digest.as_bytes()
  }

  /// Returns the transport media type, not a report format.
  #[must_use]
  pub fn content_type(&self) -> &str {
    self.content_type.as_str()
  }
}

/// Invalid input rejected while constructing an [`ArtifactObject`].
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ArtifactObjectError {
  /// The digest is not exactly 64 lowercase hexadecimal characters.
  #[error("artifact SHA-256 must be 64 lowercase hexadecimal characters")]
  InvalidSha256,
  /// The media type is empty, untrimmed, oversized, or contains control characters.
  #[error("artifact content type is invalid")]
  InvalidContentType,
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

/// Artifact-store operation exposed in safe diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactStoreOperation {
  /// Create an upload capability.
  AuthorizeUpload,
  /// Verify and publish an upload.
  CompleteUpload,
  /// Create a download capability.
  AuthorizeDownload,
  /// Delete pending and published bytes.
  Delete,
}

impl std::fmt::Display for ArtifactStoreOperation {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(match self {
      Self::AuthorizeUpload => "authorize upload",
      Self::CompleteUpload => "complete upload",
      Self::AuthorizeDownload => "authorize download",
      Self::Delete => "delete object",
    })
  }
}

/// Safe classification of an invalid artifact-store request.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InvalidArtifactStoreRequest {
  /// The requested object exceeds an adapter's atomic object limit.
  #[error("object size is unsupported by the configured backend")]
  UnsupportedObjectSize,
  /// The requested authorization lifetime is outside the supported range.
  #[error("authorization lifetime is outside the supported range")]
  InvalidAuthorizationLifetime,
}

/// Safe classification of failed artifact integrity verification.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ArtifactIntegrityError {
  /// The backend omitted the immutable generation identity.
  #[error("object generation identity is missing")]
  MissingGeneration,
  /// The stored byte length differs from the authorized length.
  #[error("stored object size differs from the authorized size")]
  SizeMismatch,
  /// The stored digest differs from the authorized digest.
  #[error("stored object digest differs from the authorized digest")]
  DigestMismatch,
  /// Published metadata differs from the immutable logical identity.
  #[error("published object metadata differs from its immutable identity")]
  PublishedMetadataMismatch,
  /// The byte count overflowed while a streamed object was verified.
  #[error("object size overflowed during verification")]
  SizeOverflow,
  /// The streamed body differs from the authorized size or digest.
  #[error("object body differs from the authorized size or digest")]
  BodyMismatch,
  /// The backend did not expose the object after publication.
  #[error("object was not visible after publication")]
  PublicationMismatch,
}

/// Storage failure containing only bounded, non-sensitive classifications.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ArtifactStoreError {
  /// A valid core object cannot be served under the requested adapter limits.
  #[error("invalid artifact storage request: {reason}")]
  Invalid {
    /// Stable reason safe for logs and error-chain reporters.
    reason: InvalidArtifactStoreRequest,
  },
  /// The uploaded bytes do not match the server-authorized object.
  #[error("uploaded artifact failed integrity verification: {reason}")]
  Integrity {
    /// Stable reason safe for logs and error-chain reporters.
    reason: ArtifactIntegrityError,
  },
  /// The requested uploaded or published object does not exist.
  #[error("artifact object was not found")]
  NotFound,
  /// A complete storage operation exceeded its server-owned deadline.
  #[error("artifact storage operation '{operation}' timed out")]
  TimedOut {
    /// Closed operation classification without endpoints or object keys.
    operation: ArtifactStoreOperation,
  },
  /// The configured storage backend rejected or failed an operation.
  #[error("artifact storage operation '{operation}' failed")]
  Backend {
    /// Closed operation classification without provider diagnostics.
    operation: ArtifactStoreOperation,
  },
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn object_construction_rejects_unsafe_or_unbounded_metadata() {
    let artifact_id = ArtifactId::generate();
    let upload_id = ArtifactUploadId::generate();
    let valid = ArtifactObject::new(artifact_id, upload_id, 0, "a".repeat(64), "application/octet-stream").unwrap();
    assert_eq!(valid.artifact_id(), artifact_id);
    assert_eq!(valid.upload_id(), upload_id);
    assert_eq!(valid.size_bytes(), 0);
    assert_eq!(valid.sha256_bytes(), [0xaa; 32]);

    assert_eq!(
      ArtifactObject::new(artifact_id, upload_id, 1, "A".repeat(64), "application/octet-stream"),
      Err(ArtifactObjectError::InvalidSha256)
    );
    assert_eq!(
      ArtifactObject::new(artifact_id, upload_id, 1, "a".repeat(64), "text/plain\nsecret"),
      Err(ArtifactObjectError::InvalidContentType)
    );
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
      operation: ArtifactStoreOperation::AuthorizeUpload,
    };
    assert!(std::error::Error::source(&backend).is_none());
    assert_eq!(
      backend.to_string(),
      "artifact storage operation 'authorize upload' failed"
    );
  }
}
