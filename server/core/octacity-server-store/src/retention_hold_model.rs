use octacity_server_domain::{BuildId, RetentionHoldVersion, Timestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
  BuildRetentionDeadlines, IdempotencyKey, MutationDisposition, StoreError, StoreInputError, StoreOperation,
};

/// Maximum UTF-8 bytes in an operator-supplied Build Result hold reason.
pub const MAX_RETENTION_HOLD_REASON_BYTES: usize = 512;
/// Maximum UTF-8 bytes in an available management actor identity.
pub const MAX_RETENTION_ACTOR_IDENTITY_BYTES: usize = 128;
/// Maximum UTF-8 bytes in the transport-independent request identity retained for audit.
pub const MAX_RETENTION_REQUEST_IDENTITY_BYTES: usize = 128;

macro_rules! bounded_retention_text {
  ($name:ident, $maximum:ident, $description:literal) => {
    #[doc = $description]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct $name(String);

    impl $name {
      /// Validates and creates the bounded value.
      pub fn new(value: impl Into<String>) -> Result<Self, StoreInputError> {
        let value = value.into();
        if !valid_text(&value, $maximum) {
          return Err(StoreInputError::InvalidRetentionHold);
        }
        Ok(Self(value))
      }

      /// Returns the validated text.
      #[must_use]
      pub fn as_str(&self) -> &str {
        &self.0
      }

      /// Consumes the value and returns its text.
      #[must_use]
      pub fn into_string(self) -> String {
        self.0
      }
    }
  };
}

bounded_retention_text!(
  RetentionHoldReason,
  MAX_RETENTION_HOLD_REASON_BYTES,
  "Validated bounded operator reason for a Build Result retention hold."
);
bounded_retention_text!(
  RetentionActorIdentity,
  MAX_RETENTION_ACTOR_IDENTITY_BYTES,
  "Validated available management actor identity for a retention mutation."
);
bounded_retention_text!(
  RetentionRequestIdentity,
  MAX_RETENTION_REQUEST_IDENTITY_BYTES,
  "Validated transport-independent request identity for retention audit."
);

/// Logical visibility of all components covered by one Build Result hold.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildResultVisibility {
  /// Build metadata and immutable configuration snapshot are visible.
  pub metadata: bool,
  /// Archived logs are visible.
  pub logs: bool,
  /// Produced artifacts are visible.
  pub artifacts: bool,
  /// Produced reports are visible.
  pub reports: bool,
}

impl BuildResultVisibility {
  /// Returns whether no component has entered automatic deletion.
  #[must_use]
  pub const fn complete(self) -> bool {
    self.metadata && self.logs && self.artifacts && self.reports
  }
}

/// State of the latest versioned Build Result hold at an observation time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionHoldState {
  /// The hold currently prevents every automatic-retention transition.
  Active,
  /// An operator explicitly released the hold.
  Released,
  /// The time-bounded hold reached its recorded expiry.
  Expired,
}

/// Safe audit identity associated with one retention-hold transition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetentionAuditIdentity {
  /// Stable actor classification.
  pub actor_kind: String,
  /// Available authenticated actor identity; absent in trusted-network v1.
  pub actor_identity: Option<String>,
  /// Original request identity.
  pub request_identity: String,
}

/// Safe versioned Build Result hold, including available audit identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetentionHold {
  /// Build Result protected by the hold.
  pub build_id: BuildId,
  /// Monotonically increasing hold resource version.
  pub version: RetentionHoldVersion,
  /// Bounded operator-supplied reason.
  pub reason: String,
  /// Authoritative time at which this hold was placed.
  pub created_at: Timestamp,
  /// Optional expiry; `None` denotes a permanent hold.
  pub expires_at: Option<Timestamp>,
  /// Explicit release time, when present.
  pub released_at: Option<Timestamp>,
  /// State derived at the query or mutation observation time.
  pub state: RetentionHoldState,
  /// Audit identity of the placement transition.
  pub creation_audit: RetentionAuditIdentity,
  /// Audit identity of the explicit release transition, when released.
  pub release_audit: Option<RetentionAuditIdentity>,
}

/// Automatic deadlines, aggregate visibility, and latest hold for one Build Result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildResultRetentionState {
  /// Build Result identity.
  pub build_id: BuildId,
  /// Immutable automatic-retention deadlines captured when the Build was accepted.
  pub deadlines: BuildRetentionDeadlines,
  /// Current logical visibility of the complete aggregate.
  pub visibility: BuildResultVisibility,
  /// Latest hold version, including released or expired state, when any exists.
  pub hold: Option<RetentionHold>,
}

/// Reads one Build Result retention state at an authoritative observation time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetBuildResultRetention {
  /// Build Result to inspect.
  pub build_id: BuildId,
  /// Time used to derive active versus expired hold state.
  pub observed_at: Timestamp,
}

/// Places one permanent or time-bounded hold over the complete Build Result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaceBuildResultHold {
  /// Build Result to protect.
  pub build_id: BuildId,
  /// Bounded operator reason.
  pub reason: RetentionHoldReason,
  /// Optional expiry; `None` creates a permanent hold.
  pub expires_at: Option<Timestamp>,
  /// Available authenticated actor identity; absent in trusted-network v1.
  pub actor_identity: Option<RetentionActorIdentity>,
  /// Transport-independent request identity retained for audit.
  pub request_identity: RetentionRequestIdentity,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative placement time.
  pub placed_at: Timestamp,
}

impl PlaceBuildResultHold {
  /// Validates all bounded hold input before persistence begins.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.expires_at.is_some_and(|expires_at| expires_at <= self.placed_at) {
      return Err(invalid(StoreOperation::PlaceBuildResultHold));
    }
    Ok(())
  }
}

/// Releases the active hold using its current optimistic version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseBuildResultHold {
  /// Build Result whose hold is released.
  pub build_id: BuildId,
  /// Hold version that must still be current.
  pub expected_version: RetentionHoldVersion,
  /// Available authenticated actor identity; absent in trusted-network v1.
  pub actor_identity: Option<RetentionActorIdentity>,
  /// Transport-independent request identity retained for audit.
  pub request_identity: RetentionRequestIdentity,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative release time.
  pub released_at: Timestamp,
}

/// Failure specific to releasing a Build Result retention hold.
#[derive(Debug, Error)]
pub enum ReleaseBuildResultHoldError {
  /// The supplied optimistic hold version is no longer current.
  #[error("retention hold version precondition is no longer current")]
  PreconditionFailed,
  /// The authoritative store rejected or could not complete the operation.
  #[error(transparent)]
  Store(#[from] StoreError),
}

/// Applied or exactly replayed retention-hold command result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetentionHoldMutationOutcome {
  /// Whether the command changed state or replayed the original result.
  pub disposition: MutationDisposition,
  /// Resulting complete Build Result retention state.
  pub retention: BuildResultRetentionState,
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
  !value.is_empty() && value.len() <= max_bytes && value.trim() == value && !value.chars().any(char::is_control)
}

const fn invalid(operation: StoreOperation) -> StoreError {
  StoreError::invalid(operation, StoreInputError::InvalidRetentionHold)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hold_input_bounds_reason_identity_and_expiry() {
    let request = fixture();
    request.validate().unwrap();

    assert_eq!(
      RetentionHoldReason::new("x".repeat(MAX_RETENTION_HOLD_REASON_BYTES + 1)),
      Err(StoreInputError::InvalidRetentionHold)
    );

    let mut request = fixture();
    request.expires_at = Some(request.placed_at);
    assert!(request.validate().is_err());

    assert_eq!(
      RetentionRequestIdentity::new("request\nidentity"),
      Err(StoreInputError::InvalidRetentionHold)
    );
  }

  fn fixture() -> PlaceBuildResultHold {
    PlaceBuildResultHold {
      build_id: BuildId::from_uuid(uuid::Uuid::from_u128(1)).unwrap(),
      reason: RetentionHoldReason::new("incident investigation").unwrap(),
      expires_at: None,
      actor_identity: None,
      request_identity: RetentionRequestIdentity::new("request:1").unwrap(),
      idempotency_key: IdempotencyKey::new("hold:1").unwrap(),
      placed_at: Timestamp::from_unix_millis(1_000).unwrap(),
    }
  }
}
