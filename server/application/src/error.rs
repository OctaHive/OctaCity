use octacity_server_store::{ReleaseBuildResultHoldError, StoreError};
use thiserror::Error;

use crate::ProjectionError;

/// Stable transport-neutral classification of an application failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationFailure {
  /// Caller input is invalid independently of current state.
  Invalid,
  /// A referenced resource does not exist.
  NotFound,
  /// Current authoritative state conflicts with the request.
  Conflict,
  /// The supplied optimistic resource version is no longer current.
  PreconditionFailed,
  /// The selected installed adapter does not advertise the requested capability.
  CapabilityUnavailable,
  /// A required authoritative dependency is temporarily unavailable.
  Unavailable,
  /// Safe projection or another internal invariant failed.
  Internal,
}

/// Transport-independent failure from a typed application handler.
#[derive(Debug, Error)]
pub enum ApplicationError {
  /// Typed application input violated a command-specific invariant.
  #[error("application input is invalid")]
  InvalidInput,
  /// A selected adapter does not implement an optional operation.
  #[error("application capability is unavailable")]
  CapabilityUnavailable,
  /// The supplied optimistic resource version is no longer current.
  #[error("application precondition is no longer current")]
  PreconditionFailed,
  /// An authoritative backend-neutral port rejected or could not complete the operation.
  #[error("authoritative application port failed")]
  Store(#[from] StoreError),
  /// Authoritative data could not be represented by a safe application projection.
  #[error("authoritative state cannot be projected safely")]
  Projection(#[from] ProjectionError),
}

impl ApplicationError {
  /// Constructs a transport-neutral invalid-input failure.
  #[must_use]
  pub const fn invalid() -> Self {
    Self::InvalidInput
  }

  /// Constructs an unavailable dependency failure for adapter-boundary tests.
  #[must_use]
  pub const fn unavailable() -> Self {
    Self::Store(StoreError::Unavailable)
  }

  /// Constructs a stable optional-capability failure.
  #[must_use]
  pub const fn capability_unavailable() -> Self {
    Self::CapabilityUnavailable
  }

  /// Returns a transport-neutral failure classification.
  #[must_use]
  pub const fn classification(&self) -> ApplicationFailure {
    match self {
      Self::InvalidInput => ApplicationFailure::Invalid,
      Self::CapabilityUnavailable => ApplicationFailure::CapabilityUnavailable,
      Self::PreconditionFailed => ApplicationFailure::PreconditionFailed,
      Self::Store(StoreError::InvalidInput { .. }) => ApplicationFailure::Invalid,
      Self::Store(StoreError::NotFound { .. }) => ApplicationFailure::NotFound,
      Self::Store(StoreError::Conflict { .. } | StoreError::Duplicate { .. }) => ApplicationFailure::Conflict,
      Self::Store(StoreError::Unavailable) => ApplicationFailure::Unavailable,
      Self::Store(
        StoreError::Fenced { .. }
        | StoreError::Expired { .. }
        | StoreError::EventGap { .. }
        | StoreError::EventsMissing { .. }
        | StoreError::CredentialRejected,
      ) => ApplicationFailure::Conflict,
      Self::Projection(_) => ApplicationFailure::Internal,
    }
  }
}

impl From<ReleaseBuildResultHoldError> for ApplicationError {
  fn from(error: ReleaseBuildResultHoldError) -> Self {
    match error {
      ReleaseBuildResultHoldError::PreconditionFailed => Self::PreconditionFailed,
      ReleaseBuildResultHoldError::Store(error) => Self::Store(error),
    }
  }
}
