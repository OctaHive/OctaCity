use async_trait::async_trait;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, PipelineId, PipelineVersion, ProjectId, RepositoryId,
  RepositoryVersion,
};
use octacity_server_store::{
  AcceptTrigger, AcceptTriggerOutcome, AgentCredentialStore, AgentRegistrationOutcome, AppendJobEvents,
  AppendJobEventsOutcome, AuthenticateAgentRegistration, AuthenticatedAgentRegistration, AuthoritativeStore,
  BuildConfigurationMutationOutcome, CompletionDisposition, ConfigurationStore, CreateBuildConfiguration,
  CreateProject, CreateRepository, DeleteProject, DeleteProjectOutcome, IssueAgentEnrollment,
  IssueAgentEnrollmentOutcome, JobClaim, JobClaimOutcome, JobCompletion, ListProjects, LogIndexPosition,
  LogIndexWorkStore, MoveProject, MutationDisposition, PipelineMutationOutcome, PipelineStore, ProjectDetails,
  ProjectMutationOutcome, ProjectPage, ProjectStore, PublishBuildConfigurationVersion, PublishPipelineVersion,
  PublishRepositoryVersion, PublishedBuildConfiguration, PublishedPipeline, PublishedRepository, RegisterAgent,
  RenameProject, RepositoryMutationOutcome, RevokeAgentCredential, StoreError,
};
use sqlx::PgPool;

/// PostgreSQL adapter for the backend-neutral authoritative store interface.
#[derive(Clone)]
pub struct PostgresStore {
  pool: PgPool,
}

impl PostgresStore {
  /// Creates an adapter backed by a migrated PostgreSQL pool.
  #[must_use]
  pub fn new(pool: PgPool) -> Self {
    Self { pool }
  }
}

#[async_trait]
impl AuthoritativeStore for PostgresStore {
  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
    crate::accept_trigger::execute(&self.pool, request).await
  }

  async fn claim_ready_job(&self, request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
    crate::job_claim::execute(&self.pool, request).await
  }

  async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
    crate::job_events::execute(&self.pool, request).await
  }

  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    crate::job_completion::execute(&self.pool, request).await
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
