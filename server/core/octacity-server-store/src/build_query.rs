use octacity_server_domain::{
  AgentId, AttemptId, AttemptNumber, AttemptVersion, BuildId, BuildVersion, JobId, JobVersion, PoolId, Timestamp,
};
use octacity_server_job::{JobFailureClass, JobState};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_trigger::{NormalizedTriggerOccurrence, TriggerOccurrenceState};

use crate::{ImmutableBuildInput, MaterializedJob};

/// Authoritative Build facts used to construct a safe application projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildRecord {
  /// Immutable Build input and snapshots.
  pub build: ImmutableBuildInput,
  /// Current aggregate state.
  pub state: BuildState,
  /// Current optimistic version.
  pub version: BuildVersion,
  /// Normalized Trigger occurrence that created the Build.
  pub trigger: NormalizedTriggerOccurrence,
  /// Current durable Trigger occurrence state.
  pub trigger_state: TriggerOccurrenceState,
  /// Authoritative Trigger occurrence creation time.
  pub trigger_created_at: Timestamp,
  /// Time of the latest accepted Trigger occurrence transition.
  pub trigger_updated_at: Timestamp,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Time of the latest accepted state transition.
  pub updated_at: Timestamp,
}

/// Authoritative Attempt facts used by execution diagnostics and retry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttemptRecord {
  /// Stable Attempt identity.
  pub id: AttemptId,
  /// Owning Build.
  pub build_id: BuildId,
  /// Positive monotonic number within the Build.
  pub number: AttemptNumber,
  /// Prior failed Attempt retried by this Attempt, when present.
  pub retry_of_attempt_id: Option<AttemptId>,
  /// Current aggregate state.
  pub state: AttemptState,
  /// Current optimistic version.
  pub version: AttemptVersion,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Time of the latest accepted state transition.
  pub updated_at: Timestamp,
  /// Materialized Jobs in stable identity order.
  pub jobs: Vec<JobRecord>,
}

/// Durable ready-queue ordering inputs for a Job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobQueueRecord {
  /// Build priority captured by the queue entry.
  pub priority: i64,
  /// Time at which the Job became ready.
  pub enqueued_at: Timestamp,
}

/// Non-secret execution assignment retained for diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobAssignmentRecord {
  /// Pool selected by placement.
  pub pool_id: PoolId,
  /// Agent selected by placement.
  pub agent_id: AgentId,
}

/// Authoritative terminal outcome retained independently of current queue state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobTerminalRecord {
  /// Terminal state.
  pub state: JobState,
  /// Typed execution or infrastructure failure for failed execution.
  pub failure_class: Option<JobFailureClass>,
  /// Authoritative completion or propagation time.
  pub completed_at: Timestamp,
}

/// Authoritative Job facts with secret signed execution envelopes omitted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobRecord {
  /// Owning Attempt.
  pub attempt_id: AttemptId,
  /// Immutable materialized Job and dependency topology.
  pub job: MaterializedJob,
  /// Current scheduling or execution state.
  pub state: JobState,
  /// Current optimistic version.
  pub version: JobVersion,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Time of the latest accepted state transition.
  pub updated_at: Timestamp,
  /// Queue inputs when the Job is ready.
  pub queue: Option<JobQueueRecord>,
  /// Latest execution assignment, retained after terminal completion.
  pub assignment: Option<JobAssignmentRecord>,
  /// Terminal outcome when available.
  pub terminal: Option<JobTerminalRecord>,
  /// Greatest contiguous durable event sequence.
  pub event_cursor: u64,
}

impl JobRecord {
  /// Returns the stable Job identity.
  #[must_use]
  pub const fn id(&self) -> JobId {
    self.job.id
  }
}
