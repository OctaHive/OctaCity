use std::{
  ops::{Deref, DerefMut},
  sync::MutexGuard,
};

use super::*;

#[derive(Clone, Default)]
pub(crate) struct MemoryState {
  pub(crate) credentials: crate::credential_testing::CredentialMemoryState,
  pub(super) trigger_prerequisites: BTreeSet<TriggerPrerequisites>,
  pub(super) accepted: BTreeMap<TriggerOccurrenceId, AcceptedRecord>,
  pub(super) suppressed: BTreeMap<TriggerOccurrenceId, SuppressedRecord>,
  pub(super) evaluated_by_deduplication: BTreeMap<TriggerDeduplicationKey, TriggerOccurrenceId>,
  pub(super) schedules: BTreeMap<TriggerDefinitionRef, ScheduleMemoryRecord>,
  pub(super) builds: BTreeMap<BuildId, BuildState>,
  pub(super) attempts: BTreeMap<AttemptId, MemoryAttempt>,
  pub(super) jobs: BTreeMap<JobId, MemoryJob>,
  pub(super) signed_job_specs: BTreeMap<JobId, DerivedJobSpec>,
  pub(super) ready_queue: BTreeSet<ReadyEntry>,
  pub(super) ready_jobs: BTreeSet<JobId>,
  pub(super) next_enqueue_order: u64,
  pub(super) leases: BTreeMap<LeaseId, LeaseGrant>,
  pub(super) lease_states: BTreeMap<LeaseId, octacity_server_scheduler::LeaseState>,
  pub(super) lease_heartbeats: BTreeMap<IdempotencyKey, HeartbeatRecord>,
  pub(crate) pools: BTreeMap<(PoolId, PoolVersion), PoolEligibility>,
  pub(crate) registrations: BTreeMap<(AgentId, RegistrationEpoch), RegistrationEligibility>,
  pub(super) claims: BTreeMap<LeaseId, ClaimRecord>,
  pub(super) current_lease_by_job: BTreeMap<JobId, LeaseId>,
  pub(super) events: BTreeMap<JobId, BTreeMap<EventSequence, EventDigest>>,
  pub(super) event_appends: BTreeMap<(LeaseId, EventSequence, EventSequence), EventAppendRecord>,
  pub(super) log_chunks: BTreeMap<LogChunkId, LogChunkManifest>,
  pub(super) completions: BTreeMap<JobId, CompletionRecord>,
  pub(super) cancellations: BTreeMap<BuildId, CancellationRecord>,
  pub(super) cancellation_keys: BTreeMap<IdempotencyKey, BuildId>,
  pub(super) retries: BTreeMap<IdempotencyKey, RetryRecord>,
  pub(super) idempotency_outcomes: BTreeSet<String>,
  pub(super) audit_facts: BTreeSet<String>,
  pub(super) outbox_entries: BTreeSet<String>,
  pub(super) committed_log_index_positions: BTreeMap<ProjectId, LogIndexPosition>,
}

pub(super) struct MemoryTransaction<'a> {
  guard: MutexGuard<'a, MemoryState>,
  state: MemoryState,
}

impl<'a> MemoryTransaction<'a> {
  pub(super) fn begin(guard: MutexGuard<'a, MemoryState>) -> Self {
    let state = guard.clone();
    Self { guard, state }
  }

  pub(super) fn commit(mut self) {
    *self.guard = std::mem::take(&mut self.state);
  }
}

impl Deref for MemoryTransaction<'_> {
  type Target = MemoryState;

  fn deref(&self) -> &Self::Target {
    &self.state
  }
}

impl DerefMut for MemoryTransaction<'_> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.state
  }
}

#[derive(Clone, Copy)]
pub(crate) struct PoolEligibility {
  pub(super) enabled: bool,
  pub(super) accepting: bool,
  pub(super) concurrency_limit: usize,
}

impl PoolEligibility {
  pub(crate) const ACCEPTING: Self = Self {
    enabled: true,
    accepting: true,
    concurrency_limit: 1,
  };
  pub(super) const DISABLED: Self = Self {
    enabled: false,
    accepting: true,
    concurrency_limit: 1,
  };
  pub(super) const DRAINING: Self = Self {
    enabled: true,
    accepting: false,
    concurrency_limit: 1,
  };
}

#[derive(Clone)]
pub(crate) struct RegistrationEligibility {
  pub(crate) pool_id: PoolId,
  pub(crate) pool_version: PoolVersion,
  pub(crate) expires_at: Timestamp,
  pub(crate) revoked: bool,
  pub(crate) inventory: Option<octacity_protocol::AgentInventory>,
}

#[derive(Clone)]
pub(super) struct AcceptedRecord {
  pub(super) request: AcceptTrigger,
  pub(super) outcome: AcceptTriggerOutcome,
}

#[derive(Clone)]
pub(super) struct SuppressedRecord {
  pub(super) request: SuppressTrigger,
  pub(super) outcome: SuppressTriggerOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MemoryAttempt {
  pub(super) build_id: BuildId,
  pub(super) number: AttemptNumber,
  pub(super) state: AttemptState,
}

#[derive(Clone)]
pub(super) struct HeartbeatRecord {
  pub(super) request: RenewLease,
  pub(super) outcome: LeaseHeartbeatOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MemoryJob {
  pub(super) attempt_id: AttemptId,
  pub(super) materialized: MaterializedJob,
  pub(super) state: JobState,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ReadyEntry {
  pub(super) enqueue_order: u64,
  pub(super) job_id: JobId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CompletionRecord {
  pub(super) request: JobCompletion,
  pub(super) outcome: CompletionDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CancellationRecord {
  pub(super) outcome: CancellationDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RetryRecord {
  pub(super) request: RetryBuild,
  pub(super) outcome: RetryDisposition,
}

#[derive(Clone)]
pub(super) struct ScheduleMemoryRecord {
  pub(super) record: ScheduleRecord,
  pub(super) idempotency_key: IdempotencyKey,
  pub(super) claim: Option<(WorkerOwner, Timestamp)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ClaimRecord {
  pub(super) request: JobClaim,
  pub(super) outcome: JobClaimOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EventAppendRecord {
  pub(super) request: AppendJobEvents,
  pub(super) outcome: AppendJobEventsOutcome,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct TriggerPrerequisites {
  trigger_id: TriggerId,
  trigger_version: TriggerVersion,
  trigger_kind: TriggerKind,
  project_id: ProjectId,
  configuration_id: BuildConfigurationId,
  configuration_version: BuildConfigurationVersion,
  pipeline_id: PipelineId,
  pipeline_version: PipelineVersion,
  repository_id: RepositoryId,
  repository_version: RepositoryVersion,
}

impl TriggerPrerequisites {
  pub(super) fn from_request(request: &AcceptTrigger) -> Self {
    Self {
      trigger_id: request.trigger.trigger.id,
      trigger_version: request.trigger.trigger.version,
      trigger_kind: request.trigger.cause.kind(),
      project_id: request.build.project_id,
      configuration_id: request.build.configuration_id,
      configuration_version: request.build.configuration_version,
      pipeline_id: request.build.pipeline_id,
      pipeline_version: request.build.pipeline_version,
      repository_id: request.build.repository_id,
      repository_version: request.build.repository_version,
    }
  }

  pub(super) fn matches(&self, occurrence: &NormalizedTriggerOccurrence) -> bool {
    self.trigger_id == occurrence.trigger.id
      && self.trigger_version == occurrence.trigger.version
      && self.trigger_kind == occurrence.cause.kind()
      && self.configuration_id == occurrence.target.configuration_id
      && self.configuration_version == occurrence.target.configuration_version
  }
}
