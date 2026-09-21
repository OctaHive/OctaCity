use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, PipelineId, PipelineVersion, PoolId, PoolVersion, ProjectId,
  RepositoryId, RepositoryVersion,
};
use octacity_server_job::JobSpecSigner;
use octacity_server_store::{
  AcceptTrigger, AcceptTriggerOutcome, AgentCredentialStore, AgentPage, AgentPoolMutationOutcome, AgentPoolPage,
  AgentPoolStore, AgentRegistrationOutcome, AgentStore, AppendJobEvents, AppendJobEventsOutcome,
  AuthenticateAgentRegistration, AuthenticatedAgentRegistration, BuildConfigurationMutationOutcome, BuildControlStore,
  BuildQueryStore, BuildRecord, CancelBuild, CancellationDisposition, ClaimExpiredLeases, CompletionDisposition,
  ConfigurationStore, CreateAgentPool, CreateBuildConfiguration, CreateProject, CreateRepository,
  CreateTriggerDefinition, DefinitionStore, DeleteAgentPool, DeleteAgentPoolOutcome, DeleteProject,
  DeleteProjectOutcome, DrainAgent, DrainAgentOutcome, ExpiredLeaseClaim, IssueAgentEnrollment,
  IssueAgentEnrollmentOutcome, JobClaim, JobClaimOutcome, JobCompletion, JobEventPage, JobEventReadStore,
  JobExecutionStore, LeaseHeartbeatOutcome, LeaseHeartbeatStore, LeaseRecoveryStore, ListAgentPools, ListAgents,
  ListProjects, LogIndexPosition, LogIndexWorkStore, MoveProject, MutationDisposition, PipelineMutationOutcome,
  PipelineStore, ProjectDetails, ProjectMutationOutcome, ProjectPage, ProjectPolicyDocument,
  ProjectPolicyMutationOutcome, ProjectPolicyStore, ProjectStore, PublishAgentPoolVersion,
  PublishBuildConfigurationVersion, PublishPipelineVersion, PublishProjectPolicy, PublishRepositoryVersion,
  PublishedAgentPool, PublishedBuildConfiguration, PublishedPipeline, PublishedRepository, ReadJobEvents,
  ReassignAgentPool, ReassignAgentPoolOutcome, RecoverExpiredLease, RecoverExpiredLeaseOutcome, RegisterAgent,
  RenameProject, RenewLease, RepositoryMutationOutcome, RetryBuild, RetryDisposition, RevokeAgentCredential,
  StoreError, SuppressTrigger, SuppressTriggerOutcome, TriggerAcceptanceProbe, TriggerAcceptanceStore,
  TriggerDefinitionMutationOutcome, TriggerDefinitionRef, TriggerDefinitionStore, TriggerEvaluationOutcome,
  TriggerKind, TriggerTarget,
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
impl JobExecutionStore for PostgresAuthoritativeStore {
  async fn claim_ready_job(&self, request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
    crate::job_claim::execute(&self.store.pool, request).await
  }

  async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
    crate::job_events::execute(&self.store.pool, request).await
  }

  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    crate::job_completion::execute(&self.store.pool, &self.job_spec_signer, request).await
  }
}

#[async_trait]
impl LeaseHeartbeatStore for PostgresStore {
  async fn renew_lease(&self, request: RenewLease) -> Result<LeaseHeartbeatOutcome, StoreError> {
    crate::lease_heartbeat::execute(&self.pool, request).await
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
impl LogIndexWorkStore for PostgresStore {
  async fn committed_log_index_position(&self, project_id: ProjectId) -> Result<Option<LogIndexPosition>, StoreError> {
    let position: Option<i64> =
      sqlx::query_scalar("SELECT committed_through FROM log_index_project_positions WHERE project_id = $1")
        .bind(project_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(crate::database::unavailable)?;
    position
      .map(|position| {
        u64::try_from(position)
          .ok()
          .and_then(|position| LogIndexPosition::new(position).ok())
          .ok_or(StoreError::Unavailable)
      })
      .transpose()
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
