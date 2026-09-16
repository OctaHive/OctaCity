use octacity_server_domain::{EntityKind, TransitionError};

/// Durable execution state of one materialized Build Attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AttemptState {
  /// Jobs exist but the Attempt has not started running.
  Created,
  /// At least one Job is active or ready and completion is pending.
  Running,
  /// Every required Job succeeded or was validly skipped.
  Succeeded,
  /// At least one required Job failed according to Pipeline policy.
  Failed,
  /// Cancellation became terminal for every non-terminal Job.
  Cancelled,
}

/// Fact applied to an [`AttemptState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AttemptEvent {
  /// Initial ready Jobs made the Attempt active.
  Start,
  /// The complete DAG derived a successful outcome.
  Succeed,
  /// The complete DAG derived a failed outcome.
  Fail,
  /// Cancellation became effective for the Attempt.
  Cancel,
}

impl AttemptState {
  /// Applies one fact without performing persistence or other side effects.
  pub fn transition(self, event: AttemptEvent) -> Result<Self, TransitionError<Self, AttemptEvent>> {
    match (self, event) {
      (Self::Created, AttemptEvent::Start) => Ok(Self::Running),
      (Self::Created | Self::Running, AttemptEvent::Cancel) => Ok(Self::Cancelled),
      (Self::Running, AttemptEvent::Succeed) => Ok(Self::Succeeded),
      (Self::Running, AttemptEvent::Fail) => Ok(Self::Failed),
      _ => Err(TransitionError::new(EntityKind::Attempt, self, event)),
    }
  }

  /// Reports whether the Attempt can no longer accept DAG outcomes.
  #[must_use]
  pub const fn is_terminal(self) -> bool {
    matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
  }
}
