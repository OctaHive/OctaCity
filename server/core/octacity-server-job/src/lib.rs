//! Server-side Job identity, state, requirements, and signed-intent inputs.
//!
//! This is distinct from the agent-side `octacity-job` execution lifecycle.
//! A server **Job** is one materialized Pipeline node and is the smallest unit
//! visible to placement. It may contain multiple Octa tasks. See the
//! [canonical glossary] and [server ownership guide].
//!
//! [`JobState`] models durable server coordination only; the Agent-side
//! Executor owns actual execution.
//!
//! [canonical glossary]: https://github.com/OctaHive/OctaCity/blob/main/CONTEXT.md
//! [server ownership guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/server-architecture.md

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use octacity_server_domain::{EntityKind, TransitionError};

/// Durable scheduling and execution state of one materialized DAG Job.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JobState {
  /// One or more required predecessor Jobs are incomplete.
  Blocked,
  /// Dependency policy is satisfied and the Job is visible to placement.
  Ready,
  /// A current fenced lease owns the Job but execution has not started.
  Leased,
  /// The current lease owner is executing the Job.
  Running,
  /// Cancellation was requested from a current lease owner.
  Cancelling,
  /// Execution completed successfully.
  Succeeded,
  /// Execution completed with failure.
  Failed,
  /// Cancellation became terminal.
  Cancelled,
  /// Pipeline dependency policy made execution unnecessary or forbidden.
  Skipped,
}

/// Fact applied to a [`JobState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JobEvent {
  /// Every required dependency satisfied its success policy.
  DependenciesSatisfied,
  /// Dependency policy requires this Job to be skipped.
  DependencyFailed,
  /// Placement atomically created the current lease.
  LeaseGranted,
  /// The current lease owner started execution.
  ExecutionStarted,
  /// Durable cancellation intent targets this Job.
  CancellationRequested,
  /// The current lease ended without a terminal Job outcome.
  LeaseLost,
  /// The current lease owner completed successfully.
  Succeed,
  /// The current lease owner completed with failure.
  Fail,
  /// The current lease owner confirmed cancellation.
  Cancel,
}

impl JobState {
  /// Applies one fact without queue, lease, or persistence side effects.
  pub fn transition(self, event: JobEvent) -> Result<Self, TransitionError<Self, JobEvent>> {
    match (self, event) {
      (Self::Blocked, JobEvent::DependenciesSatisfied) => Ok(Self::Ready),
      (Self::Blocked, JobEvent::DependencyFailed) => Ok(Self::Skipped),
      (Self::Blocked | Self::Ready, JobEvent::CancellationRequested) => Ok(Self::Cancelled),
      (Self::Ready, JobEvent::LeaseGranted) => Ok(Self::Leased),
      (Self::Leased, JobEvent::ExecutionStarted) => Ok(Self::Running),
      (Self::Leased | Self::Running, JobEvent::CancellationRequested) => Ok(Self::Cancelling),
      (Self::Leased | Self::Running, JobEvent::LeaseLost) => Ok(Self::Ready),
      (Self::Cancelling, JobEvent::LeaseLost | JobEvent::Cancel) => Ok(Self::Cancelled),
      (Self::Running | Self::Cancelling, JobEvent::Succeed) => Ok(Self::Succeeded),
      (Self::Running | Self::Cancelling, JobEvent::Fail) => Ok(Self::Failed),
      (Self::Running, JobEvent::Cancel) => Ok(Self::Cancelled),
      _ => Err(TransitionError::new(EntityKind::Job, self, event)),
    }
  }

  /// Reports whether no scheduling or execution transition remains.
  #[must_use]
  pub const fn is_terminal(self) -> bool {
    matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled | Self::Skipped)
  }
}
