use octacity_server_domain::{EntityKind, TransitionError};

/// Durable lifecycle of one idempotent Orchestrator reconciliation cycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrchestrationState {
  /// Durable work is ready to be reconciled.
  Pending,
  /// One worker is evaluating persisted facts.
  Reconciling,
  /// No transition is currently possible; another durable fact is required.
  Waiting,
  /// Reconciliation reached a terminal aggregate outcome.
  Completed,
  /// Reconciliation failed and may be retried from persisted facts.
  Failed,
}

/// Fact applied to an [`OrchestrationState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrchestrationEvent {
  /// A worker begins one reconciliation cycle.
  Begin,
  /// The graph requires another persisted Job outcome.
  Wait,
  /// A new persisted fact makes waiting work ready again.
  Resume,
  /// The aggregate reached a terminal outcome.
  Complete,
  /// Reconciliation failed after bounded handling.
  Fail,
  /// Failed work is made pending for an explicit retry.
  Retry,
}

impl OrchestrationState {
  /// Applies one fact without claiming work, persisting, or waking workers.
  pub fn transition(self, event: OrchestrationEvent) -> Result<Self, TransitionError<Self, OrchestrationEvent>> {
    match (self, event) {
      (Self::Pending, OrchestrationEvent::Begin) => Ok(Self::Reconciling),
      (Self::Reconciling, OrchestrationEvent::Wait) => Ok(Self::Waiting),
      (Self::Reconciling, OrchestrationEvent::Complete) => Ok(Self::Completed),
      (Self::Reconciling, OrchestrationEvent::Fail) => Ok(Self::Failed),
      (Self::Waiting, OrchestrationEvent::Resume) | (Self::Failed, OrchestrationEvent::Retry) => Ok(Self::Pending),
      _ => Err(TransitionError::new(EntityKind::Orchestration, self, event)),
    }
  }

  /// Reports whether reconciliation has produced a terminal aggregate result.
  #[must_use]
  pub const fn is_terminal(self) -> bool {
    matches!(self, Self::Completed)
  }
}
