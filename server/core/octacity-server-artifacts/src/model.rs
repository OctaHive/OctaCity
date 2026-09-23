use std::fmt;

use octacity_server_domain::{
  ArtifactId, ArtifactName, ArtifactVersion, AttemptId, BuildId, JobId, LeaseId, Timestamp,
};
use thiserror::Error;

use crate::{ArtifactEvent, ArtifactState};

/// Maximum UTF-8 bytes in an Artifact media type.
pub const MAX_ARTIFACT_MEDIA_TYPE_BYTES: usize = 256;
/// Maximum UTF-8 bytes in an open report-format identifier.
pub const MAX_ARTIFACT_REPORT_FORMAT_BYTES: usize = 256;

/// Exact SHA-256 identity of immutable Artifact bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactContentDigest([u8; 32]);

impl ArtifactContentDigest {
  /// Constructs a digest from its exact binary representation.
  #[must_use]
  pub const fn from_bytes(value: [u8; 32]) -> Self {
    Self(value)
  }

  /// Parses exactly 64 lowercase hexadecimal characters.
  pub fn from_lower_hex(value: &str) -> Result<Self, ArtifactRecordError> {
    if value.len() != 64
      || !value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
      return Err(ArtifactRecordError::InvalidDigest);
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
      digest[index] = (hex_digit(pair[0]) << 4) | hex_digit(pair[1]);
    }
    Ok(Self(digest))
  }

  /// Returns the binary SHA-256 representation.
  #[must_use]
  pub const fn as_bytes(self) -> [u8; 32] {
    self.0
  }
}

impl fmt::Display for ArtifactContentDigest {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    for byte in self.0 {
      write!(formatter, "{byte:02x}")?;
    }
    Ok(())
  }
}

macro_rules! bounded_text {
  ($name:ident, $limit:expr, $error:ident, $documentation:literal) => {
    #[doc = $documentation]
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct $name(String);

    impl $name {
      /// Constructs a trimmed, control-free bounded value.
      pub fn new(value: impl Into<String>) -> Result<Self, ArtifactRecordError> {
        let value = value.into();
        if value.is_empty() || value.len() > $limit || value.trim() != value || value.chars().any(char::is_control) {
          return Err(ArtifactRecordError::$error);
        }
        Ok(Self(value))
      }

      /// Borrows the validated text.
      #[must_use]
      pub fn as_str(&self) -> &str {
        &self.0
      }
    }
  };
}

bounded_text!(
  ArtifactMediaType,
  MAX_ARTIFACT_MEDIA_TYPE_BYTES,
  InvalidMediaType,
  "Bounded media type used for logical or transport Artifact metadata."
);
bounded_text!(
  ArtifactReportFormat,
  MAX_ARTIFACT_REPORT_FORMAT_BYTES,
  InvalidReportFormat,
  "Bounded open format identifier of a machine-readable report."
);

/// Logical output semantics independent of its media type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactType {
  /// A user-visible file or archive.
  Artifact,
  /// A machine-readable report in the contained open format.
  Report(ArtifactReportFormat),
}

impl ArtifactType {
  /// Stable persistence representation of the output type.
  #[must_use]
  pub const fn as_str(&self) -> &'static str {
    match self {
      Self::Artifact => "artifact",
      Self::Report(_) => "report",
    }
  }

  /// Returns the report format when this output is a report.
  #[must_use]
  pub fn report_format(&self) -> Option<&ArtifactReportFormat> {
    match self {
      Self::Artifact => None,
      Self::Report(format) => Some(format),
    }
  }
}

/// Logical retention decision attached to an Artifact at reservation time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactRetentionPolicy {
  /// Keep the Artifact until an explicit policy replacement is implemented.
  Keep,
  /// Remove logical visibility at or after the given instant.
  DeleteAfter(Timestamp),
}

impl ArtifactRetentionPolicy {
  /// Returns the optional retention deadline used by persistence adapters.
  #[must_use]
  pub const fn delete_after(self) -> Option<Timestamp> {
    match self {
      Self::Keep => None,
      Self::DeleteAfter(value) => Some(value),
    }
  }
}

/// Immutable logical and content identity of one Artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactIdentity {
  /// Server-owned logical Artifact identity.
  pub artifact_id: ArtifactId,
  /// Build that owns the output.
  pub build_id: BuildId,
  /// Attempt that produced the output.
  pub attempt_id: AttemptId,
  /// Job that produced the output.
  pub job_id: JobId,
  /// Lease that reserved the output under its then-current fence.
  pub lease_id: LeaseId,
  /// User-visible name unique inside the Job.
  pub logical_name: ArtifactName,
  /// Logical output semantics.
  pub artifact_type: ArtifactType,
  /// Logical media type preserved from the output declaration.
  pub media_type: ArtifactMediaType,
  /// Exact immutable byte length.
  pub size_bytes: u64,
  /// Exact immutable SHA-256 digest.
  pub digest: ArtifactContentDigest,
  /// Retention decision captured at reservation time.
  pub retention: ArtifactRetentionPolicy,
}

/// Durable logical Artifact record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactRecord {
  identity: ArtifactIdentity,
  state: ArtifactState,
  version: ArtifactVersion,
  created_at: Timestamp,
  published_at: Option<Timestamp>,
  deleted_at: Option<Timestamp>,
}

impl ArtifactRecord {
  /// Creates the initial unpublished record after a current fenced Lease is authorized.
  pub fn pending(identity: ArtifactIdentity, created_at: Timestamp) -> Result<Self, ArtifactRecordError> {
    if identity
      .retention
      .delete_after()
      .is_some_and(|deadline| deadline <= created_at)
    {
      return Err(ArtifactRecordError::InvalidRetention);
    }
    Ok(Self {
      identity,
      state: ArtifactState::Pending,
      version: ArtifactVersion::INITIAL,
      created_at,
      published_at: None,
      deleted_at: None,
    })
  }

  /// Restores a persisted record while revalidating its complete shape.
  pub fn restore(
    identity: ArtifactIdentity,
    state: ArtifactState,
    version: ArtifactVersion,
    created_at: Timestamp,
    published_at: Option<Timestamp>,
    deleted_at: Option<Timestamp>,
  ) -> Result<Self, ArtifactRecordError> {
    let record = Self {
      identity,
      state,
      version,
      created_at,
      published_at,
      deleted_at,
    };
    record.validate_shape()?;
    Ok(record)
  }

  /// Applies a Rust-owned state decision with immutable identity and optimistic version preconditions.
  pub fn transition(
    &self,
    expected_identity: &ArtifactIdentity,
    expected_version: ArtifactVersion,
    event: ArtifactEvent,
    transitioned_at: Timestamp,
  ) -> Result<Self, ArtifactRecordError> {
    if &self.identity != expected_identity {
      return Err(ArtifactRecordError::ImmutableIdentityMismatch);
    }
    if self.version != expected_version {
      return Err(ArtifactRecordError::VersionConflict);
    }
    if transitioned_at < self.created_at
      || self
        .published_at
        .is_some_and(|published_at| transitioned_at < published_at)
    {
      return Err(ArtifactRecordError::InvalidTransitionTime);
    }
    if event == ArtifactEvent::Expire {
      match self.identity.retention {
        ArtifactRetentionPolicy::Keep => return Err(ArtifactRecordError::RetentionNotReached),
        ArtifactRetentionPolicy::DeleteAfter(deadline) if transitioned_at < deadline => {
          return Err(ArtifactRecordError::RetentionNotReached);
        }
        ArtifactRetentionPolicy::DeleteAfter(_) => {}
      }
    }

    let state = self
      .state
      .transition(event)
      .map_err(|_| ArtifactRecordError::InvalidTransition)?;
    let mut updated = self.clone();
    updated.state = state;
    updated.version = self.version.next().map_err(|_| ArtifactRecordError::VersionOverflow)?;
    if event == ArtifactEvent::Publish {
      updated.published_at = Some(transitioned_at);
    }
    if event == ArtifactEvent::Delete {
      updated.deleted_at = Some(transitioned_at);
    }
    updated.validate_shape()?;
    Ok(updated)
  }

  /// Returns the immutable Artifact identity.
  #[must_use]
  pub const fn identity(&self) -> &ArtifactIdentity {
    &self.identity
  }

  /// Returns the current lifecycle state.
  #[must_use]
  pub const fn state(&self) -> ArtifactState {
    self.state
  }

  /// Returns the optimistic lifecycle version.
  #[must_use]
  pub const fn version(&self) -> ArtifactVersion {
    self.version
  }

  /// Returns the reservation time.
  #[must_use]
  pub const fn created_at(&self) -> Timestamp {
    self.created_at
  }

  /// Returns the publication time once verified bytes become visible.
  #[must_use]
  pub const fn published_at(&self) -> Option<Timestamp> {
    self.published_at
  }

  /// Returns the physical-deletion completion time.
  #[must_use]
  pub const fn deleted_at(&self) -> Option<Timestamp> {
    self.deleted_at
  }

  fn validate_shape(&self) -> Result<(), ArtifactRecordError> {
    if self
      .identity
      .retention
      .delete_after()
      .is_some_and(|deadline| deadline <= self.created_at)
    {
      return Err(ArtifactRecordError::InvalidRetention);
    }
    let timestamps_match_state = match self.state {
      ArtifactState::Pending | ArtifactState::Verifying => self.published_at.is_none() && self.deleted_at.is_none(),
      ArtifactState::Published | ArtifactState::Expired => self.published_at.is_some() && self.deleted_at.is_none(),
      ArtifactState::Deleted => self.deleted_at.is_some(),
    };
    if !timestamps_match_state
      || self.published_at.is_some_and(|value| value < self.created_at)
      || self.deleted_at.is_some_and(|value| value < self.created_at)
      || self
        .published_at
        .zip(self.deleted_at)
        .is_some_and(|(published, deleted)| deleted < published)
    {
      return Err(ArtifactRecordError::InvalidPersistedShape);
    }
    Ok(())
  }
}

/// Typed rejection from Artifact value construction or lifecycle decisions.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ArtifactRecordError {
  /// A digest was not exactly 64 lowercase hexadecimal characters.
  #[error("artifact digest is invalid")]
  InvalidDigest,
  /// A media type was empty, untrimmed, oversized, or contained a control character.
  #[error("artifact media type is invalid")]
  InvalidMediaType,
  /// A report format was empty, untrimmed, oversized, or contained a control character.
  #[error("artifact report format is invalid")]
  InvalidReportFormat,
  /// A finite retention deadline did not follow the reservation time.
  #[error("artifact retention policy is invalid")]
  InvalidRetention,
  /// Persisted state and lifecycle timestamps were inconsistent.
  #[error("persisted artifact lifecycle shape is invalid")]
  InvalidPersistedShape,
  /// The requested event is not legal from the current state.
  #[error("artifact state transition is invalid")]
  InvalidTransition,
  /// The requested transition preceded an existing lifecycle timestamp.
  #[error("artifact transition time is invalid")]
  InvalidTransitionTime,
  /// A retention transition was attempted before its policy allowed it.
  #[error("artifact retention deadline has not been reached")]
  RetentionNotReached,
  /// A caller attempted to change immutable identity or content metadata.
  #[error("artifact immutable identity does not match")]
  ImmutableIdentityMismatch,
  /// A lifecycle mutation used a stale optimistic version.
  #[error("artifact version does not match")]
  VersionConflict,
  /// The optimistic lifecycle version cannot be incremented.
  #[error("artifact version overflowed")]
  VersionOverflow,
}

fn hex_digit(value: u8) -> u8 {
  match value {
    b'0'..=b'9' => value - b'0',
    b'a'..=b'f' => value - b'a' + 10,
    _ => unreachable!("digest validation accepts only lowercase hexadecimal bytes"),
  }
}
