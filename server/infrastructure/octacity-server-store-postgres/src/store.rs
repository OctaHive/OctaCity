use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, PipelineId, PipelineVersion, PoolId, PoolVersion, ProjectId,
  RepositoryId, RepositoryVersion,
};
use octacity_server_job::JobSpecSigner;
use octacity_server_store::{
  AcceptTrigger, AcceptTriggerOutcome, AgentCredentialStore, AgentPage, AgentPoolMutationOutcome, AgentPoolPage,
  AgentPoolStore, AgentRegistrationOutcome, AgentStore, AppendJobEvents, AppendJobEventsOutcome, ArtifactRecord,
  ArtifactRecordStore, ArtifactUploadRecord, ArtifactVerificationResult, AuthenticateAgentRegistration,
  AuthenticatedAgentRegistration, AuthorizeCacheSession, BeginArtifactUpload, BeginArtifactUploadOutcome,
  BeginCacheSession, BeginCacheSessionOutcome, BuildConfigurationMutationOutcome, BuildControlStore, BuildQueryStore,
  BuildRecord, BuildRetentionStore, CacheAuthorizationOutcome, CacheBlobPreparationOutcome, CacheDataAccess,
  CacheDataStore, CachePublicationOutcome, CacheRetentionOutcome, CacheSessionRecord, CacheSessionStore, CancelBuild,
  CancellationDisposition, ClaimDueSchedules, ClaimExpiredLeases, ClaimInternalTriggerEvents, ClaimOrphanLogChunks,
  ClaimRetentionWork, ClaimTriggerEvaluations, CompleteInternalTriggerEvent, CompleteOrphanLogChunk,
  CompleteRetentionObject, CompleteRetentionSearch, CompleteScheduleClaim, CompleteTriggerEvaluation,
  CompletionDisposition, ConfigurationStore, CreateAgentPool, CreateBuildConfiguration,
  CreateInternalTriggerDefinition, CreateManagedWebhook, CreateProject, CreateRepository, CreateSchedule,
  CreateTriggerDefinition, CreateUnmanagedWebhook, DefinitionStore, DeleteAgentPool, DeleteAgentPoolOutcome,
  DeleteProject, DeleteProjectOutcome, DrainAgent, DrainAgentOutcome, DueScheduleClaim, ExpiredLeaseClaim,
  FailOrphanLogChunk, FailRetentionWork, FailTriggerEvaluation, FinishRetentionPass, InternalTriggerDefinitionPage,
  InternalTriggerDefinitionRecord, InternalTriggerDefinitionStore, InternalTriggerEventClaim,
  InternalTriggerEventStore, IssueAgentEnrollment, IssueAgentEnrollmentOutcome, JobClaim, JobClaimOutcome,
  JobCompletion, JobEventPage, JobEventReadStore, JobExecutionStore, LeaseHeartbeatOutcome, LeaseHeartbeatStore,
  LeaseRecoveryStore, ListAgentPools, ListAgents, ListBuildCacheSessions, ListInternalTriggerDefinitions, ListProjects,
  ListPublishedArtifacts, LogChunkManifestStore, ManagedWebhookMutationOutcome, ManagedWebhookOperationStore,
  ManagedWebhookRecord, ManagedWebhookRegistrationStore, MoveProject, MutationDisposition, OrphanLogChunkClaim,
  OrphanLogChunkStore, PipelineMutationOutcome, PipelineStore, PrepareRetentionWork, ProjectDetails,
  ProjectMutationOutcome, ProjectPage, ProjectPolicyDocument, ProjectPolicyMutationOutcome, ProjectPolicyStore,
  ProjectStore, PublishAgentPoolVersion, PublishBuildConfigurationVersion, PublishCacheAction, PublishCacheBlob,
  PublishInternalTriggerVersion, PublishPipelineVersion, PublishProjectPolicy, PublishRepositoryVersion,
  PublishedAgentPool, PublishedBuildConfiguration, PublishedPipeline, PublishedRepository, ReadJobEvents,
  ReassignAgentPool, ReassignAgentPoolOutcome, RecordManagedWebhookRegistration, RecordTriggerEvaluationRevision,
  RecoverExpiredLease, RecoverExpiredLeaseOutcome, RegisterAgent, RenameProject, RenewLease, RepositoryMutationOutcome,
  ReserveArtifact, ReserveTriggerEvaluation, RetentionPassOutcome, RetentionPreparation, RetentionWorkClaim,
  RetryBuild, RetryDisposition, RevokeAgentCredential, RevokeCacheSession, ScheduleRecord, ScheduleStore,
  StageOrphanLogChunk, StoreError, SuppressTrigger, SuppressTriggerOutcome, SuppressWebhookDelivery,
  TransitionArtifact, TriggerAcceptanceProbe, TriggerAcceptanceStore, TriggerDefinitionMutationOutcome,
  TriggerDefinitionRef, TriggerDefinitionStore, TriggerEvaluationClaim, TriggerEvaluationOutcome,
  TriggerEvaluationReservation, TriggerEvaluationWorkStore, TriggerKind, TriggerTarget,
  UnmanagedWebhookMutationOutcome, VerifyArtifactUpload, WebhookConfigurationStore, WebhookDeliveryAdmissionStore,
  WebhookDeliveryQueryStore, WebhookDeliveryWorkStore, WebhookIntegrationReader, WebhookIntegrationRecord,
};
use sqlx::PgPool;

/// PostgreSQL adapter for ports that do not cross a JobSpec signing boundary.
#[derive(Clone)]
pub struct PostgresStore {
  pool: PgPool,
}

impl PostgresStore {
  /// Creates infrastructure ports backed by a migrated PostgreSQL pool.
  #[must_use]
  pub const fn new(pool: PgPool) -> Self {
    Self { pool }
  }

  pub(crate) const fn pool(&self) -> &PgPool {
    &self.pool
  }
}

#[async_trait]
impl ArtifactRecordStore for PostgresStore {
  async fn reserve_artifact(&self, request: ReserveArtifact) -> Result<ArtifactRecord, StoreError> {
    crate::artifact::reserve(&self.pool, request).await
  }

  async fn artifact(&self, artifact_id: octacity_server_domain::ArtifactId) -> Result<ArtifactRecord, StoreError> {
    crate::artifact::read(&self.pool, artifact_id).await
  }

  async fn transition_artifact(&self, request: TransitionArtifact) -> Result<ArtifactRecord, StoreError> {
    crate::artifact::transition(&self.pool, request).await
  }

  async fn begin_artifact_upload(
    &self,
    request: BeginArtifactUpload,
  ) -> Result<BeginArtifactUploadOutcome, StoreError> {
    crate::artifact::begin_upload(&self.pool, request).await
  }

  async fn artifact_upload(
    &self,
    upload_id: octacity_server_domain::ArtifactUploadId,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    crate::artifact::read_upload(&self.pool, upload_id).await
  }

  async fn begin_artifact_verification(
    &self,
    request: VerifyArtifactUpload,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    crate::artifact::begin_verification(&self.pool, request).await
  }

  async fn finish_artifact_verification(
    &self,
    request: VerifyArtifactUpload,
    result: ArtifactVerificationResult,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    crate::artifact::finish_verification(&self.pool, request, result).await
  }

  async fn published_artifact(
    &self,
    artifact_id: octacity_server_domain::ArtifactId,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    crate::artifact::published(&self.pool, artifact_id).await
  }

  async fn list_published_artifacts(
    &self,
    query: ListPublishedArtifacts,
  ) -> Result<Vec<ArtifactUploadRecord>, StoreError> {
    crate::artifact::list_published(&self.pool, query).await
  }
}

#[async_trait]
impl CacheSessionStore for PostgresStore {
  async fn begin_cache_session(&self, request: BeginCacheSession) -> Result<BeginCacheSessionOutcome, StoreError> {
    crate::cache::begin(&self.pool, request).await
  }

  async fn revoke_cache_session(&self, request: RevokeCacheSession) -> Result<MutationDisposition, StoreError> {
    crate::cache::revoke(&self.pool, request).await
  }

  async fn authorize_cache_session(
    &self,
    request: AuthorizeCacheSession,
  ) -> Result<CacheAuthorizationOutcome, StoreError> {
    crate::cache::authorize(&self.pool, request).await
  }

  async fn cache_session(
    &self,
    session_id: octacity_server_domain::CacheSessionId,
  ) -> Result<CacheSessionRecord, StoreError> {
    crate::cache::read(&self.pool, session_id).await
  }

  async fn list_build_cache_sessions(
    &self,
    request: ListBuildCacheSessions,
  ) -> Result<Vec<CacheSessionRecord>, StoreError> {
    crate::cache::list(&self.pool, request).await
  }
}

#[async_trait]
impl CacheDataStore for PostgresStore {
  async fn resolve_cache_namespace(
    &self,
    credential_digest: octacity_server_cache::CacheCredentialDigest,
    observed_at: octacity_server_domain::Timestamp,
  ) -> Result<octacity_server_cache::CacheNamespace, StoreError> {
    crate::cache::resolve_namespace(&self.pool, credential_digest, observed_at).await
  }

  async fn find_missing_cache_blobs(
    &self,
    access: CacheDataAccess,
    blobs: Vec<octacity_server_cache::BlobDescriptor>,
  ) -> Result<Vec<octacity_server_cache::BlobDescriptor>, StoreError> {
    crate::cache::find_missing_blobs(&self.pool, access, blobs).await
  }

  async fn cache_blob(
    &self,
    access: CacheDataAccess,
    blob: octacity_server_cache::BlobDescriptor,
  ) -> Result<Option<octacity_server_cache::CacheBlobObject>, StoreError> {
    crate::cache::read_blob(&self.pool, access, blob).await
  }

  async fn publish_cache_blob(&self, request: PublishCacheBlob) -> Result<CachePublicationOutcome, StoreError> {
    crate::cache::publish_blob(&self.pool, request).await
  }

  async fn prepare_cache_blob(
    &self,
    access: CacheDataAccess,
    blob: octacity_server_cache::BlobDescriptor,
  ) -> Result<CacheBlobPreparationOutcome, StoreError> {
    crate::cache::prepare_blob(&self.pool, access, blob).await
  }

  async fn cache_action(
    &self,
    access: CacheDataAccess,
    action: octacity_server_cache::Digest,
  ) -> Result<Option<octacity_server_cache::ActionResultV1>, StoreError> {
    crate::cache::read_action(&self.pool, access, action).await
  }

  async fn publish_cache_action(&self, request: PublishCacheAction) -> Result<CachePublicationOutcome, StoreError> {
    crate::cache::publish_action(&self.pool, request).await
  }

  async fn prune_cache(&self, access: CacheDataAccess) -> Result<CacheRetentionOutcome, StoreError> {
    crate::cache::prune(&self.pool, access).await
  }
}

#[async_trait]
impl BuildRetentionStore for PostgresStore {
  async fn claim_retention_work(&self, request: ClaimRetentionWork) -> Result<Vec<RetentionWorkClaim>, StoreError> {
    crate::retention::claim(&self.pool, request).await
  }

  async fn prepare_retention_work(&self, request: PrepareRetentionWork) -> Result<RetentionPreparation, StoreError> {
    crate::retention::prepare(&self.pool, request).await
  }

  async fn complete_retention_search(&self, request: CompleteRetentionSearch) -> Result<(), StoreError> {
    crate::retention::complete_search(&self.pool, request).await
  }

  async fn complete_retention_object(&self, request: CompleteRetentionObject) -> Result<(), StoreError> {
    crate::retention::complete_object(&self.pool, request).await
  }

  async fn finish_retention_pass(&self, request: FinishRetentionPass) -> Result<RetentionPassOutcome, StoreError> {
    crate::retention::finish_pass(&self.pool, request).await
  }

  async fn fail_retention_work(&self, request: FailRetentionWork) -> Result<(), StoreError> {
    crate::retention::fail(&self.pool, request).await
  }
}

#[async_trait]
impl OrphanLogChunkStore for PostgresStore {
  async fn stage_orphan_log_chunk(&self, request: StageOrphanLogChunk) -> Result<(), StoreError> {
    crate::retention::stage_orphan(&self.pool, request).await
  }

  async fn claim_orphan_log_chunks(
    &self,
    request: ClaimOrphanLogChunks,
  ) -> Result<Vec<OrphanLogChunkClaim>, StoreError> {
    crate::retention::claim_orphans(&self.pool, request).await
  }

  async fn complete_orphan_log_chunk(&self, request: CompleteOrphanLogChunk) -> Result<(), StoreError> {
    crate::retention::complete_orphan(&self.pool, request).await
  }

  async fn fail_orphan_log_chunk(&self, request: FailOrphanLogChunk) -> Result<(), StoreError> {
    crate::retention::fail_orphan(&self.pool, request).await
  }
}

/// PostgreSQL adapter for mutations that cross a signed JobSpec boundary.
#[derive(Clone)]
pub struct PostgresAuthoritativeStore {
  store: PostgresStore,
  job_spec_signer: Arc<JobSpecSigner>,
}

impl PostgresAuthoritativeStore {
  /// Creates an authoritative adapter with an explicit active signing key.
  #[must_use]
  pub fn new(pool: PgPool, job_spec_signer: Arc<JobSpecSigner>) -> Self {
    Self {
      store: PostgresStore::new(pool),
      job_spec_signer,
    }
  }
}

#[async_trait]
impl TriggerAcceptanceStore for PostgresAuthoritativeStore {
  async fn replay_trigger_acceptance(
    &self,
    request: TriggerAcceptanceProbe,
  ) -> Result<Option<TriggerEvaluationOutcome>, StoreError> {
    crate::accept_trigger::replay_evaluation(&self.store.pool, request).await
  }

  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
    crate::accept_trigger::execute(&self.store.pool, &self.job_spec_signer, request).await
  }

  async fn suppress_trigger(&self, request: SuppressTrigger) -> Result<SuppressTriggerOutcome, StoreError> {
    crate::accept_trigger::suppress(&self.store.pool, request).await
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
    crate::job_claim::execute(&self.store.pool, request).await
  }

  async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
    crate::job_events::execute(&self.store.pool, request).await
  }

  async fn prepare_job_event_append(
    &self,
    lease: octacity_server_store::LeaseAccess,
    accepted_at: octacity_server_domain::Timestamp,
  ) -> Result<octacity_server_store::JobEventAppendPreparation, StoreError> {
    crate::job_events::prepare(&self.store.pool, lease, accepted_at).await
  }

  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    crate::job_completion::execute(&self.store.pool, &self.job_spec_signer, request).await
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
    crate::lease_heartbeat::execute(&self.pool, request).await
  }
}

#[async_trait]
impl InternalTriggerEventStore for PostgresStore {
  async fn claim_internal_trigger_events(
    &self,
    request: ClaimInternalTriggerEvents,
  ) -> Result<Vec<InternalTriggerEventClaim>, StoreError> {
    crate::internal_trigger::claim(&self.pool, request).await
  }

  async fn complete_internal_trigger_event(
    &self,
    request: CompleteInternalTriggerEvent,
  ) -> Result<MutationDisposition, StoreError> {
    crate::internal_trigger::complete(&self.pool, request).await
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
    crate::lease_heartbeat::execute(&self.store.pool, request).await
  }
}

#[async_trait]
impl LeaseRecoveryStore for PostgresAuthoritativeStore {
  async fn claim_expired_leases(&self, request: ClaimExpiredLeases) -> Result<Vec<ExpiredLeaseClaim>, StoreError> {
    crate::lease_recovery::claim(&self.store.pool, request).await
  }

  async fn recover_expired_lease(
    &self,
    request: RecoverExpiredLease,
  ) -> Result<RecoverExpiredLeaseOutcome, StoreError> {
    crate::lease_recovery::recover(&self.store.pool, &self.job_spec_signer, request).await
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

#[async_trait]
impl TriggerDefinitionStore for PostgresStore {
  async fn require_enabled_trigger(
    &self,
    trigger: TriggerDefinitionRef,
    kind: TriggerKind,
    target: TriggerTarget,
  ) -> Result<(), StoreError> {
    crate::trigger_query::require_enabled(&self.pool, trigger, kind, target).await
  }
}

#[async_trait]
impl WebhookConfigurationStore for PostgresStore {
  async fn create_unmanaged_webhook(
    &self,
    request: CreateUnmanagedWebhook,
  ) -> Result<UnmanagedWebhookMutationOutcome, StoreError> {
    crate::external_trigger::create(&self.pool, request).await
  }

  async fn create_managed_webhook(
    &self,
    request: CreateManagedWebhook,
  ) -> Result<ManagedWebhookMutationOutcome, StoreError> {
    crate::external_trigger::create_managed(&self.pool, request).await
  }
}

#[async_trait]
impl ManagedWebhookRegistrationStore for PostgresStore {
  async fn managed_webhook(
    &self,
    integration_id: octacity_server_domain::IntegrationId,
  ) -> Result<ManagedWebhookRecord, StoreError> {
    crate::external_trigger::read_managed(&self.pool, integration_id).await
  }
}

#[async_trait]
impl ManagedWebhookOperationStore for PostgresStore {
  async fn enqueue_managed_webhook_operation(
    &self,
    request: octacity_server_store::EnqueueManagedWebhookOperation,
  ) -> Result<MutationDisposition, StoreError> {
    crate::external_trigger::enqueue_managed_operation(&self.pool, request).await
  }

  async fn claim_managed_webhook_operations(
    &self,
    request: octacity_server_store::ClaimManagedWebhookOperations,
  ) -> Result<Vec<octacity_server_store::ManagedWebhookOperationClaim>, StoreError> {
    crate::external_trigger::claim_managed_operations(&self.pool, request).await
  }

  async fn record_managed_webhook_registration(
    &self,
    request: RecordManagedWebhookRegistration,
  ) -> Result<ManagedWebhookMutationOutcome, StoreError> {
    crate::external_trigger::record_managed(&self.pool, request).await
  }

  async fn fail_managed_webhook_operation(
    &self,
    request: octacity_server_store::FailManagedWebhookOperation,
  ) -> Result<(), StoreError> {
    crate::external_trigger::fail_managed_operation(&self.pool, request).await
  }
}

#[async_trait]
impl WebhookIntegrationReader for PostgresStore {
  async fn webhook_integration(
    &self,
    integration_id: octacity_server_domain::IntegrationId,
  ) -> Result<WebhookIntegrationRecord, StoreError> {
    crate::external_trigger::read(&self.pool, integration_id).await
  }
}

#[async_trait]
impl WebhookDeliveryAdmissionStore for PostgresStore {
  async fn enqueue_webhook_delivery(
    &self,
    request: octacity_server_store::EnqueueWebhookDelivery,
  ) -> Result<octacity_server_store::MutationDisposition, StoreError> {
    crate::external_trigger::enqueue_delivery(&self.pool, request).await
  }
}

#[async_trait]
impl WebhookDeliveryWorkStore for PostgresStore {
  async fn claim_webhook_deliveries(
    &self,
    request: octacity_server_store::ClaimWebhookDeliveries,
  ) -> Result<Vec<octacity_server_store::WebhookDeliveryClaim>, StoreError> {
    crate::external_trigger::claim_deliveries(&self.pool, request).await
  }

  async fn record_webhook_event(
    &self,
    request: octacity_server_store::RecordWebhookEvent,
  ) -> Result<octacity_server_store::RecordWebhookEventOutcome, StoreError> {
    crate::external_trigger::record_event(&self.pool, request).await
  }

  async fn fail_webhook_delivery(&self, request: octacity_server_store::FailWebhookDelivery) -> Result<(), StoreError> {
    crate::external_trigger::fail_delivery(&self.pool, request).await
  }

  async fn complete_webhook_delivery(
    &self,
    request: octacity_server_store::CompleteWebhookDelivery,
  ) -> Result<octacity_server_store::MutationDisposition, StoreError> {
    crate::external_trigger::complete_delivery(&self.pool, request).await
  }

  async fn suppress_webhook_delivery(
    &self,
    request: SuppressWebhookDelivery,
  ) -> Result<MutationDisposition, StoreError> {
    crate::external_trigger::suppress_delivery(&self.pool, request).await
  }
}

#[async_trait]
impl WebhookDeliveryQueryStore for PostgresStore {
  async fn webhook_delivery(
    &self,
    delivery_id: octacity_server_store::WebhookDeliveryId,
  ) -> Result<octacity_server_store::WebhookDeliveryDiagnostic, StoreError> {
    crate::external_trigger::delivery_diagnostic(&self.pool, delivery_id).await
  }
}

#[async_trait]
impl ProjectStore for PostgresStore {
  async fn create_project(&self, request: CreateProject) -> Result<ProjectMutationOutcome, StoreError> {
    crate::project_mutation::create(&self.pool, request).await
  }

  async fn rename_project(&self, request: RenameProject) -> Result<ProjectMutationOutcome, StoreError> {
    crate::project_mutation::rename(&self.pool, request).await
  }

  async fn move_project(&self, request: MoveProject) -> Result<ProjectMutationOutcome, StoreError> {
    crate::project_mutation::move_project(&self.pool, request).await
  }

  async fn project(&self, project_id: ProjectId) -> Result<ProjectDetails, StoreError> {
    crate::project_query::read(&self.pool, project_id).await
  }

  async fn list_projects(&self, request: ListProjects) -> Result<ProjectPage, StoreError> {
    crate::project_query::list(&self.pool, request).await
  }

  async fn delete_project(&self, request: DeleteProject) -> Result<DeleteProjectOutcome, StoreError> {
    crate::project_mutation::delete(&self.pool, request).await
  }
}

#[async_trait]
impl ProjectPolicyStore for PostgresStore {
  async fn project_policy_lineage(&self, project_id: ProjectId) -> Result<Vec<ProjectPolicyDocument>, StoreError> {
    crate::project_policy_query::lineage(&self.pool, project_id).await
  }
}

#[async_trait]
impl DefinitionStore for PostgresStore {
  async fn publish_project_policy(
    &self,
    request: PublishProjectPolicy,
  ) -> Result<ProjectPolicyMutationOutcome, StoreError> {
    crate::definition_mutation::publish_policy(&self.pool, request).await
  }

  async fn create_trigger_definition(
    &self,
    request: CreateTriggerDefinition,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    crate::definition_mutation::create_trigger(&self.pool, request).await
  }
}

#[async_trait]
impl ScheduleStore for PostgresStore {
  async fn create_schedule(&self, request: CreateSchedule) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    crate::schedule::create(&self.pool, request).await
  }

  async fn schedule(
    &self,
    trigger_id: octacity_server_domain::TriggerId,
    version: octacity_server_domain::TriggerVersion,
  ) -> Result<ScheduleRecord, StoreError> {
    crate::schedule::read(&self.pool, trigger_id, version).await
  }

  async fn claim_due_schedules(&self, request: ClaimDueSchedules) -> Result<Vec<DueScheduleClaim>, StoreError> {
    crate::schedule::claim(&self.pool, request).await
  }

  async fn complete_schedule_claim(&self, request: CompleteScheduleClaim) -> Result<MutationDisposition, StoreError> {
    crate::schedule::complete(&self.pool, request).await
  }
}

#[async_trait]
impl AgentPoolStore for PostgresStore {
  async fn create_agent_pool(&self, request: CreateAgentPool) -> Result<AgentPoolMutationOutcome, StoreError> {
    crate::pool_mutation::create(&self.pool, request).await
  }

  async fn publish_agent_pool_version(
    &self,
    request: PublishAgentPoolVersion,
  ) -> Result<AgentPoolMutationOutcome, StoreError> {
    crate::pool_mutation::publish(&self.pool, request).await
  }

  async fn agent_pool_version(&self, pool_id: PoolId, version: PoolVersion) -> Result<PublishedAgentPool, StoreError> {
    crate::pool_query::read(&self.pool, pool_id, version).await
  }

  async fn list_agent_pools(&self, request: ListAgentPools) -> Result<AgentPoolPage, StoreError> {
    crate::pool_query::list(&self.pool, request).await
  }

  async fn delete_agent_pool(&self, request: DeleteAgentPool) -> Result<DeleteAgentPoolOutcome, StoreError> {
    crate::pool_mutation::delete(&self.pool, request).await
  }
}

#[async_trait]
impl AgentStore for PostgresStore {
  async fn agent(
    &self,
    agent_id: octacity_server_domain::AgentId,
  ) -> Result<octacity_server_store::EnrolledAgent, StoreError> {
    crate::agent_query::read(&self.pool, agent_id).await
  }

  async fn list_agents(&self, request: ListAgents) -> Result<AgentPage, StoreError> {
    crate::agent_query::list(&self.pool, request).await
  }

  async fn reassign_agent_pool(&self, request: ReassignAgentPool) -> Result<ReassignAgentPoolOutcome, StoreError> {
    crate::agent_mutation::reassign(&self.pool, request).await
  }

  async fn drain_agent(&self, request: DrainAgent) -> Result<DrainAgentOutcome, StoreError> {
    crate::agent_mutation::drain(&self.pool, request).await
  }
}

#[async_trait]
impl PipelineStore for PostgresStore {
  async fn create_pipeline(
    &self,
    request: octacity_server_store::CreatePipeline,
  ) -> Result<PipelineMutationOutcome, StoreError> {
    crate::pipeline_mutation::create(&self.pool, request).await
  }

  async fn publish_pipeline_version(
    &self,
    request: PublishPipelineVersion,
  ) -> Result<PipelineMutationOutcome, StoreError> {
    crate::pipeline_mutation::publish(&self.pool, request).await
  }

  async fn pipeline_version(
    &self,
    pipeline_id: PipelineId,
    version: PipelineVersion,
  ) -> Result<PublishedPipeline, StoreError> {
    crate::pipeline_query::read(&self.pool, pipeline_id, version).await
  }
}

#[async_trait]
impl ConfigurationStore for PostgresStore {
  async fn create_repository(&self, request: CreateRepository) -> Result<RepositoryMutationOutcome, StoreError> {
    crate::repository_mutation::create(&self.pool, request).await
  }

  async fn publish_repository_version(
    &self,
    request: PublishRepositoryVersion,
  ) -> Result<RepositoryMutationOutcome, StoreError> {
    crate::repository_mutation::publish(&self.pool, request).await
  }

  async fn repository_version(
    &self,
    repository_id: RepositoryId,
    version: RepositoryVersion,
  ) -> Result<PublishedRepository, StoreError> {
    crate::configuration_query::repository(&self.pool, repository_id, version).await
  }

  async fn create_build_configuration(
    &self,
    request: CreateBuildConfiguration,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError> {
    crate::build_configuration_mutation::create(&self.pool, request).await
  }

  async fn publish_build_configuration_version(
    &self,
    request: PublishBuildConfigurationVersion,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError> {
    crate::build_configuration_mutation::publish(&self.pool, request).await
  }

  async fn build_configuration_version(
    &self,
    configuration_id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<PublishedBuildConfiguration, StoreError> {
    crate::configuration_query::build_configuration(&self.pool, configuration_id, version).await
  }
}

#[async_trait]
impl AgentCredentialStore for PostgresStore {
  async fn issue_agent_enrollment(
    &self,
    request: IssueAgentEnrollment,
  ) -> Result<IssueAgentEnrollmentOutcome, StoreError> {
    crate::agent_enrollment::execute(&self.pool, request).await
  }

  async fn register_agent(&self, request: RegisterAgent) -> Result<AgentRegistrationOutcome, StoreError> {
    crate::agent_registration::execute(&self.pool, request).await
  }

  async fn authenticate_agent_registration(
    &self,
    request: AuthenticateAgentRegistration,
  ) -> Result<AuthenticatedAgentRegistration, StoreError> {
    crate::agent_credential_auth::execute(&self.pool, request).await
  }

  async fn revoke_agent_credential(&self, request: RevokeAgentCredential) -> Result<MutationDisposition, StoreError> {
    crate::agent_credential_revocation::execute(&self.pool, request).await
  }
}
