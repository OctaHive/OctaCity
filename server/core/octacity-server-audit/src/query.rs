use octacity_server_domain::{AuditFactId, Timestamp};
use thiserror::Error;

use crate::{AuditActorKind, AuditFact};

/// Maximum facts returned by one audit query.
pub const MAX_AUDIT_PAGE_SIZE: u16 = 200;
/// Maximum UTF-8 bytes in an actor identity filter.
pub const MAX_AUDIT_ACTOR_IDENTITY_BYTES: usize = 256;
/// Maximum UTF-8 bytes in an operation filter.
pub const MAX_AUDIT_OPERATION_BYTES: usize = 128;
/// Maximum UTF-8 bytes in a target classification filter.
pub const MAX_AUDIT_TARGET_KIND_BYTES: usize = 64;
/// Maximum UTF-8 bytes in a target identity filter.
pub const MAX_AUDIT_TARGET_IDENTITY_BYTES: usize = 256;
/// Maximum UTF-8 bytes in a request identity filter.
pub const MAX_AUDIT_REQUEST_IDENTITY_BYTES: usize = 256;

/// Stable exclusive cursor for newest-first audit pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuditCursor {
  /// Commit time of the last fact returned by the preceding page.
  pub occurred_at: Timestamp,
  /// Stable tie-breaker of the last fact returned by the preceding page.
  pub id: AuditFactId,
}

/// Bounded filters for immutable audit facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditFactQuery {
  /// Optional actor classification.
  pub actor_kind: Option<AuditActorKind>,
  /// Optional exact actor identity.
  pub actor_identity: Option<String>,
  /// Optional exact operation.
  pub operation: Option<String>,
  /// Optional exact target classification.
  pub target_kind: Option<String>,
  /// Optional exact target identity.
  pub target_identity: Option<String>,
  /// Optional exact request identity.
  pub request_identity: Option<String>,
  /// Inclusive lower commit-time bound.
  pub occurred_from: Option<Timestamp>,
  /// Inclusive upper commit-time bound.
  pub occurred_through: Option<Timestamp>,
  /// Exclusive cursor from the preceding page.
  pub after: Option<AuditCursor>,
  /// Positive bounded page size.
  pub limit: u16,
}

impl AuditFactQuery {
  /// Validates all adapter-independent bounds.
  pub fn validate(&self) -> Result<(), AuditInputError> {
    validate_optional(&self.actor_identity, MAX_AUDIT_ACTOR_IDENTITY_BYTES)?;
    validate_optional(&self.operation, MAX_AUDIT_OPERATION_BYTES)?;
    validate_optional(&self.target_kind, MAX_AUDIT_TARGET_KIND_BYTES)?;
    validate_optional(&self.target_identity, MAX_AUDIT_TARGET_IDENTITY_BYTES)?;
    validate_optional(&self.request_identity, MAX_AUDIT_REQUEST_IDENTITY_BYTES)?;
    if self.limit == 0 || self.limit > MAX_AUDIT_PAGE_SIZE {
      return Err(AuditInputError::InvalidPageSize);
    }
    if self
      .occurred_from
      .zip(self.occurred_through)
      .is_some_and(|(from, through)| from > through)
    {
      return Err(AuditInputError::InvalidTimeRange);
    }
    Ok(())
  }
}

fn validate_optional(value: &Option<String>, maximum: usize) -> Result<(), AuditInputError> {
  if value.as_ref().is_some_and(|value| {
    value.is_empty() || value.len() > maximum || value.trim() != value || value.chars().any(char::is_control)
  }) {
    return Err(AuditInputError::InvalidFilter);
  }
  Ok(())
}

/// Deterministic newest-first page of immutable audit facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditFactPage {
  /// Facts in stable newest-first order.
  pub items: Vec<AuditFact>,
  /// Exclusive continuation cursor, when another page exists.
  pub next_cursor: Option<AuditCursor>,
}

/// Stable invalid-input classifications for audit values and queries.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AuditInputError {
  /// Actor kind is unknown.
  #[error("invalid audit actor kind")]
  InvalidActorKind,
  /// Stored outcome is unknown.
  #[error("invalid audit outcome")]
  InvalidOutcome,
  /// Metadata is unbounded, malformed, or contains a sensitive field.
  #[error("invalid audit metadata")]
  InvalidMetadata,
  /// A textual query filter is empty, unbounded, or malformed.
  #[error("invalid audit filter")]
  InvalidFilter,
  /// Page size is zero or exceeds the server limit.
  #[error("invalid audit page size")]
  InvalidPageSize,
  /// Lower time bound is later than the upper bound.
  #[error("invalid audit time range")]
  InvalidTimeRange,
}
