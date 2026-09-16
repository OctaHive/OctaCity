use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable classification of a server domain entity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
  /// Hierarchical project.
  Project,
  /// Versioned build configuration.
  Configuration,
  /// Immutable Pipeline resource.
  Pipeline,
  /// Immutable build input and its derived state.
  Build,
  /// One execution attempt of a build.
  Attempt,
  /// One materialized Pipeline Job.
  Job,
  /// Static Agent Pool.
  Pool,
  /// Enrolled Agent.
  Agent,
  /// Fenced Job lease.
  Lease,
  /// Logical build output.
  Artifact,
  /// One upload attempt for logical Artifact bytes.
  ArtifactUpload,
  /// External-system integration.
  Integration,
  /// Trigger definition or occurrence.
  Trigger,
  /// Stable node within a Pipeline version.
  PipelineNode,
  /// Short-lived fenced remote-cache session.
  CacheSession,
  /// Durable Orchestrator reconciliation cycle.
  Orchestration,
}

impl std::fmt::Display for EntityKind {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(match self {
      Self::Project => "project",
      Self::Configuration => "configuration",
      Self::Pipeline => "pipeline",
      Self::Build => "build",
      Self::Attempt => "attempt",
      Self::Job => "job",
      Self::Pool => "pool",
      Self::Agent => "agent",
      Self::Lease => "lease",
      Self::Artifact => "artifact",
      Self::ArtifactUpload => "artifact_upload",
      Self::Integration => "integration",
      Self::Trigger => "trigger",
      Self::PipelineNode => "pipeline_node",
      Self::CacheSession => "cache_session",
      Self::Orchestration => "orchestration",
    })
  }
}

/// Reason an opaque identifier failed validation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierErrorKind {
  /// The value is not a UUID.
  Malformed,
  /// The all-zero UUID is reserved and cannot identify an entity.
  Nil,
  /// The UUID is valid but not lowercase hyphenated canonical text.
  NonCanonical,
}

/// Reason a bounded textual value failed validation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextErrorKind {
  /// The value is empty.
  Empty,
  /// The UTF-8 representation exceeds the declared byte limit.
  TooLong,
  /// Leading or trailing whitespace makes identity ambiguous.
  SurroundingWhitespace,
  /// The value contains a control character.
  ControlCharacter,
  /// The value contains a character outside its declared alphabet.
  InvalidCharacter,
  /// The first character is not valid for this value type.
  InvalidStart,
}

/// Reason a positive version or sequence number failed validation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionErrorKind {
  /// Versions and attempt numbers start at one.
  Zero,
  /// Incrementing the value would exceed `u64`.
  Overflow,
}

/// Reason a timestamp failed validation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimestampErrorKind {
  /// The millisecond value is outside years 0001 through 9999 UTC.
  OutOfRange,
}

/// Validation error returned while constructing a domain value object.
#[derive(Clone, Debug, Deserialize, Eq, Error, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DomainValueError {
  /// An opaque entity identifier is invalid.
  #[error("invalid {entity} identifier: {reason:?}")]
  InvalidIdentifier {
    /// Entity whose identifier was rejected.
    entity: EntityKind,
    /// Stable rejection reason.
    reason: IdentifierErrorKind,
  },
  /// A resource name is invalid.
  #[error("invalid {entity} name: {reason:?}")]
  InvalidName {
    /// Entity whose name was rejected.
    entity: EntityKind,
    /// Stable rejection reason.
    reason: TextErrorKind,
  },
  /// A positive entity version is invalid.
  #[error("invalid {entity} version: {reason:?}")]
  InvalidVersion {
    /// Entity whose version was rejected.
    entity: EntityKind,
    /// Stable rejection reason.
    reason: VersionErrorKind,
  },
  /// An attempt number is invalid.
  #[error("invalid attempt number: {reason:?}")]
  InvalidAttemptNumber {
    /// Stable rejection reason.
    reason: VersionErrorKind,
  },
  /// A timestamp is outside the supported UTC range.
  #[error("invalid timestamp: {reason:?}")]
  InvalidTimestamp {
    /// Stable rejection reason.
    reason: TimestampErrorKind,
  },
  /// A stable Trigger deduplication identity is invalid.
  #[error("invalid trigger identity: {reason:?}")]
  InvalidTriggerIdentity {
    /// Stable rejection reason.
    reason: TextErrorKind,
  },
  /// A Pipeline node identity is invalid.
  #[error("invalid pipeline node identity: {reason:?}")]
  InvalidPipelineNodeIdentity {
    /// Stable rejection reason.
    reason: TextErrorKind,
  },
}

/// Typed failure returned by server domain operations.
///
/// The enum intentionally carries classifications rather than arbitrary text,
/// credentials, provider diagnostics, or database errors.
#[derive(Clone, Debug, Deserialize, Eq, Error, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case", deny_unknown_fields)]
pub enum DomainError {
  /// A value object could not be constructed.
  #[error("invalid domain value: {error}")]
  InvalidValue {
    /// Typed validation failure.
    error: DomainValueError,
  },
  /// The requested entity does not exist or is not visible.
  #[error("{entity} was not found")]
  NotFound {
    /// Missing entity kind.
    entity: EntityKind,
  },
  /// Creation would duplicate an existing logical entity.
  #[error("{entity} already exists")]
  AlreadyExists {
    /// Conflicting entity kind.
    entity: EntityKind,
  },
  /// Current state conflicts with the requested operation.
  #[error("{entity} conflicts with the requested operation")]
  Conflict {
    /// Conflicting entity kind.
    entity: EntityKind,
  },
  /// An optimistic concurrency precondition is stale.
  #[error("{entity} version conflict: expected {expected}, actual {actual}")]
  VersionConflict {
    /// Entity whose version changed.
    entity: EntityKind,
    /// Version supplied by the caller.
    expected: u64,
    /// Current authoritative version.
    actual: u64,
  },
  /// A state-machine transition is forbidden.
  #[error("invalid {entity} state transition")]
  InvalidTransition {
    /// Entity whose transition was rejected.
    entity: EntityKind,
  },
  /// Effective policy rejects the operation.
  #[error("policy rejects the {entity} operation")]
  PolicyViolation {
    /// Entity affected by the policy.
    entity: EntityKind,
  },
  /// A declared domain limit would be exceeded.
  #[error("{entity} limit exceeded")]
  LimitExceeded {
    /// Entity whose limit was exceeded.
    entity: EntityKind,
  },
}

impl From<DomainValueError> for DomainError {
  fn from(error: DomainValueError) -> Self {
    Self::InvalidValue { error }
  }
}
