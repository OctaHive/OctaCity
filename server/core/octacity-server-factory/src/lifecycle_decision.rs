use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

use crate::{FactoryDigest, FactoryError, FactoryKey, FactoryRunState};

/// Opaque content-derived fencing value for one Factory reconciler claim.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FactoryClaimFence(FactoryDigest);

impl FactoryClaimFence {
  /// Wraps an opaque digest without making it displayable in diagnostics.
  #[must_use]
  pub const fn new(value: FactoryDigest) -> Self {
    Self(value)
  }

  /// Returns the opaque digest for durable equality and identity binding.
  #[must_use]
  pub const fn digest(self) -> FactoryDigest {
    self.0
  }
}

/// Current exclusive ownership window for one Factory Run reconciliation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactoryClaim {
  fence: FactoryClaimFence,
  claimed_at: Timestamp,
  expires_at: Timestamp,
}

impl FactoryClaim {
  /// Constructs a positive claim window.
  pub fn new(fence: FactoryClaimFence, claimed_at: Timestamp, expires_at: Timestamp) -> Result<Self, FactoryError> {
    if expires_at <= claimed_at {
      return Err(FactoryError::InvalidClaim);
    }
    Ok(Self {
      fence,
      claimed_at,
      expires_at,
    })
  }

  /// Returns the opaque fence bound to this ownership window.
  #[must_use]
  pub const fn fence(self) -> FactoryClaimFence {
    self.fence
  }

  /// Returns the authoritative instant at which ownership began.
  #[must_use]
  pub const fn claimed_at(self) -> Timestamp {
    self.claimed_at
  }

  /// Returns the exclusive ownership deadline.
  #[must_use]
  pub const fn expires_at(self) -> Timestamp {
    self.expires_at
  }

  /// Verifies that a presented fence owns the claim at an authoritative time.
  pub fn verify_fence(self, presented_fence: FactoryClaimFence, observed_at: Timestamp) -> Result<(), FactoryError> {
    if self.fence != presented_fence {
      return Err(FactoryError::StaleClaim);
    }
    if observed_at < self.claimed_at {
      return Err(FactoryError::InvalidClaim);
    }
    if observed_at >= self.expires_at {
      return Err(FactoryError::ExpiredClaim);
    }
    Ok(())
  }
}

/// Authoritative WIP counters observed with one Factory snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryWipUsage {
  active_runs: u32,
  active_nodes: u32,
}

impl FactoryWipUsage {
  /// Constructs WIP usage including the current non-terminal Run.
  #[must_use]
  pub const fn new(active_runs: u32, active_nodes: u32) -> Self {
    Self {
      active_runs,
      active_nodes,
    }
  }

  /// Returns unfinished executable node attempts across active Runs; suspended
  /// subflow call frames do not consume an additional execution slot.
  #[must_use]
  pub const fn active_nodes(self) -> u32 {
    self.active_nodes
  }

  /// Returns active Runs pinned to this exact Factory Configuration version.
  #[must_use]
  pub const fn active_runs(self) -> u32 {
    self.active_runs
  }
}

/// Technical root execution progress. Logical outcomes exist only in the
/// admitted node contracts and their retained results.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum FactoryLifecycleProgress {
  /// Work has been admitted with a complete pinned graph.
  Admitted,
  /// The admitted root is executing or waiting for an exact node operation.
  FlowRunning {
    /// Exact root instance owning execution.
    root: crate::FlowRunId,
  },
  /// All owned effects have stopped after accepted cancellation.
  FlowCancelled {
    /// Exact cancelled root instance.
    root: crate::FlowRunId,
  },
  /// The pinned graph selected one of its arbitrary terminal contracts.
  FlowResolved {
    /// Exact completed root instance.
    root: crate::FlowRunId,
    /// Terminal key from the admitted definition; it selects no behavior in code.
    terminal: FactoryKey,
  },
}

/// Checks technical aggregate state without interpreting a business outcome.
pub fn validate_lifecycle_progress(
  state: FactoryRunState,
  progress: &FactoryLifecycleProgress,
) -> Result<(), FactoryError> {
  if matches!(
    (state, progress),
    (FactoryRunState::Admitted, FactoryLifecycleProgress::Admitted)
      | (
        FactoryRunState::Running | FactoryRunState::Waiting,
        FactoryLifecycleProgress::FlowRunning { .. }
      )
      | (
        FactoryRunState::Cancelled,
        FactoryLifecycleProgress::FlowCancelled { .. }
      )
      | (
        FactoryRunState::Completed,
        FactoryLifecycleProgress::FlowResolved { .. }
      )
  ) {
    Ok(())
  } else {
    Err(FactoryError::InvalidLifecycle { state })
  }
}

/// Checks monotonic technical progress; only the admitted graph and its journal
/// can prove a logical terminal or permit cross-node execution.
pub fn validate_lifecycle_transition(
  previous_state: FactoryRunState,
  previous: &FactoryLifecycleProgress,
  next_state: FactoryRunState,
  next: &FactoryLifecycleProgress,
) -> Result<(), FactoryError> {
  validate_lifecycle_progress(previous_state, previous)?;
  validate_lifecycle_progress(next_state, next)?;
  let valid = (previous_state == next_state && previous == next)
    || match (previous, next) {
      (
        FactoryLifecycleProgress::Admitted,
        FactoryLifecycleProgress::FlowRunning { .. } | FactoryLifecycleProgress::FlowCancelled { .. },
      ) => true,
      (
        FactoryLifecycleProgress::FlowRunning { root: previous },
        FactoryLifecycleProgress::FlowRunning { root: next }
        | FactoryLifecycleProgress::FlowCancelled { root: next }
        | FactoryLifecycleProgress::FlowResolved { root: next, .. },
      ) => previous == next,
      _ => false,
    };
  if valid {
    Ok(())
  } else {
    Err(FactoryError::InvalidLifecycle { state: next_state })
  }
}
