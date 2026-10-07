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
  ImmutableRevision, JobId, LeaseId, LogChunkId, PipelineId, PipelineNodeId, PipelineVersion, PoolId, PoolVersion,
  ProjectId, RepositoryId, RepositoryVersion, Timestamp, TriggerId, TriggerIdentity, TriggerOccurrenceId,
  TriggerVersion,
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

use crate::ManagementIdempotencyKey;
use crate::test_support::{id, time};
use crate::{
  AcceptTrigger, AcceptTriggerOutcome, AppendJobEvents, AppendJobEventsOutcome, AuditActorKind, BuildControlStore,
  CancelBuild, CancellationDisposition, CompletionDisposition, EventDigest, EventSequence, IdempotencyKey,
  ImmutableBuildInput, JobClaim, JobClaimOutcome, JobCompletion, JobEventAppendPreparation, JobExecutionStore,
  LeaseAccess, LeaseGrant, LeaseHeartbeatOutcome, LeaseHeartbeatStore, LogChunkManifest, LogChunkManifestStore,
  LogIndexPosition, LogIndexWorkStore, ManagementMutation, ManagementSecurityScope, MaterializedJob,
  MaterializedJobPayload, MutationAuditContext, MutationDisposition, NormalizedTriggerOccurrence, RegistrationEpoch,
  RenewLease, RetryBuild, RetryDisposition, ScheduleRecord, StoreError, StoreOperation, SuppressTrigger,
  SuppressTriggerOutcome, TriggerAcceptanceProbe, TriggerAcceptanceStore, TriggerCause, TriggerDeduplicationKey,
  TriggerDefinitionRef, TriggerEvaluationOutcome, TriggerIntentDigest, TriggerKind, TriggerMetadata, TriggerTarget,
  WorkerOwner, complete_job_state, retry_graph_is_equivalent, start_job_execution, validate_new_log_chunks,
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
pub use crate::authoritative_contract_testing::{
  verify_authoritative_store_contract, verify_in_memory_store_contract, verify_management_trigger_audit_contract,
};
pub use crate::build_discovery_contract_testing::verify_in_memory_build_discovery_contract;
pub use crate::build_discovery_testing::InMemoryBuildDiscoveryStore;
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
pub use crate::definition_discovery_contract_testing::{
  verify_in_memory_definition_discovery_contract, verify_seeded_configuration_definition_discovery_contract,
  verify_seeded_pipeline_definition_discovery_contract, verify_seeded_trigger_definition_discovery_contract,
};
pub use crate::definition_discovery_testing::InMemoryTriggerDefinitionDiscoveryStore;
pub use crate::factory_configuration_testing::InMemoryFactoryConfigurationStore;
pub use crate::log_search_contract_testing::{
  verify_in_memory_log_search_index_contract, verify_log_search_index_contract,
};
pub use crate::log_search_testing::InMemoryLogSearchIndex;
pub use crate::operator_attention_testing::{InMemoryOperatorAttentionStore, OperatorAttentionSeedError};
pub use crate::pipeline_contract_testing::{verify_in_memory_pipeline_store_contract, verify_pipeline_store_contract};
pub use crate::pipeline_testing::InMemoryPipelineStore;
pub use crate::pool_contract_testing::{verify_agent_pool_store_contract, verify_in_memory_agent_pool_store_contract};
pub use crate::pool_testing::{InMemoryAgentPoolStore, PoolReferenceKind};
pub use crate::project_contract_testing::{
  verify_in_memory_project_store_contract, verify_project_store_contract, verify_security_scoped_project_replay,
};
pub use crate::project_testing::InMemoryProjectStore;
pub use crate::resource_search_testing::{InMemoryResourceSearchStore, ResourceSearchSeedError};

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

/// Actor-faithful audit evidence observed from one in-memory management mutation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RecordedManagementAuditFact {
  /// Accepted management actor classification.
  pub actor_kind: AuditActorKind,
  /// Verified actor identity when the accepted context carried one.
  pub actor_identity: Option<String>,
  /// Typed authoritative operation that committed.
  pub operation: StoreOperation,
  /// Typed entity targeted by the operation.
  pub target_kind: EntityKind,
  /// Stable logical target identity.
  pub target_identity: String,
  /// Safe request correlation identity accepted at ingress.
  pub request_identity: String,
}

/// Test-only observation seam for actor-faithful management audit evidence.
#[async_trait]
pub trait ManagementAuditProbe: Send + Sync {
  /// Returns immutable management facts in deterministic order.
  async fn management_audit_facts(&self) -> Vec<RecordedManagementAuditFact>;
}

/// Builds the canonical fact expected from the deterministic contract actor.
#[must_use]
pub fn expected_management_audit(
  operation: StoreOperation,
  target_kind: EntityKind,
  target_identity: impl ToString,
) -> RecordedManagementAuditFact {
  RecordedManagementAuditFact {
    actor_kind: AuditActorKind::UnauthenticatedManagement,
    actor_identity: None,
    operation,
    target_kind,
    target_identity: target_identity.to_string(),
    request_identity: "contract-request".to_owned(),
  }
}

/// Requires one exact, duplicate-free management fact for every accepted
/// mutation described by a reusable store contract.
pub async fn assert_management_audit_facts<P>(
  probe: &P,
  expected: impl IntoIterator<Item = RecordedManagementAuditFact>,
) where
  P: ManagementAuditProbe + ?Sized,
{
  let mut expected = expected.into_iter().collect::<Vec<_>>();
  expected.sort_unstable();
  assert_eq!(probe.management_audit_facts().await, expected);
}

pub(crate) fn recorded_management_audit(
  context: &MutationAuditContext,
  operation: StoreOperation,
  target_kind: EntityKind,
  target_identity: impl ToString,
) -> RecordedManagementAuditFact {
  RecordedManagementAuditFact {
    actor_kind: context.actor().kind,
    actor_identity: context.actor().identity.clone(),
    operation,
    target_kind,
    target_identity: target_identity.to_string(),
    request_identity: context.request_identity().to_owned(),
  }
}

/// Binds a contract-test mutation to a deterministic anonymous management request.
#[must_use]
pub fn management_mutation<T>(mutation: T) -> ManagementMutation<T> {
  management_mutation_with_request(mutation, "contract-request")
}

/// Binds a contract-test mutation to a deterministic anonymous management request identity.
#[must_use]
pub fn management_mutation_with_request<T>(mutation: T, request_identity: impl Into<String>) -> ManagementMutation<T> {
  ManagementMutation::new(
    mutation,
    MutationAuditContext::try_new(
      crate::AuditActor {
        kind: AuditActorKind::UnauthenticatedManagement,
        identity: None,
      },
      crate::ManagementSecurityScope::trusted_network(),
      request_identity,
    )
    .expect("contract management audit context is valid"),
  )
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
          execution_contract_version: octacity_protocol::EXECUTION_CONTRACT_V1,
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
impl ManagementAuditProbe for InMemoryStore {
  async fn management_audit_facts(&self) -> Vec<RecordedManagementAuditFact> {
    self
      .lock()
      .expect("in-memory authoritative store must remain available")
      .management_audit_facts
      .iter()
      .cloned()
      .collect()
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
    trigger::accept(self, request, None).await
  }

  async fn accept_management_trigger(
    &self,
    request: ManagementMutation<AcceptTrigger>,
  ) -> Result<AcceptTriggerOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    trigger::accept(self, request, Some(audit)).await
  }

  async fn suppress_trigger(&self, request: SuppressTrigger) -> Result<SuppressTriggerOutcome, StoreError> {
    trigger::suppress(self, request, None).await
  }

  async fn suppress_management_trigger(
    &self,
    request: ManagementMutation<SuppressTrigger>,
  ) -> Result<SuppressTriggerOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    trigger::suppress(self, request, Some(audit)).await
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

  async fn prepare_job_event_append(
    &self,
    lease: LeaseAccess,
    accepted_at: Timestamp,
  ) -> Result<crate::JobEventAppendPreparation, StoreError> {
    lease_events::prepare_append(self, lease, accepted_at).await
  }

  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    lease_events::complete(self, request).await
  }
}

#[async_trait]
impl LogChunkManifestStore for InMemoryStore {
  async fn log_chunk_is_committed(&self, chunk_id: octacity_server_domain::LogChunkId) -> Result<bool, StoreError> {
    Ok(self.lock()?.log_chunks.contains_key(&chunk_id))
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
  async fn cancel_build(
    &self,
    request: ManagementMutation<CancelBuild>,
  ) -> Result<CancellationDisposition, StoreError> {
    let (request, audit) = request.into_parts();
    build_control::cancel(self, request, &audit).await
  }

  async fn retry_build(&self, request: ManagementMutation<RetryBuild>) -> Result<RetryDisposition, StoreError> {
    let (request, audit) = request.into_parts();
    build_control::retry(self, request, &audit).await
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

pub(crate) fn management_evidence_identity(operation: &str, key: &ManagementIdempotencyKey) -> String {
  scoped_management_evidence_identity(operation, key.security_scope(), key.caller_key().as_str())
}

pub(crate) fn scoped_management_evidence_identity(
  operation: &str,
  security_scope: &ManagementSecurityScope,
  caller_key: &str,
) -> String {
  ManagementEvidenceIdentity {
    operation,
    security_scope,
    caller_key,
  }
  .encode()
}

struct ManagementEvidenceIdentity<'a> {
  operation: &'a str,
  security_scope: &'a ManagementSecurityScope,
  caller_key: &'a str,
}

impl ManagementEvidenceIdentity<'_> {
  fn encode(&self) -> String {
    format!(
      "{}:{}{}:{}{}:{}",
      self.operation.len(),
      self.operation,
      self.security_scope.as_str().len(),
      self.security_scope.as_str(),
      self.caller_key.len(),
      self.caller_key,
    )
  }
}

#[cfg(test)]
mod evidence_identity_tests {
  use super::*;

  #[test]
  fn management_evidence_components_cannot_collide_at_delimiters() {
    let left_scope = ManagementSecurityScope::new("a:b").unwrap();
    let right_scope = ManagementSecurityScope::new("a").unwrap();
    assert_ne!(
      scoped_management_evidence_identity("operation", &left_scope, "c"),
      scoped_management_evidence_identity("operation", &right_scope, "b:c")
    );
  }
}
