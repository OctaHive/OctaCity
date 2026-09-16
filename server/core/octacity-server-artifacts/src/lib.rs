//! Logical artifact lifecycle and policy.
//!
//! This module owns upload, publication, download, fencing, and retention
//! decisions independently of any concrete byte-store implementation.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use octacity_server_domain::{EntityKind, TransitionError};

/// Durable publication and retention state of one logical Artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArtifactState {
  /// Metadata is reserved but no verified object is published.
  Pending,
  /// Exact object generation, size, and digest are being verified.
  Verifying,
  /// Verified immutable bytes are logically visible.
  Published,
  /// Logical visibility is removed and physical deletion is pending.
  Expired,
  /// Metadata and physical bytes have completed idempotent deletion.
  Deleted,
}

/// Fact applied to an [`ArtifactState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArtifactEvent {
  /// Begin independent verification of the uploaded generation.
  BeginVerification,
  /// Verification failed and the logical Artifact remains unpublished.
  VerificationFailed,
  /// Verification succeeded and publication committed.
  Publish,
  /// Retention removes logical visibility before deleting bytes.
  Expire,
  /// Delete an abandoned pending upload or an expired Artifact.
  Delete,
}

impl ArtifactState {
  /// Applies one fact without accessing an object store or persistence.
  pub fn transition(self, event: ArtifactEvent) -> Result<Self, TransitionError<Self, ArtifactEvent>> {
    match (self, event) {
      (Self::Pending, ArtifactEvent::BeginVerification) => Ok(Self::Verifying),
      (Self::Pending | Self::Expired, ArtifactEvent::Delete) => Ok(Self::Deleted),
      (Self::Verifying, ArtifactEvent::VerificationFailed) => Ok(Self::Pending),
      (Self::Verifying, ArtifactEvent::Publish) => Ok(Self::Published),
      (Self::Published, ArtifactEvent::Expire) => Ok(Self::Expired),
      _ => Err(TransitionError::new(EntityKind::Artifact, self, event)),
    }
  }

  /// Reports whether bytes may be returned to a management client.
  #[must_use]
  pub const fn is_visible(self) -> bool {
    matches!(self, Self::Published)
  }
}
