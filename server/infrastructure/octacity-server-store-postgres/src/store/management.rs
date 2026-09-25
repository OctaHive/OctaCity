use async_trait::async_trait;
use octacity_server_domain::*;
use octacity_server_store::*;

use super::PostgresStore;

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
