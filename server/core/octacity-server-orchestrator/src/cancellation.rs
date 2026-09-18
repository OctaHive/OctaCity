use std::collections::BTreeMap;

use octacity_server_domain::JobId;
use octacity_server_job::{JobEvent, JobState};

use crate::{AttemptState, BuildState};

/// One Job transition selected while applying a durable Build cancellation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancellationTransition {
  job_id: JobId,
  state: JobState,
}

impl CancellationTransition {
  /// Returns the affected Job.
  #[must_use]
  pub const fn job_id(self) -> JobId {
    self.job_id
  }

  /// Returns the state selected by the Job state machine.
  #[must_use]
  pub const fn state(self) -> JobState {
    self.state
  }
}

/// Deterministic result of propagating cancellation through one active Attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancellationDecision {
  transitions: Vec<CancellationTransition>,
  attempt_state: AttemptState,
  build_state: BuildState,
}

impl CancellationDecision {
  /// Borrows stably ordered Job transitions.
  #[must_use]
  pub fn transitions(&self) -> &[CancellationTransition] {
    &self.transitions
  }

  /// Returns the aggregate Attempt state after immediate transitions.
  #[must_use]
  pub const fn attempt_state(&self) -> AttemptState {
    self.attempt_state
  }

  /// Returns the aggregate Build state after immediate transitions.
  #[must_use]
  pub const fn build_state(&self) -> BuildState {
    self.build_state
  }
}

/// Invalid persisted facts that prevent safe cancellation propagation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationError {
  /// The Build is no longer active.
  BuildNotActive,
  /// The current Attempt is no longer active.
  AttemptNotActive,
  /// The active Attempt contains no Jobs.
  EmptyAttempt,
  /// The same Job identity occurs more than once.
  DuplicateJob {
    /// Repeated Job identity.
    job_id: JobId,
  },
  /// The Job state machine rejected a required cancellation transition.
  InvalidTransition {
    /// Job containing the inconsistent state.
    job_id: JobId,
  },
}

/// Propagates durable cancellation intent through one complete active Attempt.
///
/// Blocked and ready Jobs become terminal immediately. Leased and running Jobs
/// enter `Cancelling` so their current owners can observe the directive and
/// report a fenced terminal outcome. Existing terminal Jobs are immutable.
pub fn cancel_job_states(
  build_state: BuildState,
  attempt_state: AttemptState,
  jobs: impl IntoIterator<Item = (JobId, JobState)>,
) -> Result<CancellationDecision, CancellationError> {
  if !matches!(build_state, BuildState::Queued | BuildState::Running) {
    return Err(CancellationError::BuildNotActive);
  }
  if !matches!(attempt_state, AttemptState::Created | AttemptState::Running) {
    return Err(CancellationError::AttemptNotActive);
  }
  let mut states = BTreeMap::new();
  for (job_id, state) in jobs {
    if states.insert(job_id, state).is_some() {
      return Err(CancellationError::DuplicateJob { job_id });
    }
  }
  if states.is_empty() {
    return Err(CancellationError::EmptyAttempt);
  }

  let mut transitions = Vec::new();
  let current: Vec<_> = states.iter().map(|(job_id, state)| (*job_id, *state)).collect();
  for (job_id, state) in current {
    let next = match state {
      JobState::Blocked | JobState::Ready | JobState::Leased | JobState::Running => Some(
        state
          .transition(JobEvent::CancellationRequested)
          .map_err(|_| CancellationError::InvalidTransition { job_id })?,
      ),
      JobState::Cancelling | JobState::Succeeded | JobState::Failed | JobState::Cancelled | JobState::Skipped => None,
    };
    if let Some(next) = next {
      transitions.push(CancellationTransition { job_id, state: next });
      states.insert(job_id, next);
    }
  }

  let cancellation_pending = states.values().any(|state| *state == JobState::Cancelling);
  Ok(CancellationDecision {
    transitions,
    attempt_state: if cancellation_pending {
      AttemptState::Running
    } else {
      AttemptState::Cancelled
    },
    build_state: if cancellation_pending {
      BuildState::Running
    } else {
      BuildState::Cancelled
    },
  })
}
