use octacity_server_domain::AttemptNumber;

use crate::{AttemptEvent, AttemptState, BuildEvent, BuildState};

/// Deterministic state and identity allocation for one Build retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryDecision {
  attempt_number: AttemptNumber,
  attempt_state: AttemptState,
  build_state: BuildState,
}

impl RetryDecision {
  /// Returns the next monotonically allocated Attempt number.
  #[must_use]
  pub const fn attempt_number(self) -> AttemptNumber {
    self.attempt_number
  }

  /// Returns the initial state of the materialized retry Attempt.
  #[must_use]
  pub const fn attempt_state(self) -> AttemptState {
    self.attempt_state
  }

  /// Returns the reactivated Build state.
  #[must_use]
  pub const fn build_state(self) -> BuildState {
    self.build_state
  }
}

/// Persisted facts that do not permit a retry decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecisionError {
  /// The Build is not in its terminal failed state.
  BuildNotFailed,
  /// The latest Attempt is not in its terminal failed state.
  AttemptNotFailed,
  /// The latest Attempt number cannot be incremented.
  AttemptNumberOverflow,
}

/// Decides whether one latest failed Attempt may be retried.
pub fn decide_retry(
  build_state: BuildState,
  attempt_state: AttemptState,
  latest_attempt_number: AttemptNumber,
) -> Result<RetryDecision, RetryDecisionError> {
  let queued = build_state
    .transition(BuildEvent::Retry)
    .map_err(|_| RetryDecisionError::BuildNotFailed)?;
  if attempt_state != AttemptState::Failed {
    return Err(RetryDecisionError::AttemptNotFailed);
  }
  let build_state = queued
    .transition(BuildEvent::Start)
    .map_err(|_| RetryDecisionError::BuildNotFailed)?;
  let attempt_state = AttemptState::Created
    .transition(AttemptEvent::Start)
    .map_err(|_| RetryDecisionError::AttemptNotFailed)?;
  let attempt_number = latest_attempt_number
    .next()
    .map_err(|_| RetryDecisionError::AttemptNumberOverflow)?;
  Ok(RetryDecision {
    attempt_number,
    attempt_state,
    build_state,
  })
}
