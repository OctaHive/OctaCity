//! Immutable Pipeline versions, DAG validation, and dependency policy.
//!
//! Attempt materialization derives server Jobs from validated snapshots owned
//! by this module.
//!
//! A **Pipeline** is the immutable DAG definition. An **Attempt** is one
//! numbered materialization of a Pipeline into server Jobs; neither term means
//! Build or retry. See the [canonical glossary] and [server ownership guide].
//!
//! [canonical glossary]: https://github.com/OctaHive/OctaCity/blob/main/CONTEXT.md
//! [server ownership guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/server-architecture.md

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// Immutable fan-in policy captured with a materialized Pipeline Job.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyPolicy {
  /// Every direct predecessor must complete successfully.
  AllSucceeded,
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
}

impl DependencyPolicy {
  /// Evaluates a non-empty set of direct predecessor outcomes.
  ///
  /// The Pipeline domain owns this policy. Persistence adapters may evaluate
  /// it only against a serialized authoritative snapshot while atomically
  /// applying an Orchestrator transition.
  pub fn is_satisfied(self, outcomes: impl IntoIterator<Item = DependencyOutcome>) -> bool {
    let mut outcomes = outcomes.into_iter().peekable();
    if outcomes.peek().is_none() {
      return false;
    }
    match self {
      Self::AllSucceeded => outcomes.all(|outcome| outcome == DependencyOutcome::Succeeded),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::{DependencyOutcome, DependencyPolicy};

  #[test]
  fn all_succeeded_requires_a_non_empty_successful_fan_in() {
    let policy = DependencyPolicy::AllSucceeded;
    assert!(!policy.is_satisfied([]));
    assert!(policy.is_satisfied([DependencyOutcome::Succeeded, DependencyOutcome::Succeeded]));
    assert!(!policy.is_satisfied([DependencyOutcome::Succeeded, DependencyOutcome::Pending]));
    assert!(!policy.is_satisfied([DependencyOutcome::Succeeded, DependencyOutcome::Failed]));
    assert!(!policy.is_satisfied([DependencyOutcome::Cancelled]));
  }
}
