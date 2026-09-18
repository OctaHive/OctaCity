use serde::{Deserialize, Serialize};

/// Immutable fan-in and failure-propagation policy captured with a Pipeline Job.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyPolicy {
  /// Every predecessor must succeed; one terminal failure skips the dependent.
  AllSucceeded,
  /// Every predecessor must finish; failures do not prevent the dependent.
  AllCompleted,
  /// One predecessor must succeed; the dependent is skipped only when none can.
  AnySucceeded,
}

/// Persisted terminal state of one predecessor, or absence of a terminal state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyOutcome {
  /// The predecessor has not completed.
  Pending,
  /// The predecessor completed successfully.
  Succeeded,
  /// The predecessor completed unsuccessfully.
  Failed,
  /// The predecessor was cancelled.
  Cancelled,
  /// The predecessor was skipped by its own dependency policy.
  Skipped,
}

/// Deterministic decision for a non-root Job's complete direct fan-in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyDecision {
  /// At least one predecessor outcome is still required.
  Blocked,
  /// The Job may become ready for placement.
  Ready,
  /// Recorded failure policy makes the Job terminal without execution.
  Skipped,
}

impl DependencyPolicy {
  /// Evaluates a non-empty set of direct predecessor outcomes.
  ///
  /// Empty input remains blocked because root-node readiness is derived from
  /// graph structure rather than pretending it has a satisfied dependency.
  #[must_use]
  pub fn decide(self, outcomes: impl IntoIterator<Item = DependencyOutcome>) -> DependencyDecision {
    let mut observed = false;
    let mut pending = false;
    let mut succeeded = false;
    let mut unsuccessful = false;
    for outcome in outcomes {
      observed = true;
      match outcome {
        DependencyOutcome::Pending => pending = true,
        DependencyOutcome::Succeeded => succeeded = true,
        DependencyOutcome::Failed | DependencyOutcome::Cancelled | DependencyOutcome::Skipped => unsuccessful = true,
      }
    }
    if !observed {
      return DependencyDecision::Blocked;
    }
    match self {
      Self::AllSucceeded if unsuccessful => DependencyDecision::Skipped,
      Self::AllSucceeded if pending => DependencyDecision::Blocked,
      Self::AllSucceeded => DependencyDecision::Ready,
      Self::AllCompleted => {
        if pending {
          DependencyDecision::Blocked
        } else {
          DependencyDecision::Ready
        }
      }
      Self::AnySucceeded => {
        if succeeded {
          DependencyDecision::Ready
        } else if pending {
          DependencyDecision::Blocked
        } else {
          DependencyDecision::Skipped
        }
      }
    }
  }

  /// Reports whether the complete non-empty fan-in permits placement.
  #[must_use]
  pub fn is_satisfied(self, outcomes: impl IntoIterator<Item = DependencyOutcome>) -> bool {
    self.decide(outcomes) == DependencyDecision::Ready
  }
}
