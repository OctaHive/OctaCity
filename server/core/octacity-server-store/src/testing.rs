//! Deterministic authoritative-store adapter and test-support exports.
//!
//! Production adapters enable the `test-support` feature from their test
//! dependencies and run [`verify_authoritative_store_contract`] unchanged.

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{
  AgentId, AttemptId, AttemptNumber, BuildConfigurationId, BuildConfigurationVersion, BuildId, EntityKind, JobId,
  LeaseId, PipelineId, PipelineNodeId, PipelineVersion, PoolId, PoolVersion, ProjectId, RepositoryId,
  RepositoryVersion, Timestamp, TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_orchestrator::{DependencyObservation, newly_ready_jobs};
use octacity_server_pipeline::{DependencyOutcome, DependencyPolicy};
use serde_json::json;

use crate::test_support::{id, time};
use crate::{
  AcceptTrigger, AcceptTriggerOutcome, AppendJobEvents, AppendJobEventsOutcome, AuthoritativeStore,
  CompletionDisposition, EventDigest, EventSequence, ImmutableBuildInput, JobClaim, JobClaimOutcome, JobCompletion,
  JobCompletionKind, LeaseAccess, LeaseGrant, LogIndexPosition, LogIndexWorkStore, MaterializedJob,
  MutationDisposition, NormalizedTriggerOccurrence, RegistrationEpoch, StoreError, StoreOperation, TriggerCause,
  TriggerDeduplicationKey, TriggerDefinitionRef, TriggerKind, TriggerMetadata, TriggerTarget,
};

pub use crate::authoritative_contract_testing::{verify_authoritative_store_contract, verify_in_memory_store_contract};
pub use crate::configuration_contract_testing::{
  verify_configuration_store_contract, verify_in_memory_configuration_store_contract,
};
pub use crate::configuration_testing::InMemoryConfigurationStore;
pub use crate::credential_contract_testing::{
  agent_credential_store_contract_fixture, verify_agent_credential_store_contract,
  verify_in_memory_agent_credential_contract,
};
pub use crate::log_search_contract_testing::{
  verify_in_memory_log_search_index_contract, verify_log_search_index_contract,
};
pub use crate::log_search_testing::InMemoryLogSearchIndex;
pub use crate::pipeline_contract_testing::{verify_in_memory_pipeline_store_contract, verify_pipeline_store_contract};
pub use crate::pipeline_testing::InMemoryPipelineStore;
pub use crate::project_contract_testing::{verify_in_memory_project_store_contract, verify_project_store_contract};
pub use crate::project_testing::InMemoryProjectStore;

/// Counts of transactional evidence produced by accepted mutations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MutationEvidenceCounts {
  /// Durable idempotency outcomes.
  pub idempotency: usize,
  /// Immutable audit facts.
  pub audit: usize,
  /// Transactional outbox entries.
  pub outbox: usize,
}

/// Test-only observation seam used by reusable adapter contracts.
///
/// Production interfaces deliberately do not expose persistence internals.
#[async_trait]
pub trait MutationEvidenceProbe: Send + Sync {
  /// Returns evidence counts after the adapter has committed a contract run.
  async fn mutation_evidence_counts(&self) -> MutationEvidenceCounts;
}

/// Deterministic process-local adapter for application and contract tests.
///
/// One mutex represents one serializable authoritative transaction boundary;
/// no timing, random identity, external process, filesystem, or network state
/// participates in its behavior.
#[derive(Default)]
pub struct InMemoryStore {
  state: Mutex<MemoryState>,
}

impl InMemoryStore {
  /// Creates an empty deterministic store.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Seeds only the Pool and registration prerequisites used by the shared contract.
  pub fn seed_authoritative_contract_prerequisites(&self, fixture: &StoreContractFixture) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    state
      .trigger_prerequisites
      .insert(TriggerPrerequisites::from_request(&fixture.request));
    state.pools.extend([
      ((fixture.allowed_pool, PoolVersion::INITIAL), PoolEligibility::ACCEPTING),
      ((fixture.other_pool, PoolVersion::INITIAL), PoolEligibility::ACCEPTING),
      ((fixture.disabled_pool, PoolVersion::INITIAL), PoolEligibility::DISABLED),
      ((fixture.draining_pool, PoolVersion::INITIAL), PoolEligibility::DRAINING),
    ]);
    for (agent_id, pool_id, expires_at, revoked) in [
      (fixture.agent_id, fixture.allowed_pool, time(10_000), false),
      (fixture.other_agent_id, fixture.other_pool, time(10_000), false),
      (fixture.disabled_agent_id, fixture.disabled_pool, time(10_000), false),
      (fixture.draining_agent_id, fixture.draining_pool, time(10_000), false),
      (fixture.expired_agent_id, fixture.allowed_pool, time(1_000), false),
      (fixture.revoked_agent_id, fixture.allowed_pool, time(10_000), true),
    ] {
      state.registrations.insert(
        (agent_id, fixture.registration_epoch),
        RegistrationEligibility {
          pool_id,
          pool_version: PoolVersion::INITIAL,
          expires_at,
          revoked,
        },
      );
    }
    Ok(())
  }

  /// Seeds one immutable Pool version used by Agent-credential contracts.
  pub fn seed_agent_pool(&self, pool_id: PoolId, pool_version: PoolVersion) -> Result<(), StoreError> {
    self
      .lock()?
      .pools
      .insert((pool_id, pool_version), PoolEligibility::ACCEPTING);
    Ok(())
  }

  /// Seeds the durable indexing-work watermark used by application tests.
  pub fn seed_committed_log_index_position(
    &self,
    project_id: ProjectId,
    position: LogIndexPosition,
  ) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    state
      .committed_log_index_positions
      .entry(project_id)
      .and_modify(|current| *current = (*current).max(position))
      .or_insert(position);
    Ok(())
  }

  pub(crate) fn lock(&self) -> Result<MutexGuard<'_, MemoryState>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[async_trait]
impl MutationEvidenceProbe for InMemoryStore {
  async fn mutation_evidence_counts(&self) -> MutationEvidenceCounts {
    let state = self.lock().expect("in-memory contract state must remain available");
    MutationEvidenceCounts {
      idempotency: state.idempotency_outcomes.len(),
      audit: state.audit_facts.len(),
      outbox: state.outbox_entries.len(),
    }
  }
}

#[derive(Default)]
pub(crate) struct MemoryState {
  pub(crate) credentials: crate::credential_testing::CredentialMemoryState,
  trigger_prerequisites: BTreeSet<TriggerPrerequisites>,
  accepted: BTreeMap<TriggerOccurrenceId, AcceptedRecord>,
  accepted_by_deduplication: BTreeMap<TriggerDeduplicationKey, TriggerOccurrenceId>,
  builds: BTreeSet<BuildId>,
  attempts: BTreeSet<AttemptId>,
  jobs: BTreeMap<JobId, MaterializedJob>,
  ready_queue: BTreeSet<ReadyEntry>,
  ready_jobs: BTreeSet<JobId>,
  next_enqueue_order: u64,
  leases: BTreeMap<LeaseId, LeaseGrant>,
  pub(crate) pools: BTreeMap<(PoolId, PoolVersion), PoolEligibility>,
  pub(crate) registrations: BTreeMap<(AgentId, RegistrationEpoch), RegistrationEligibility>,
  claims: BTreeMap<LeaseId, ClaimRecord>,
  current_lease_by_job: BTreeMap<JobId, LeaseId>,
  events: BTreeMap<JobId, BTreeMap<EventSequence, EventDigest>>,
  event_appends: BTreeMap<(LeaseId, EventSequence, EventSequence), EventAppendRecord>,
  completions: BTreeMap<JobId, CompletionRecord>,
  idempotency_outcomes: BTreeSet<String>,
  audit_facts: BTreeSet<String>,
  outbox_entries: BTreeSet<String>,
  committed_log_index_positions: BTreeMap<ProjectId, LogIndexPosition>,
}

#[derive(Clone, Copy)]
pub(crate) struct PoolEligibility {
  enabled: bool,
  accepting: bool,
}

impl PoolEligibility {
  pub(crate) const ACCEPTING: Self = Self {
    enabled: true,
    accepting: true,
  };
  const DISABLED: Self = Self {
    enabled: false,
    accepting: true,
  };
  const DRAINING: Self = Self {
    enabled: true,
    accepting: false,
  };
}

#[derive(Clone, Copy)]
pub(crate) struct RegistrationEligibility {
  pub(crate) pool_id: PoolId,
  pub(crate) pool_version: PoolVersion,
  pub(crate) expires_at: Timestamp,
  pub(crate) revoked: bool,
}

#[derive(Clone)]
struct AcceptedRecord {
  request: AcceptTrigger,
  outcome: AcceptTriggerOutcome,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReadyEntry {
  enqueue_order: u64,
  job_id: JobId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionRecord {
  request: JobCompletion,
  outcome: CompletionDisposition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ClaimRecord {
  request: JobClaim,
  outcome: JobClaimOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EventAppendRecord {
  request: AppendJobEvents,
  outcome: AppendJobEventsOutcome,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TriggerPrerequisites {
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
  fn from_request(request: &AcceptTrigger) -> Self {
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
}

/// Deterministic prerequisite identities used by the reusable adapter contract.
#[derive(Clone)]
pub struct StoreContractFixture {
  /// First complete Trigger acceptance exercised by the contract.
  pub request: AcceptTrigger,
  /// Pool accepted by the materialized Jobs and registered Agent.
  pub allowed_pool: PoolId,
  /// Existing Pool that is intentionally incompatible with the Jobs.
  pub other_pool: PoolId,
  /// Existing disabled Pool that is allowed by the Jobs but cannot accept work.
  pub disabled_pool: PoolId,
  /// Existing draining Pool that is allowed by the Jobs but cannot accept work.
  pub draining_pool: PoolId,
  /// Stable Agent used for placement and fenced mutations.
  pub agent_id: AgentId,
  /// Stable Agent registered in the intentionally incompatible Pool.
  pub other_agent_id: AgentId,
  /// Agent assigned to the disabled Pool.
  pub disabled_agent_id: AgentId,
  /// Agent assigned to the draining Pool.
  pub draining_agent_id: AgentId,
  /// Agent whose otherwise valid registration has expired.
  pub expired_agent_id: AgentId,
  /// Agent whose otherwise valid registration was revoked.
  pub revoked_agent_id: AgentId,
  /// Current registration epoch used by the Agent.
  pub registration_epoch: RegistrationEpoch,
}

/// Builds deterministic prerequisite data for an adapter contract run.
#[must_use]
pub fn authoritative_store_contract_fixture() -> StoreContractFixture {
  let allowed_pool = id::<PoolId>(1);
  let disabled_pool = id::<PoolId>(3);
  let draining_pool = id::<PoolId>(4);
  let mut request = trigger_request(10, 100, allowed_pool);
  for job in &mut request.jobs {
    job.allowed_pools.extend([disabled_pool, draining_pool]);
    job.allowed_pools.sort_unstable();
  }
  StoreContractFixture {
    request,
    allowed_pool,
    other_pool: id::<PoolId>(2),
    disabled_pool,
    draining_pool,
    agent_id: id::<AgentId>(20),
    other_agent_id: id::<AgentId>(21),
    disabled_agent_id: id::<AgentId>(22),
    draining_agent_id: id::<AgentId>(23),
    expired_agent_id: id::<AgentId>(24),
    revoked_agent_id: id::<AgentId>(25),
    registration_epoch: RegistrationEpoch::new(1).unwrap(),
  }
}

#[async_trait]
impl AuthoritativeStore for InMemoryStore {
  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
    request.validate()?;
    let mut state = self.lock()?;
    if !state
      .trigger_prerequisites
      .contains(&TriggerPrerequisites::from_request(&request))
    {
      return Err(StoreError::NotFound {
        entity: EntityKind::Trigger,
      });
    }
    if request
      .jobs
      .iter()
      .flat_map(|job| &job.allowed_pools)
      .any(|pool| !state.pools.keys().any(|(pool_id, _)| pool_id == pool))
    {
      return Err(StoreError::NotFound {
        entity: EntityKind::Pool,
      });
    }
    let deduplication_key = request.trigger.deduplication_key();
    let existing_occurrence = state
      .accepted
      .contains_key(&request.trigger.id)
      .then_some(request.trigger.id)
      .or_else(|| state.accepted_by_deduplication.get(&deduplication_key).copied());
    if let Some(existing_occurrence) = existing_occurrence {
      let existing = state
        .accepted
        .get(&existing_occurrence)
        .ok_or(StoreError::Unavailable)?;
      if same_trigger_acceptance(&existing.request, &request) {
        let mut outcome = existing.outcome.clone();
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      return Err(StoreError::Conflict {
        entity: EntityKind::Trigger,
      });
    }

    if state.builds.contains(&request.build.id) {
      return Err(StoreError::Conflict {
        entity: EntityKind::Build,
      });
    }
    if state.attempts.contains(&request.attempt_id) {
      return Err(StoreError::Conflict {
        entity: EntityKind::Attempt,
      });
    }
    if request.jobs.iter().any(|job| state.jobs.contains_key(&job.id)) {
      return Err(StoreError::Conflict {
        entity: EntityKind::Job,
      });
    }

    let ready_jobs: Vec<_> = request
      .jobs
      .iter()
      .filter(|job| job.dependencies.is_empty())
      .map(|job| job.id)
      .collect();
    let outcome = AcceptTriggerOutcome {
      disposition: MutationDisposition::Applied,
      build_id: request.build.id,
      attempt_id: request.attempt_id,
      ready_jobs: ready_jobs.clone(),
    };
    let evidence_identity = format!("accept-trigger:{}", request.trigger.id);
    ensure_evidence_available(&state, &evidence_identity)?;
    ensure_enqueue_capacity(&state, ready_jobs.iter().copied())?;

    state.builds.insert(request.build.id);
    state.attempts.insert(request.attempt_id);
    for job in &request.jobs {
      state.jobs.insert(job.id, job.clone());
    }
    for job_id in ready_jobs {
      enqueue(&mut state, job_id);
    }
    let occurrence_id = request.trigger.id;
    state.accepted.insert(
      occurrence_id,
      AcceptedRecord {
        request,
        outcome: outcome.clone(),
      },
    );
    state.accepted_by_deduplication.insert(deduplication_key, occurrence_id);
    record_evidence(&mut state, evidence_identity);
    Ok(outcome)
  }

  async fn claim_ready_job(&self, request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
    request.validate()?;
    let mut state = self.lock()?;
    if let Some(existing) = state.claims.get(&request.lease_id) {
      if same_job_claim(existing.request, request) {
        return Ok(existing.outcome);
      }
      return Err(StoreError::Conflict {
        entity: EntityKind::Lease,
      });
    }
    let Some(registration) = state.registrations.get(&(request.agent_id, request.registration_epoch)) else {
      return Ok(JobClaimOutcome::Empty);
    };
    let pool = state.pools.get(&(request.pool_id, registration.pool_version));
    if registration.pool_id != request.pool_id
      || registration.revoked
      || registration.expires_at <= request.claimed_at
      || !pool.is_some_and(|pool| pool.enabled && pool.accepting)
    {
      return Ok(JobClaimOutcome::Empty);
    }
    let selected = state.ready_queue.iter().copied().find(|entry| {
      state
        .jobs
        .get(&entry.job_id)
        .is_some_and(|job| job.allowed_pools.binary_search(&request.pool_id).is_ok())
    });
    let Some(selected) = selected else {
      return Ok(JobClaimOutcome::Empty);
    };
    let grant = LeaseGrant {
      lease_id: request.lease_id,
      fence: request.fence,
      job_id: selected.job_id,
      agent_id: request.agent_id,
      registration_epoch: request.registration_epoch,
      pool_id: request.pool_id,
      expires_at: request.expires_at,
    };
    let evidence_identity = format!("claim-ready-job:{}", request.lease_id);
    ensure_evidence_available(&state, &evidence_identity)?;
    state.ready_queue.remove(&selected);
    state.ready_jobs.remove(&selected.job_id);
    state.current_lease_by_job.insert(selected.job_id, request.lease_id);
    state.leases.insert(request.lease_id, grant);
    state.claims.insert(
      request.lease_id,
      ClaimRecord {
        request,
        outcome: JobClaimOutcome::Claimed(grant),
      },
    );
    record_evidence(&mut state, evidence_identity);
    Ok(JobClaimOutcome::Claimed(grant))
  }

  async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
    request.validate()?;
    let mut state = self.lock()?;
    let first = request.events[0].sequence();
    let last = request.events[request.events.len() - 1].sequence();
    let append_key = (request.lease.lease_id, first, last);
    if let Some(existing) = state.event_appends.get(&append_key) {
      if same_event_append(&existing.request, &request) {
        return Ok(existing.outcome);
      }
      return Err(StoreError::Conflict {
        entity: EntityKind::Job,
      });
    }
    let grant = current_grant(&state, request.lease, request.accepted_at)?;
    let events = state.events.get(&grant.job_id);
    let durable_through = events
      .and_then(|events| events.last_key_value().map(|(sequence, _)| sequence.get()))
      .unwrap_or(0);
    let mut expected = durable_through.saturating_add(1);
    let mut new_events = Vec::new();

    for event in &request.events {
      let sequence = event.sequence().get();
      if sequence <= durable_through {
        if events.and_then(|events| events.get(&event.sequence())) != Some(&event.digest()) {
          return Err(StoreError::Conflict {
            entity: EntityKind::Job,
          });
        }
      } else if sequence != expected {
        return Err(StoreError::EventGap {
          job: grant.job_id,
          expected,
          actual: sequence,
        });
      } else {
        new_events.push(event.clone());
        expected = expected.checked_add(1).ok_or(StoreError::Unavailable)?;
      }
    }

    let acknowledged = durable_through
      .checked_add(u64::try_from(new_events.len()).map_err(|_| StoreError::Unavailable)?)
      .ok_or(StoreError::Unavailable)?;
    let acknowledged_through = EventSequence::new(acknowledged).map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::AppendJobEvents,
      source,
    })?;
    let outcome = AppendJobEventsOutcome {
      acknowledged_through,
      inserted: new_events.len(),
    };
    let evidence_identity = format!(
      "append-job-events:{}:{}-{}",
      append_key.0,
      append_key.1.get(),
      append_key.2.get()
    );
    ensure_evidence_available(&state, &evidence_identity)?;
    let events = state.events.entry(grant.job_id).or_default();
    for event in &new_events {
      events.insert(event.sequence(), event.digest());
    }
    state
      .event_appends
      .insert(append_key, EventAppendRecord { request, outcome });
    record_evidence(&mut state, evidence_identity);
    Ok(outcome)
  }

  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    let mut state = self.lock()?;
    let grant = lease_grant(&state, request.lease)?;
    if let Some(existing) = state.completions.get(&grant.job_id) {
      if same_completion(existing.request, request) {
        let mut outcome = existing.outcome.clone();
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      return Err(StoreError::Conflict {
        entity: EntityKind::Job,
      });
    }
    current_grant(&state, request.lease, request.completed_at)?;

    let durable_through = state
      .events
      .get(&grant.job_id)
      .and_then(|events| events.last_key_value().map(|(sequence, _)| sequence.get()))
      .unwrap_or(0);
    let required = request.final_sequence.map_or(0, EventSequence::get);
    if required > durable_through {
      return Err(StoreError::EventsMissing {
        job: grant.job_id,
        durable_through,
        required,
      });
    }
    if required < durable_through {
      return Err(StoreError::Conflict {
        entity: EntityKind::Job,
      });
    }

    let mut observations = Vec::new();
    for job in state.jobs.values().filter(|job| {
      !job.dependencies.is_empty()
        && !state.ready_jobs.contains(&job.id)
        && !state.current_lease_by_job.contains_key(&job.id)
        && !state.completions.contains_key(&job.id)
    }) {
      observations.extend(job.dependencies.iter().map(|dependency| DependencyObservation {
        job_id: job.id,
        policy: job.dependency_policy,
        outcome: if *dependency == grant.job_id {
          dependency_outcome(request.kind)
        } else {
          state
            .completions
            .get(dependency)
            .map_or(DependencyOutcome::Pending, |completion| {
              dependency_outcome(completion.request.kind)
            })
        },
      }));
    }
    let candidates = newly_ready_jobs(observations).map_err(|_| StoreError::Unavailable)?;
    let ready_jobs = candidates;
    let outcome = CompletionDisposition {
      disposition: MutationDisposition::Applied,
      job_id: grant.job_id,
      ready_jobs: ready_jobs.clone(),
    };
    let evidence_identity = format!("complete-job:{}", request.lease.lease_id);
    ensure_evidence_available(&state, &evidence_identity)?;
    ensure_enqueue_capacity(&state, ready_jobs.iter().copied())?;
    state.current_lease_by_job.remove(&grant.job_id);
    for job_id in &ready_jobs {
      enqueue(&mut state, *job_id);
    }
    state.completions.insert(
      grant.job_id,
      CompletionRecord {
        request,
        outcome: outcome.clone(),
      },
    );
    record_evidence(&mut state, evidence_identity);
    Ok(outcome)
  }
}

fn same_trigger_acceptance(left: &AcceptTrigger, right: &AcceptTrigger) -> bool {
  left.trigger == right.trigger
    && left.build == right.build
    && left.attempt_id == right.attempt_id
    && left.attempt_number == right.attempt_number
    && left.jobs == right.jobs
}

fn same_job_claim(left: JobClaim, right: JobClaim) -> bool {
  left.lease_id == right.lease_id
    && left.fence == right.fence
    && left.agent_id == right.agent_id
    && left.registration_epoch == right.registration_epoch
    && left.pool_id == right.pool_id
    && left.expires_at == right.expires_at
}

fn same_event_append(left: &AppendJobEvents, right: &AppendJobEvents) -> bool {
  left.lease == right.lease && left.events == right.events
}

fn same_completion(left: JobCompletion, right: JobCompletion) -> bool {
  left.lease == right.lease && left.final_sequence == right.final_sequence && left.kind == right.kind
}

#[async_trait]
impl LogIndexWorkStore for InMemoryStore {
  async fn committed_log_index_position(&self, project_id: ProjectId) -> Result<Option<LogIndexPosition>, StoreError> {
    Ok(self.lock()?.committed_log_index_positions.get(&project_id).copied())
  }
}

const fn dependency_outcome(kind: JobCompletionKind) -> DependencyOutcome {
  match kind {
    JobCompletionKind::Succeeded => DependencyOutcome::Succeeded,
    JobCompletionKind::Failed => DependencyOutcome::Failed,
    JobCompletionKind::Cancelled => DependencyOutcome::Cancelled,
  }
}

fn ensure_enqueue_capacity(state: &MemoryState, jobs: impl IntoIterator<Item = JobId>) -> Result<(), StoreError> {
  let additional = jobs
    .into_iter()
    .filter(|job_id| !state.ready_jobs.contains(job_id))
    .collect::<BTreeSet<_>>()
    .len() as u64;
  state
    .next_enqueue_order
    .checked_add(additional)
    .ok_or(StoreError::Unavailable)?;
  Ok(())
}

fn enqueue(state: &mut MemoryState, job_id: JobId) {
  if state.ready_jobs.insert(job_id) {
    let enqueue_order = state.next_enqueue_order;
    state.next_enqueue_order += 1;
    state.ready_queue.insert(ReadyEntry { enqueue_order, job_id });
  }
}

pub(crate) fn ensure_evidence_available(state: &MemoryState, identity: &str) -> Result<(), StoreError> {
  if state.idempotency_outcomes.contains(identity)
    || state.audit_facts.contains(identity)
    || state.outbox_entries.contains(identity)
  {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

pub(crate) fn record_evidence(state: &mut MemoryState, identity: String) {
  let idempotency_inserted = state.idempotency_outcomes.insert(identity.clone());
  let audit_inserted = state.audit_facts.insert(identity.clone());
  let outbox_inserted = state.outbox_entries.insert(identity);
  debug_assert!(idempotency_inserted && audit_inserted && outbox_inserted);
}

fn lease_grant(state: &MemoryState, access: LeaseAccess) -> Result<LeaseGrant, StoreError> {
  let grant = state
    .leases
    .get(&access.lease_id)
    .copied()
    .ok_or(StoreError::Fenced { lease: access.lease_id })?;
  if grant.fence != access.fence
    || grant.agent_id != access.agent_id
    || grant.registration_epoch != access.registration_epoch
  {
    return Err(StoreError::Fenced { lease: access.lease_id });
  }
  Ok(grant)
}

fn current_grant(state: &MemoryState, access: LeaseAccess, observed_at: Timestamp) -> Result<LeaseGrant, StoreError> {
  let grant = lease_grant(state, access)?;
  if observed_at >= grant.expires_at {
    return Err(StoreError::Expired { lease: access.lease_id });
  }
  if state.current_lease_by_job.get(&grant.job_id) != Some(&grant.lease_id) {
    return Err(StoreError::Fenced { lease: access.lease_id });
  }
  Ok(grant)
}

pub(crate) fn trigger_request(occurrence: u64, base: u64, pool: PoolId) -> AcceptTrigger {
  let root = MaterializedJob::new(
    id(base + 3),
    PipelineNodeId::new("root").unwrap(),
    Vec::new(),
    DependencyPolicy::AllSucceeded,
    vec![pool],
    json!({"command": "root"}),
    json!({"platform": "linux"}),
  )
  .unwrap();
  let child = MaterializedJob::new(
    id(base + 4),
    PipelineNodeId::new("child").unwrap(),
    vec![root.id],
    DependencyPolicy::AllSucceeded,
    vec![pool],
    json!({"command": "child"}),
    json!({"platform": "linux"}),
  )
  .unwrap();
  let build = ImmutableBuildInput {
    id: id(base + 1),
    project_id: id::<ProjectId>(31),
    configuration_id: id::<BuildConfigurationId>(32),
    configuration_version: BuildConfigurationVersion::INITIAL,
    pipeline_id: id::<PipelineId>(33),
    pipeline_version: PipelineVersion::INITIAL,
    repository_id: id::<RepositoryId>(34),
    repository_version: RepositoryVersion::INITIAL,
    immutable_revision: "0123456789abcdef".to_owned(),
    input_snapshot: json!({"parameter": "value"}),
    effective_policy_snapshot: json!({"allowed_pool": pool.to_string()}),
    priority: 10,
  };
  let trigger = NormalizedTriggerOccurrence::root(
    id(occurrence),
    TriggerDefinitionRef {
      id: id::<TriggerId>(30),
      version: TriggerVersion::INITIAL,
    },
    TriggerTarget {
      configuration_id: build.configuration_id,
      configuration_version: build.configuration_version,
    },
    TriggerIdentity::new(format!("manual:{occurrence}")).unwrap(),
    TriggerCause::Manual {},
    TriggerMetadata::default(),
    time(400),
  )
  .unwrap();
  AcceptTrigger::new(
    trigger,
    build,
    id(base + 2),
    AttemptNumber::FIRST,
    vec![root, child],
    time(500),
  )
  .unwrap()
}
