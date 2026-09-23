//! Deterministic authoritative-store adapter and test-support exports.
//!
//! Production adapters enable the `test-support` feature from their test
//! dependencies and run [`verify_authoritative_store_contract`] unchanged.

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_protocol::{NetworkPolicy, OctaSpec, OutputLimits, RuntimeSpec, RuntimeTarget};
use octacity_server_domain::{
  AgentId, AttemptId, AttemptNumber, BuildConfigurationId, BuildConfigurationVersion, BuildId, EntityKind,
  ImmutableRevision, JobId, LeaseId, PipelineId, PipelineNodeId, PipelineVersion, PoolId, PoolVersion, ProjectId,
  RepositoryId, RepositoryVersion, Timestamp, TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_job::{
  DerivedJobSpec, JobSpecBuildSnapshot, JobSpecPolicySnapshot, JobSpecSigner, JobSpecTemplate, JobState,
  SourcePluginPolicy, derive_job_spec_template, sign_ready_job_spec,
};
use octacity_server_orchestrator::{
  AttemptState, BuildState, JobGraphNode, cancel_job_states, decide_retry, reconcile_cancelled_job_graph,
  reconcile_job_graph,
};
use octacity_server_pipeline::DependencyPolicy;
use serde_json::json;

use crate::test_support::{id, time};
use crate::{
  AcceptTrigger, AcceptTriggerOutcome, AppendJobEvents, AppendJobEventsOutcome, BuildControlStore, CancelBuild,
  CancellationDisposition, CompletionDisposition, EventDigest, EventSequence, IdempotencyKey, ImmutableBuildInput,
  JobClaim, JobClaimOutcome, JobCompletion, JobExecutionStore, LeaseAccess, LeaseGrant, LeaseHeartbeatOutcome,
  LeaseHeartbeatStore, LogIndexPosition, LogIndexWorkStore, MaterializedJob, MaterializedJobPayload,
  MutationDisposition, NormalizedTriggerOccurrence, RegistrationEpoch, RenewLease, RetryBuild, RetryDisposition,
  ScheduleRecord, StoreError, StoreOperation, SuppressTrigger, SuppressTriggerOutcome, TriggerAcceptanceProbe,
  TriggerAcceptanceStore, TriggerCause, TriggerDeduplicationKey, TriggerDefinitionRef, TriggerEvaluationOutcome,
  TriggerIntentDigest, TriggerKind, TriggerMetadata, TriggerTarget, WorkerOwner, complete_job_state,
  retry_graph_is_equivalent, start_job_execution,
};

mod build_control;
mod fixtures;
mod lease_events;
mod schedule;
mod state;
mod trigger;

pub(crate) use fixtures::trigger_request;
pub use fixtures::{
  compatible_inventory, compatible_snapshot, job_spec_template, job_spec_template_with_cache, retry_request,
};
pub(crate) use state::*;

pub use crate::agent_testing::InMemoryAgentStore;
pub use crate::artifact_contract_testing::{
  ArtifactRecordStoreContractFixture, ArtifactUploadStoreContractFixture, verify_artifact_record_store_contract,
  verify_artifact_upload_store_contract,
};
pub use crate::artifact_testing::{ArtifactLeaseFixture, InMemoryArtifactRecordStore};
pub use crate::authoritative_contract_testing::{verify_authoritative_store_contract, verify_in_memory_store_contract};
pub use crate::cache_contract_testing::{CacheSessionStoreContractFixture, verify_cache_session_store_contract};
pub use crate::cache_testing::{CacheLeaseFixture, InMemoryCacheSessionStore};
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
pub use crate::pool_contract_testing::{verify_agent_pool_store_contract, verify_in_memory_agent_pool_store_contract};
pub use crate::pool_testing::{InMemoryAgentPoolStore, PoolReferenceKind};
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

/// Test-only failure stage for an accepted mutation transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationFailurePoint {
  /// Fail after staging the domain-state change.
  DomainState,
  /// Fail after staging the durable idempotency outcome.
  IdempotencyOutcome,
  /// Fail after staging the immutable audit fact.
  AuditFact,
  /// Fail after staging the required outbox entry.
  OutboxEntry,
  /// Fail immediately before publishing the staged transaction.
  Commit,
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
pub struct InMemoryStore {
  state: Mutex<MemoryState>,
  job_spec_signer: Arc<JobSpecSigner>,
}

impl Default for InMemoryStore {
  fn default() -> Self {
    Self::with_signer(Arc::new(
      JobSpecSigner::new("contract-key", [7; 32]).expect("fixed contract signing key is valid"),
    ))
  }
}

impl InMemoryStore {
  /// Creates an empty deterministic store.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Creates an empty store with the explicit signer used at Ready boundaries.
  #[must_use]
  pub fn with_signer(job_spec_signer: Arc<JobSpecSigner>) -> Self {
    Self {
      state: Mutex::new(MemoryState::default()),
      job_spec_signer,
    }
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
          inventory: Some(fixtures::compatible_inventory(agent_id)),
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
impl TriggerAcceptanceStore for InMemoryStore {
  async fn replay_trigger_acceptance(
    &self,
    request: TriggerAcceptanceProbe,
  ) -> Result<Option<TriggerEvaluationOutcome>, StoreError> {
    trigger::replay(self, request).await
  }

  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
    trigger::accept(self, request).await
  }

  async fn suppress_trigger(&self, request: SuppressTrigger) -> Result<SuppressTriggerOutcome, StoreError> {
    trigger::suppress(self, request).await
  }
}

#[async_trait]
impl JobExecutionStore for InMemoryStore {
  async fn claim_ready_job(&self, request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
    lease_events::claim(self, request).await
  }

  async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
    lease_events::append_events(self, request).await
  }

  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    lease_events::complete(self, request).await
  }
}

#[async_trait]
impl LeaseHeartbeatStore for InMemoryStore {
  async fn renew_lease(&self, request: RenewLease) -> Result<LeaseHeartbeatOutcome, StoreError> {
    lease_events::renew(self, request).await
  }
}

#[async_trait]
impl BuildControlStore for InMemoryStore {
  async fn cancel_build(&self, request: CancelBuild) -> Result<CancellationDisposition, StoreError> {
    build_control::cancel(self, request).await
  }

  async fn retry_build(&self, request: RetryBuild) -> Result<RetryDisposition, StoreError> {
    build_control::retry(self, request).await
  }
}

#[async_trait]
impl LogIndexWorkStore for InMemoryStore {
  async fn committed_log_index_position(&self, project_id: ProjectId) -> Result<Option<LogIndexPosition>, StoreError> {
    Ok(self.lock()?.committed_log_index_positions.get(&project_id).copied())
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

fn sign_ready_job(
  state: &mut MemoryState,
  signer: &JobSpecSigner,
  job_id: JobId,
  issued_at: Timestamp,
) -> Result<(), StoreError> {
  let job = state.jobs.get(&job_id).ok_or(StoreError::Unavailable)?;
  let attempt = state.attempts.get(&job.attempt_id).ok_or(StoreError::Unavailable)?;
  let signed = sign_ready_job_spec(
    &job.materialized.job_spec_template,
    attempt.number,
    job_id,
    issued_at,
    signer,
  )
  .map_err(|_| StoreError::Unavailable)?;
  state.signed_job_specs.insert(job_id, signed);
  Ok(())
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
