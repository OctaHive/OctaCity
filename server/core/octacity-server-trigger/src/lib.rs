//! Manual, scheduled, external, and internal Trigger normalization.
//!
//! This module owns occurrence identity and deduplication rules independently
//! of ready-Job placement.
//!
//! A **Trigger** is the normalized cause. The **Trigger Engine** evaluates one
//! durable occurrence and creates at most one Build; it is neither the
//! Orchestrator nor the Placement Scheduler. See the [canonical glossary] and
//! the [server ownership guide].
//!
//! [canonical glossary]: https://github.com/OctaHive/OctaCity/blob/main/CONTEXT.md
//! [server ownership guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/server-architecture.md

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use octacity_server_domain::{EntityKind, TransitionError};

/// Durable evaluation state of one deduplicated Trigger occurrence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TriggerOccurrenceState {
  /// The normalized occurrence is durable and awaits evaluation.
  Pending,
  /// One claimed evaluation is resolving policy and immutable inputs.
  Evaluating,
  /// A transient condition deferred evaluation without creating a Build.
  Deferred,
  /// Evaluation created exactly one Build.
  Accepted,
  /// Policy intentionally suppressed Build creation.
  Suppressed,
  /// A permanent validation or policy failure rejected the occurrence.
  Rejected,
}

/// Fact applied to a [`TriggerOccurrenceState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TriggerOccurrenceEvent {
  /// A worker durably claims the occurrence for evaluation.
  BeginEvaluation,
  /// A transient condition requires later retry.
  Defer,
  /// Build creation committed for this deduplication identity.
  Accept,
  /// Disabled configuration or policy suppresses Build creation.
  Suppress,
  /// A permanent failure rejects Build creation.
  Reject,
  /// Deferred work is made pending for another claim.
  Retry,
}

impl TriggerOccurrenceState {
  /// Applies one fact without claiming work or creating a Build.
  pub fn transition(
    self,
    event: TriggerOccurrenceEvent,
  ) -> Result<Self, TransitionError<Self, TriggerOccurrenceEvent>> {
    match (self, event) {
      (Self::Pending, TriggerOccurrenceEvent::BeginEvaluation) => Ok(Self::Evaluating),
      (Self::Evaluating, TriggerOccurrenceEvent::Defer) => Ok(Self::Deferred),
      (Self::Evaluating, TriggerOccurrenceEvent::Accept) => Ok(Self::Accepted),
      (Self::Evaluating, TriggerOccurrenceEvent::Suppress) => Ok(Self::Suppressed),
      (Self::Evaluating, TriggerOccurrenceEvent::Reject) => Ok(Self::Rejected),
      (Self::Deferred, TriggerOccurrenceEvent::Retry) => Ok(Self::Pending),
      _ => Err(TransitionError::new(EntityKind::Trigger, self, event)),
    }
  }

  /// Reports whether evaluation has a permanent outcome.
  #[must_use]
  pub const fn is_terminal(self) -> bool {
    matches!(self, Self::Accepted | Self::Suppressed | Self::Rejected)
  }
}
