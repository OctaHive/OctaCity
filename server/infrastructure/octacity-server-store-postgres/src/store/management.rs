use async_trait::async_trait;
use octacity_server_domain::*;
use octacity_server_store::*;

use super::PostgresStore;

#[async_trait]
impl ProjectStore for PostgresStore {
  async fn create_project(
    &self,
    request: ManagementMutation<CreateProject>,
  ) -> Result<ProjectMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::projects::project_mutation::create(&self.pool, request, &audit).await
  }

  async fn rename_project(
    &self,
    request: ManagementMutation<RenameProject>,
  ) -> Result<ProjectMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::projects::project_mutation::rename(&self.pool, request, &audit).await
  }

  async fn move_project(&self, request: ManagementMutation<MoveProject>) -> Result<ProjectMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::projects::project_mutation::move_project(&self.pool, request, &audit).await
  }

  async fn project(&self, project_id: ProjectId) -> Result<ProjectDetails, StoreError> {
    crate::projects::project_query::read(&self.pool, project_id).await
  }

  async fn list_projects(&self, request: ListProjects) -> Result<ProjectPage, StoreError> {
    crate::projects::project_query::list(&self.pool, request).await
  }

  async fn delete_project(
    &self,
    request: ManagementMutation<DeleteProject>,
  ) -> Result<DeleteProjectOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::projects::project_mutation::delete(&self.pool, request, &audit).await
  }
}

#[async_trait]
impl ResourceSearchStore for PostgresStore {
  async fn search_resources(&self, request: SearchResources) -> Result<ResourceSearchPage, StoreError> {
    crate::operations::resource_search::search(&self.pool, request).await
  }
}

#[async_trait]
impl OperatorAttentionStore for PostgresStore {
  async fn list_operator_attention(&self, request: ListOperatorAttention) -> Result<OperatorAttentionPage, StoreError> {
    crate::operations::operator_attention::list(&self.pool, request).await
  }
}

#[async_trait]
impl CriticalSystemConditionStore for PostgresStore {
  async fn record_critical_system_condition(&self, change: CriticalSystemConditionChange) -> Result<(), StoreError> {
    crate::operations::operator_attention::record_critical_system_condition(&self.pool, change).await
  }
}

#[async_trait]
impl ProjectPolicyStore for PostgresStore {
  async fn project_policy_lineage(&self, project_id: ProjectId) -> Result<Vec<ProjectPolicyDocument>, StoreError> {
    crate::projects::project_policy_query::lineage(&self.pool, project_id).await
  }
}

#[async_trait]
impl DefinitionStore for PostgresStore {
  async fn publish_project_policy(
    &self,
    request: ManagementMutation<PublishProjectPolicy>,
  ) -> Result<ProjectPolicyMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::definitions::definition_mutation::publish_policy(&self.pool, request, &audit).await
  }

  async fn create_trigger_definition(
    &self,
    request: ManagementMutation<CreateTriggerDefinition>,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::definitions::definition_mutation::create_trigger(&self.pool, request, &audit).await
  }

  async fn manual_trigger_definition(
    &self,
    trigger_id: TriggerId,
    version: TriggerVersion,
  ) -> Result<ManualTriggerDefinitionRecord, StoreError> {
    crate::cicd::definitions::definition_query::read_manual(&self.pool, trigger_id, version).await
  }
}

#[async_trait]
impl ScheduleStore for PostgresStore {
  async fn create_schedule(
    &self,
    request: ManagementMutation<CreateSchedule>,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::triggers::schedule::create(&self.pool, request, &audit).await
  }

  async fn schedule(
    &self,
    trigger_id: octacity_server_domain::TriggerId,
    version: octacity_server_domain::TriggerVersion,
  ) -> Result<ScheduleRecord, StoreError> {
    crate::cicd::triggers::schedule::read(&self.pool, trigger_id, version).await
  }

  async fn claim_due_schedules(&self, request: ClaimDueSchedules) -> Result<Vec<DueScheduleClaim>, StoreError> {
    crate::cicd::triggers::schedule::claim(&self.pool, request).await
  }

  async fn complete_schedule_claim(&self, request: CompleteScheduleClaim) -> Result<MutationDisposition, StoreError> {
    crate::cicd::triggers::schedule::complete(&self.pool, request).await
  }
}

#[async_trait]
impl AgentPoolStore for PostgresStore {
  async fn create_agent_pool(
    &self,
    request: ManagementMutation<CreateAgentPool>,
  ) -> Result<AgentPoolMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::agents::pool_mutation::create(&self.pool, request, &audit).await
  }

  async fn publish_agent_pool_version(
    &self,
    request: ManagementMutation<PublishAgentPoolVersion>,
  ) -> Result<AgentPoolMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::agents::pool_mutation::publish(&self.pool, request, &audit).await
  }

  async fn agent_pool_version(&self, pool_id: PoolId, version: PoolVersion) -> Result<PublishedAgentPool, StoreError> {
    crate::cicd::agents::pool_query::read(&self.pool, pool_id, version).await
  }

  async fn current_agent_pool(&self, pool_id: PoolId) -> Result<PublishedAgentPool, StoreError> {
    crate::cicd::agents::pool_query::read_current(&self.pool, pool_id).await
  }

  async fn list_agent_pools(&self, request: ListAgentPools) -> Result<AgentPoolPage, StoreError> {
    crate::cicd::agents::pool_query::list(&self.pool, request).await
  }

  async fn delete_agent_pool(
    &self,
    request: ManagementMutation<DeleteAgentPool>,
  ) -> Result<DeleteAgentPoolOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::agents::pool_mutation::delete(&self.pool, request, &audit).await
  }
}

#[async_trait]
impl AgentStore for PostgresStore {
  async fn set_agent_enabled(
    &self,
    request: ManagementMutation<octacity_server_store::SetAgentEnabled>,
  ) -> Result<octacity_server_store::SetAgentEnabledOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::agents::agent_mutation::set_enabled(&self.pool, request, &audit).await
  }

  async fn agent_detail(
    &self,
    agent_id: octacity_server_domain::AgentId,
  ) -> Result<octacity_server_store::AgentDetail, StoreError> {
    crate::cicd::agents::agent_query::read_detail(&self.pool, agent_id).await
  }

  async fn list_agents(&self, request: ListAgents) -> Result<AgentPage, StoreError> {
    crate::cicd::agents::agent_query::list(&self.pool, request).await
  }

  async fn reassign_agent_pool(
    &self,
    request: ManagementMutation<ReassignAgentPool>,
  ) -> Result<ReassignAgentPoolOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::agents::agent_mutation::reassign(&self.pool, request, &audit).await
  }

  async fn drain_agent(&self, request: ManagementMutation<DrainAgent>) -> Result<DrainAgentOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::agents::agent_mutation::drain(&self.pool, request, &audit).await
  }
}

#[async_trait]
impl PipelineStore for PostgresStore {
  async fn create_pipeline(
    &self,
    request: ManagementMutation<octacity_server_store::CreatePipeline>,
  ) -> Result<PipelineMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::definitions::pipeline_mutation::create(&self.pool, request, &audit).await
  }

  async fn publish_pipeline_version(
    &self,
    request: ManagementMutation<PublishPipelineVersion>,
  ) -> Result<PipelineMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::definitions::pipeline_mutation::publish(&self.pool, request, &audit).await
  }

  async fn pipeline_version(
    &self,
    pipeline_id: PipelineId,
    version: PipelineVersion,
  ) -> Result<PublishedPipeline, StoreError> {
    crate::cicd::definitions::pipeline_query::read(&self.pool, pipeline_id, version).await
  }
}

#[async_trait]
impl PipelineDiscoveryStore for PostgresStore {
  async fn list_project_pipelines(&self, request: ListProjectPipelines) -> Result<CurrentPipelinePage, StoreError> {
    crate::cicd::definitions::definition_discovery::pipelines(&self.pool, request).await
  }
}

#[async_trait]
impl ConfigurationStore for PostgresStore {
  async fn create_repository(
    &self,
    request: ManagementMutation<CreateRepository>,
  ) -> Result<RepositoryMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::projects::repository_mutation::create(&self.pool, request, &audit).await
  }

  async fn publish_repository_version(
    &self,
    request: ManagementMutation<PublishRepositoryVersion>,
  ) -> Result<RepositoryMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::projects::repository_mutation::publish(&self.pool, request, &audit).await
  }

  async fn repository_version(
    &self,
    repository_id: RepositoryId,
    version: RepositoryVersion,
  ) -> Result<PublishedRepository, StoreError> {
    crate::cicd::definitions::configuration_query::repository(&self.pool, repository_id, version).await
  }

  async fn create_build_configuration(
    &self,
    request: ManagementMutation<CreateBuildConfiguration>,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::definitions::build_configuration_mutation::create(&self.pool, request, &audit).await
  }

  async fn publish_build_configuration_version(
    &self,
    request: ManagementMutation<PublishBuildConfigurationVersion>,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::definitions::build_configuration_mutation::publish(&self.pool, request, &audit).await
  }

  async fn build_configuration_version(
    &self,
    configuration_id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<PublishedBuildConfiguration, StoreError> {
    crate::cicd::definitions::configuration_query::build_configuration(&self.pool, configuration_id, version).await
  }
}

#[async_trait]
impl ConfigurationDiscoveryStore for PostgresStore {
  async fn list_project_repositories(
    &self,
    request: ListProjectRepositories,
  ) -> Result<CurrentRepositoryPage, StoreError> {
    crate::cicd::definitions::definition_discovery::repositories(&self.pool, request).await
  }

  async fn list_project_build_configurations(
    &self,
    request: ListProjectBuildConfigurations,
  ) -> Result<CurrentBuildConfigurationPage, StoreError> {
    crate::cicd::definitions::definition_discovery::build_configurations(&self.pool, request).await
  }
}

#[async_trait]
impl TriggerDefinitionDiscoveryStore for PostgresStore {
  async fn list_project_trigger_definitions(
    &self,
    request: ListProjectTriggerDefinitions,
  ) -> Result<CurrentTriggerDefinitionPage, StoreError> {
    crate::cicd::definitions::definition_discovery::triggers(&self.pool, request).await
  }
}

#[async_trait]
impl AgentCredentialStore for PostgresStore {
  async fn issue_agent_enrollment(
    &self,
    request: ManagementMutation<IssueAgentEnrollment>,
  ) -> Result<IssueAgentEnrollmentOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::agents::agent_enrollment::execute(&self.pool, request, &audit).await
  }

  async fn register_agent(&self, request: RegisterAgent) -> Result<AgentRegistrationOutcome, StoreError> {
    crate::cicd::agents::agent_registration::execute(&self.pool, request).await
  }

  async fn authenticate_agent_registration(
    &self,
    request: AuthenticateAgentRegistration,
  ) -> Result<AuthenticatedAgentRegistration, StoreError> {
    crate::cicd::agents::agent_credential_auth::execute(&self.pool, request).await
  }

  async fn revoke_agent_credential(
    &self,
    request: ManagementMutation<RevokeAgentCredential>,
  ) -> Result<MutationDisposition, StoreError> {
    let (request, audit) = request.into_parts();
    crate::cicd::agents::agent_credential_revocation::execute(&self.pool, request, &audit).await
  }
}

#[async_trait]
impl IntegrationConnectionStore for PostgresStore {
  async fn list_integration_connections(
    &self,
    request: ReadIntegrationConnections,
  ) -> Result<IntegrationConnectionPage, StoreError> {
    crate::integrations::integration_connection::list(&self.pool, request).await
  }
  async fn replay_integration_connection(
    &self,
    request: ManagementMutation<ReplayIntegrationConnection>,
  ) -> Result<Option<IntegrationConnectionMutationOutcome>, StoreError> {
    crate::integrations::integration_connection::replay_mutation(&self.pool, request).await
  }
  async fn publish_integration_connection(
    &self,
    request: ManagementMutation<PublishIntegrationConnection>,
  ) -> Result<IntegrationConnectionMutationOutcome, StoreError> {
    crate::integrations::integration_connection::publish(&self.pool, request).await
  }
  async fn integration_connection(
    &self,
    id: IntegrationId,
    version: Option<IntegrationVersion>,
  ) -> Result<PublishedIntegrationConnection, StoreError> {
    crate::integrations::integration_connection::read(&self.pool, id, version).await
  }
}

#[async_trait]
impl IntegrationAdministrationStore for PostgresStore {
  async fn begin_integration_administration(
    &self,
    request: ManagementMutation<BeginIntegrationAdministration>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError> {
    crate::integrations::integration_administration::begin(&self.pool, request).await
  }
  async fn complete_integration_administration(
    &self,
    request: ManagementMutation<IntegrationAdministrationReceipt>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError> {
    crate::integrations::integration_administration::complete(&self.pool, request).await
  }
  async fn integration_administration(&self, id: IntegrationId) -> Result<IntegrationAdministrationState, StoreError> {
    crate::integrations::integration_administration::read(&self.pool, id).await
  }
  async fn integration_preparation_cleanup(
    &self,
    request: ManagementMutation<IntegrationPreparationCleanup>,
  ) -> Result<Vec<octacity_server_store::CredentialPublication>, StoreError> {
    crate::integrations::integration_administration::cleanup(&self.pool, request).await
  }
  async fn acknowledge_integration_preparation_cleanup(
    &self,
    request: ManagementMutation<octacity_server_store::CredentialPublication>,
  ) -> Result<(), StoreError> {
    crate::integrations::integration_administration::acknowledge(&self.pool, request).await
  }
}

#[async_trait]
impl IntegrationProjectAccessStore for PostgresStore {
  async fn publish_integration_project_access(
    &self,
    request: ManagementMutation<PublishIntegrationProjectAccess>,
  ) -> Result<IntegrationProjectAccessOutcome, StoreError> {
    crate::integrations::integration_project_access::publish(&self.pool, request).await
  }
  async fn integration_project_access(
    &self,
    connection_id: IntegrationId,
    project_id: octacity_server_domain::ProjectId,
  ) -> Result<Option<PublishedIntegrationProjectAccess>, StoreError> {
    crate::integrations::integration_project_access::read(&self.pool, connection_id, project_id).await
  }
}
