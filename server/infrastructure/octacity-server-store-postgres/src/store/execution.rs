use async_trait::async_trait;
use octacity_observability::Operation;
use octacity_server_store::*;

use super::{PostgresAuthoritativeStore, PostgresStore};

#[async_trait]
impl TriggerAcceptanceStore for PostgresAuthoritativeStore {
  async fn replay_trigger_acceptance(
    &self,
    request: TriggerAcceptanceProbe,
  ) -> Result<Option<TriggerEvaluationOutcome>, StoreError> {
    crate::telemetry::observe(
      Operation::Verify,
      crate::accept_trigger::replay_evaluation(&self.store.pool, request),
    )
    .await
  }

  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
    crate::telemetry::observe(
      Operation::Accept,
      crate::accept_trigger::execute(&self.store.pool, &self.job_spec_signer, request),
    )
    .await
  }

  async fn suppress_trigger(&self, request: SuppressTrigger) -> Result<SuppressTriggerOutcome, StoreError> {
    crate::telemetry::observe(
      Operation::Complete,
      crate::accept_trigger::suppress(&self.store.pool, request),
    )
    .await
  }
}

#[async_trait]
impl TriggerEvaluationWorkStore for PostgresStore {
  async fn reserve_trigger_evaluation(
    &self,
    request: ReserveTriggerEvaluation,
  ) -> Result<TriggerEvaluationReservation, StoreError> {
    crate::trigger_evaluation::reserve(&self.pool, request).await
  }

  async fn claim_trigger_evaluations(
    &self,
    request: ClaimTriggerEvaluations,
  ) -> Result<Vec<TriggerEvaluationClaim>, StoreError> {
    crate::trigger_evaluation::claim(&self.pool, request).await
  }

  async fn complete_trigger_evaluation(&self, request: CompleteTriggerEvaluation) -> Result<(), StoreError> {
    crate::trigger_evaluation::complete(&self.pool, request).await
  }

  async fn record_trigger_evaluation_revision(
    &self,
    request: RecordTriggerEvaluationRevision,
  ) -> Result<(), StoreError> {
    crate::trigger_evaluation::record_revision(&self.pool, request).await
  }

  async fn fail_trigger_evaluation(&self, request: FailTriggerEvaluation) -> Result<(), StoreError> {
    crate::trigger_evaluation::fail(&self.pool, request).await
  }
}

#[async_trait]
impl JobExecutionStore for PostgresAuthoritativeStore {
  async fn claim_ready_job(&self, request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
    crate::telemetry::observe(Operation::Claim, crate::job_claim::execute(&self.store.pool, request)).await
  }

  async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
    crate::telemetry::observe(Operation::Append, crate::job_events::execute(&self.store.pool, request)).await
  }

  async fn prepare_job_event_append(
    &self,
    lease: octacity_server_store::LeaseAccess,
    accepted_at: octacity_server_domain::Timestamp,
  ) -> Result<octacity_server_store::JobEventAppendPreparation, StoreError> {
    crate::job_events::prepare(&self.store.pool, lease, accepted_at).await
  }

  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    crate::telemetry::observe(
      Operation::Complete,
      crate::job_completion::execute(&self.store.pool, &self.job_spec_signer, request),
    )
    .await
  }
}

#[async_trait]
impl LogChunkManifestStore for PostgresStore {
  async fn log_chunk_is_committed(&self, chunk_id: octacity_server_domain::LogChunkId) -> Result<bool, StoreError> {
    crate::job_events::is_chunk_committed(&self.pool, chunk_id).await
  }
}

#[async_trait]
impl LogChunkManifestStore for PostgresAuthoritativeStore {
  async fn log_chunk_is_committed(&self, chunk_id: octacity_server_domain::LogChunkId) -> Result<bool, StoreError> {
    crate::job_events::is_chunk_committed(&self.store.pool, chunk_id).await
  }
}

#[async_trait]
impl OrphanLogChunkStore for PostgresAuthoritativeStore {
  async fn stage_orphan_log_chunk(&self, request: StageOrphanLogChunk) -> Result<(), StoreError> {
    crate::retention::stage_orphan(&self.store.pool, request).await
  }

  async fn claim_orphan_log_chunks(
    &self,
    request: ClaimOrphanLogChunks,
  ) -> Result<Vec<OrphanLogChunkClaim>, StoreError> {
    crate::retention::claim_orphans(&self.store.pool, request).await
  }

  async fn complete_orphan_log_chunk(&self, request: CompleteOrphanLogChunk) -> Result<(), StoreError> {
    crate::retention::complete_orphan(&self.store.pool, request).await
  }

  async fn fail_orphan_log_chunk(&self, request: FailOrphanLogChunk) -> Result<(), StoreError> {
    crate::retention::fail_orphan(&self.store.pool, request).await
  }
}

#[async_trait]
impl LeaseHeartbeatStore for PostgresStore {
  async fn renew_lease(&self, request: RenewLease) -> Result<LeaseHeartbeatOutcome, StoreError> {
    crate::telemetry::observe(Operation::Renew, crate::lease_heartbeat::execute(&self.pool, request)).await
  }
}

#[async_trait]
impl InternalTriggerEventStore for PostgresStore {
  async fn claim_internal_trigger_events(
    &self,
    request: ClaimInternalTriggerEvents,
  ) -> Result<Vec<InternalTriggerEventClaim>, StoreError> {
    crate::telemetry::observe(Operation::Claim, crate::internal_trigger::claim(&self.pool, request)).await
  }

  async fn complete_internal_trigger_event(
    &self,
    request: CompleteInternalTriggerEvent,
  ) -> Result<MutationDisposition, StoreError> {
    crate::telemetry::observe(
      Operation::Complete,
      crate::internal_trigger::complete(&self.pool, request),
    )
    .await
  }
}

#[async_trait]
impl InternalTriggerDefinitionStore for PostgresStore {
  async fn create_internal_trigger_definition(
    &self,
    request: CreateInternalTriggerDefinition,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    crate::internal_trigger_definition::create(&self.pool, request).await
  }

  async fn publish_internal_trigger_version(
    &self,
    request: PublishInternalTriggerVersion,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    crate::internal_trigger_definition::publish(&self.pool, request).await
  }

  async fn internal_trigger_definition(
    &self,
    trigger_id: octacity_server_domain::TriggerId,
    version: octacity_server_domain::TriggerVersion,
  ) -> Result<InternalTriggerDefinitionRecord, StoreError> {
    crate::internal_trigger_definition::read(&self.pool, trigger_id, version).await
  }

  async fn list_internal_trigger_definitions(
    &self,
    request: ListInternalTriggerDefinitions,
  ) -> Result<InternalTriggerDefinitionPage, StoreError> {
    crate::internal_trigger_definition::list(&self.pool, request).await
  }
}

#[async_trait]
impl LeaseHeartbeatStore for PostgresAuthoritativeStore {
  async fn renew_lease(&self, request: RenewLease) -> Result<LeaseHeartbeatOutcome, StoreError> {
    crate::telemetry::observe(
      Operation::Renew,
      crate::lease_heartbeat::execute(&self.store.pool, request),
    )
    .await
  }
}

#[async_trait]
impl LeaseRecoveryStore for PostgresAuthoritativeStore {
  async fn claim_expired_leases(&self, request: ClaimExpiredLeases) -> Result<Vec<ExpiredLeaseClaim>, StoreError> {
    crate::telemetry::observe(
      Operation::Claim,
      crate::lease_recovery::claim(&self.store.pool, request),
    )
    .await
  }

  async fn recover_expired_lease(
    &self,
    request: RecoverExpiredLease,
  ) -> Result<RecoverExpiredLeaseOutcome, StoreError> {
    crate::telemetry::observe(
      Operation::Retry,
      crate::lease_recovery::recover(&self.store.pool, &self.job_spec_signer, request),
    )
    .await
  }
}

#[async_trait]
impl JobEventReadStore for PostgresStore {
  async fn read_job_events(&self, request: ReadJobEvents) -> Result<JobEventPage, StoreError> {
    crate::job_event_query::read(&self.pool, request).await
  }
}

#[async_trait]
impl JobEventReadStore for PostgresAuthoritativeStore {
  async fn read_job_events(&self, request: ReadJobEvents) -> Result<JobEventPage, StoreError> {
    crate::job_event_query::read(&self.store.pool, request).await
  }
}

#[async_trait]
impl BuildControlStore for PostgresAuthoritativeStore {
  async fn cancel_build(&self, request: CancelBuild) -> Result<CancellationDisposition, StoreError> {
    crate::cancel_build::execute(&self.store.pool, request).await
  }

  async fn retry_build(&self, request: RetryBuild) -> Result<RetryDisposition, StoreError> {
    crate::retry_build::execute(&self.store.pool, &self.job_spec_signer, request).await
  }
}

#[async_trait]
impl BuildQueryStore for PostgresStore {
  async fn build(&self, build_id: octacity_server_domain::BuildId) -> Result<BuildRecord, StoreError> {
    crate::build_query::build(&self.pool, build_id).await
  }

  async fn attempt(
    &self,
    attempt_id: octacity_server_domain::AttemptId,
  ) -> Result<octacity_server_store::AttemptRecord, StoreError> {
    crate::build_query::attempt(&self.pool, attempt_id).await
  }

  async fn job(&self, job_id: octacity_server_domain::JobId) -> Result<octacity_server_store::JobRecord, StoreError> {
    crate::build_query::job(&self.pool, job_id).await
  }

  async fn latest_attempt(
    &self,
    build_id: octacity_server_domain::BuildId,
  ) -> Result<octacity_server_store::AttemptRecord, StoreError> {
    crate::build_query::latest_attempt(&self.pool, build_id).await
  }
}

#[async_trait]
impl BuildQueryStore for PostgresAuthoritativeStore {
  async fn build(&self, build_id: octacity_server_domain::BuildId) -> Result<BuildRecord, StoreError> {
    self.store.build(build_id).await
  }

  async fn attempt(
    &self,
    attempt_id: octacity_server_domain::AttemptId,
  ) -> Result<octacity_server_store::AttemptRecord, StoreError> {
    self.store.attempt(attempt_id).await
  }

  async fn job(&self, job_id: octacity_server_domain::JobId) -> Result<octacity_server_store::JobRecord, StoreError> {
    self.store.job(job_id).await
  }

  async fn latest_attempt(
    &self,
    build_id: octacity_server_domain::BuildId,
  ) -> Result<octacity_server_store::AttemptRecord, StoreError> {
    self.store.latest_attempt(build_id).await
  }
}
