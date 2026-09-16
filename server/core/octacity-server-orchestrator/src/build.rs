use octacity_server_domain::{EntityKind, TransitionError};

/// Durable aggregate state of one Build across its Attempts.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BuildState {
  /// Accepted Build waiting for its next Attempt to start.
  Queued,
  /// An Attempt is active.
  Running,
  /// The Build completed successfully.
  Succeeded,
  /// The latest Attempt failed and the Build may be retried.
  Failed,
  /// Cancellation became terminal.
  Cancelled,
}

/// Fact applied to a [`BuildState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BuildEvent {
  /// A queued Attempt became active.
  Start,
  /// The active Attempt completed successfully.
  Succeed,
  /// The active Attempt failed.
  Fail,
  /// Cancellation became effective before or during execution.
  Cancel,
  /// A failed Build was accepted for another Attempt.
  Retry,
}

impl BuildState {
  /// Applies one fact without performing persistence or other side effects.
  pub fn transition(self, event: BuildEvent) -> Result<Self, TransitionError<Self, BuildEvent>> {
    match (self, event) {
      (Self::Queued, BuildEvent::Start) => Ok(Self::Running),
      (Self::Queued | Self::Running, BuildEvent::Cancel) => Ok(Self::Cancelled),
      (Self::Running, BuildEvent::Succeed) => Ok(Self::Succeeded),
      (Self::Running, BuildEvent::Fail) => Ok(Self::Failed),
      (Self::Failed, BuildEvent::Retry) => Ok(Self::Queued),
      _ => Err(TransitionError::new(EntityKind::Build, self, event)),
    }
  }

  /// Reports whether no later event can change this Build without a retry.
  #[must_use]
  pub const fn is_terminal(self) -> bool {
    matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
  }
}
