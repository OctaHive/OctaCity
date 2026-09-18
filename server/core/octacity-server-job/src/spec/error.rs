use super::JobSpecSigningError;
use thiserror::Error;

/// Failure while deriving or signing JobSpec intent from immutable snapshots.
#[derive(Debug, Error)]
pub enum JobSpecDerivationError {
  /// The Pipeline node template is malformed or contains a server-owned field.
  #[error("pipeline Job template is not a valid execution template")]
  InvalidTemplate,
  /// A Build parameter cannot be represented as an Octa string variable.
  #[error("build parameter '{0}' cannot be represented as an execution variable")]
  InvalidParameter(String),
  /// The Attempt number cannot be represented by JobSpec v1.
  #[error("attempt number exceeds the JobSpec v1 range")]
  AttemptOutOfRange,
  /// The ready timestamp predates the Unix epoch required by JobSpec v1.
  #[error("JobSpec issue time predates the Unix epoch")]
  InvalidIssueTime,
  /// The policy validity interval overflows the JobSpec v1 timestamp range.
  #[error("JobSpec validity interval exceeds the JobSpec v1 range")]
  ValidityOverflow,
  /// A bounded server-owned source or signing policy value is malformed.
  #[error("JobSpec policy is invalid")]
  InvalidPolicy,
  /// Shared protocol validation or signing rejected the derived payload.
  #[error("derived JobSpec could not be signed")]
  Signing(#[source] JobSpecSigningError),
}
