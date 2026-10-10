use async_trait::async_trait;
use octacity_server_domain::*;
use octacity_server_store::*;

use super::PostgresStore;

#[async_trait]
impl FactoryConfigurationStore for PostgresStore {
  async fn replay_factory_configuration_mutation(
    &self,
    request: ManagementMutation<ReplayFactoryConfigurationMutation>,
  ) -> Result<Option<FactoryConfigurationMutationOutcome>, StoreError> {
    crate::factory_configuration::replay_mutation(&self.pool, request).await
  }

  async fn factory_configuration_availability(
    &self,
    project_id: ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    crate::discovery::require_project(&self.pool, project_id).await?;
    self.factory_capabilities().availability(project_id).await
  }

  async fn create_factory_configuration(
    &self,
    request: ManagementMutation<CreateFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, StoreError> {
    crate::factory_configuration::create(&self.pool, request).await
  }

  async fn replace_factory_configuration(
    &self,
    request: ManagementMutation<ReplaceFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, ReplaceFactoryConfigurationError> {
    crate::factory_configuration::replace(&self.pool, request).await
  }

  async fn factory_configuration_version(
    &self,
    id: octacity_server_factory::FactoryConfigurationId,
    version: octacity_server_factory::FactoryConfigurationVersion,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    crate::factory_configuration::read(&self.pool, id, version).await
  }

  async fn current_factory_configuration(
    &self,
    id: octacity_server_factory::FactoryConfigurationId,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    crate::factory_configuration::read_current(&self.pool, id).await
  }
}

#[async_trait]
impl FactoryAdmissionStore for PostgresStore {
  async fn replay_factory_admission(
    &self,
    probe: &FactoryAdmissionProbe,
  ) -> Result<Option<FactoryAdmissionMutationOutcome>, StoreError> {
    crate::factory_admission::replay(&self.pool, probe).await
  }

  async fn factory_admission_context(
    &self,
    request: ReadFactoryAdmissionContext,
  ) -> Result<FactoryAdmissionContext, StoreError> {
    crate::factory_admission::context(&self.pool, request).await
  }

  async fn admit_factory_work(
    &self,
    request: ManagementMutation<AdmitFactoryWork>,
  ) -> Result<FactoryAdmissionMutationOutcome, StoreError> {
    crate::factory_admission::admit(&self.pool, request).await
  }
}

#[async_trait]
impl FactoryDiscoveryStore for PostgresStore {
  async fn list_project_factory_configurations(
    &self,
    request: ListProjectFactoryConfigurations,
  ) -> Result<CurrentFactoryConfigurationPage, StoreError> {
    crate::factory_discovery::list_configurations(&self.pool, request).await
  }

  async fn list_factory_runs(&self, request: ListFactoryRuns) -> Result<FactoryRunPage, StoreError> {
    crate::factory_discovery::list_runs(&self.pool, request).await
  }

  async fn factory_run_summary(
    &self,
    run_id: octacity_server_factory::FactoryRunId,
    visibility: FactoryRunListVisibility,
  ) -> Result<FactoryRunSummary, StoreError> {
    crate::factory_discovery::read_run(&self.pool, run_id, visibility).await
  }
}

#[async_trait]
impl FactoryRunStore for PostgresStore {
  async fn factory_run_snapshot(
    &self,
    run_id: octacity_server_factory::FactoryRunId,
  ) -> Result<FactoryRunSnapshot, StoreError> {
    crate::factory_run::read_snapshot(&self.pool, run_id).await
  }

  async fn list_factory_run_diagnostics(
    &self,
    request: octacity_server_store::ListFactoryRunDiagnostics,
  ) -> Result<octacity_server_store::FactoryRunDiagnosticPage, StoreError> {
    crate::factory_run::list_diagnostics(&self.pool, request).await
  }

  async fn claim_factory_run(&self, request: ClaimFactoryRun) -> Result<ClaimFactoryRunOutcome, StoreError> {
    crate::factory_run::claim(&self.pool, request).await
  }

  async fn claim_factory_runs(&self, request: ClaimFactoryRuns) -> Result<Vec<ClaimedFactoryRun>, StoreError> {
    crate::factory_run::claim_batch(&self.pool, request).await
  }

  async fn commit_factory_run_transition(
    &self,
    request: CommitFactoryRunTransition,
  ) -> Result<CommitFactoryRunTransitionOutcome, StoreError> {
    crate::factory_run::commit_transition(&self.pool, request).await
  }

  async fn claim_factory_outbox(&self, request: ClaimFactoryOutbox) -> Result<Vec<ClaimedFactoryOutbox>, StoreError> {
    crate::factory_run::claim_outbox(&self.pool, request).await
  }

  async fn settle_factory_outbox(&self, request: SettleFactoryOutbox) -> Result<FactoryOutboxRecord, StoreError> {
    crate::factory_run::settle_outbox(&self.pool, request).await
  }
}

#[async_trait]
impl ProjectStore for PostgresStore {
  async fn create_project(
    &self,
    request: ManagementMutation<CreateProject>,
  ) -> Result<ProjectMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::project_mutation::create(&self.pool, request, &audit).await
  }

  async fn rename_project(
    &self,
    request: ManagementMutation<RenameProject>,
  ) -> Result<ProjectMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::project_mutation::rename(&self.pool, request, &audit).await
  }

  async fn move_project(&self, request: ManagementMutation<MoveProject>) -> Result<ProjectMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::project_mutation::move_project(&self.pool, request, &audit).await
  }

  async fn project(&self, project_id: ProjectId) -> Result<ProjectDetails, StoreError> {
    crate::project_query::read(&self.pool, project_id).await
  }

  async fn list_projects(&self, request: ListProjects) -> Result<ProjectPage, StoreError> {
    crate::project_query::list(&self.pool, request).await
  }

  async fn delete_project(
    &self,
    request: ManagementMutation<DeleteProject>,
  ) -> Result<DeleteProjectOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::project_mutation::delete(&self.pool, request, &audit).await
  }
}

#[async_trait]
impl ResourceSearchStore for PostgresStore {
  async fn search_resources(&self, request: SearchResources) -> Result<ResourceSearchPage, StoreError> {
    crate::resource_search::search(&self.pool, request).await
  }
}

#[async_trait]
impl OperatorAttentionStore for PostgresStore {
  async fn list_operator_attention(&self, request: ListOperatorAttention) -> Result<OperatorAttentionPage, StoreError> {
    crate::operator_attention::list(&self.pool, request).await
  }
}

#[async_trait]
impl CriticalSystemConditionStore for PostgresStore {
  async fn record_critical_system_condition(&self, change: CriticalSystemConditionChange) -> Result<(), StoreError> {
    crate::operator_attention::record_critical_system_condition(&self.pool, change).await
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
    request: ManagementMutation<PublishProjectPolicy>,
  ) -> Result<ProjectPolicyMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::definition_mutation::publish_policy(&self.pool, request, &audit).await
  }

  async fn create_trigger_definition(
    &self,
    request: ManagementMutation<CreateTriggerDefinition>,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::definition_mutation::create_trigger(&self.pool, request, &audit).await
  }

  async fn manual_trigger_definition(
    &self,
    trigger_id: TriggerId,
    version: TriggerVersion,
  ) -> Result<ManualTriggerDefinitionRecord, StoreError> {
    crate::definition_query::read_manual(&self.pool, trigger_id, version).await
  }
}

#[async_trait]
impl ScheduleStore for PostgresStore {
  async fn create_schedule(
    &self,
    request: ManagementMutation<CreateSchedule>,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::schedule::create(&self.pool, request, &audit).await
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
  async fn create_agent_pool(
    &self,
    request: ManagementMutation<CreateAgentPool>,
  ) -> Result<AgentPoolMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::pool_mutation::create(&self.pool, request, &audit).await
  }

  async fn publish_agent_pool_version(
    &self,
    request: ManagementMutation<PublishAgentPoolVersion>,
  ) -> Result<AgentPoolMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::pool_mutation::publish(&self.pool, request, &audit).await
  }

  async fn agent_pool_version(&self, pool_id: PoolId, version: PoolVersion) -> Result<PublishedAgentPool, StoreError> {
    crate::pool_query::read(&self.pool, pool_id, version).await
  }

  async fn current_agent_pool(&self, pool_id: PoolId) -> Result<PublishedAgentPool, StoreError> {
    crate::pool_query::read_current(&self.pool, pool_id).await
  }

  async fn list_agent_pools(&self, request: ListAgentPools) -> Result<AgentPoolPage, StoreError> {
    crate::pool_query::list(&self.pool, request).await
  }

  async fn delete_agent_pool(
    &self,
    request: ManagementMutation<DeleteAgentPool>,
  ) -> Result<DeleteAgentPoolOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::pool_mutation::delete(&self.pool, request, &audit).await
  }
}

#[async_trait]
impl AgentStore for PostgresStore {
  async fn agent_detail(
    &self,
    agent_id: octacity_server_domain::AgentId,
  ) -> Result<octacity_server_store::AgentDetail, StoreError> {
    crate::agent_query::read_detail(&self.pool, agent_id).await
  }

  async fn list_agents(&self, request: ListAgents) -> Result<AgentPage, StoreError> {
    crate::agent_query::list(&self.pool, request).await
  }

  async fn reassign_agent_pool(
    &self,
    request: ManagementMutation<ReassignAgentPool>,
  ) -> Result<ReassignAgentPoolOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::agent_mutation::reassign(&self.pool, request, &audit).await
  }

  async fn drain_agent(&self, request: ManagementMutation<DrainAgent>) -> Result<DrainAgentOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::agent_mutation::drain(&self.pool, request, &audit).await
  }
}

#[async_trait]
impl PipelineStore for PostgresStore {
  async fn create_pipeline(
    &self,
    request: ManagementMutation<octacity_server_store::CreatePipeline>,
  ) -> Result<PipelineMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::pipeline_mutation::create(&self.pool, request, &audit).await
  }

  async fn publish_pipeline_version(
    &self,
    request: ManagementMutation<PublishPipelineVersion>,
  ) -> Result<PipelineMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::pipeline_mutation::publish(&self.pool, request, &audit).await
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
impl PipelineDiscoveryStore for PostgresStore {
  async fn list_project_pipelines(&self, request: ListProjectPipelines) -> Result<CurrentPipelinePage, StoreError> {
    crate::definition_discovery::pipelines(&self.pool, request).await
  }
}

#[async_trait]
impl ConfigurationStore for PostgresStore {
  async fn create_repository(
    &self,
    request: ManagementMutation<CreateRepository>,
  ) -> Result<RepositoryMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::repository_mutation::create(&self.pool, request, &audit).await
  }

  async fn publish_repository_version(
    &self,
    request: ManagementMutation<PublishRepositoryVersion>,
  ) -> Result<RepositoryMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::repository_mutation::publish(&self.pool, request, &audit).await
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
    request: ManagementMutation<CreateBuildConfiguration>,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::build_configuration_mutation::create(&self.pool, request, &audit).await
  }

  async fn publish_build_configuration_version(
    &self,
    request: ManagementMutation<PublishBuildConfigurationVersion>,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::build_configuration_mutation::publish(&self.pool, request, &audit).await
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
impl ConfigurationDiscoveryStore for PostgresStore {
  async fn list_project_repositories(
    &self,
    request: ListProjectRepositories,
  ) -> Result<CurrentRepositoryPage, StoreError> {
    crate::definition_discovery::repositories(&self.pool, request).await
  }

  async fn list_project_build_configurations(
    &self,
    request: ListProjectBuildConfigurations,
  ) -> Result<CurrentBuildConfigurationPage, StoreError> {
    crate::definition_discovery::build_configurations(&self.pool, request).await
  }
}

#[async_trait]
impl TriggerDefinitionDiscoveryStore for PostgresStore {
  async fn list_project_trigger_definitions(
    &self,
    request: ListProjectTriggerDefinitions,
  ) -> Result<CurrentTriggerDefinitionPage, StoreError> {
    crate::definition_discovery::triggers(&self.pool, request).await
  }
}

#[async_trait]
impl AgentCredentialStore for PostgresStore {
  async fn issue_agent_enrollment(
    &self,
    request: ManagementMutation<IssueAgentEnrollment>,
  ) -> Result<IssueAgentEnrollmentOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    crate::agent_enrollment::execute(&self.pool, request, &audit).await
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

  async fn revoke_agent_credential(
    &self,
    request: ManagementMutation<RevokeAgentCredential>,
  ) -> Result<MutationDisposition, StoreError> {
    let (request, audit) = request.into_parts();
    crate::agent_credential_revocation::execute(&self.pool, request, &audit).await
  }
}

#[async_trait]
impl FactoryPhasePoolStore for PostgresStore {
  async fn phase_pool_selection_for_claim(
    &self,
    run: octacity_server_factory::FactoryRunId,
    claim: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<PhasePoolSelection>, StoreError> {
    let mut connection = self.pool.acquire().await.map_err(crate::database::unavailable)?;
    crate::factory_run::selection_for_claim(&mut connection, run, claim).await
  }
  async fn publish_phase_ready(
    &self,
    policy: octacity_server_factory::PhasePoolPolicy,
    input: PhasePoolInput,
  ) -> Result<PhasePoolEntry, StoreError> {
    crate::factory_run::publish_phase_ready(&self.pool, policy, input).await
  }
  async fn phase_ready_entries(
    &self,
    policy: octacity_server_factory::FactoryDigest,
    after: Option<octacity_server_factory::FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<PhasePoolEntry>, StoreError> {
    crate::factory_run::phase_ready_entries(&self.pool, policy, after, limit).await
  }
  async fn select_phase_ready(&self, request: SelectPhasePool) -> Result<Vec<PhasePoolSelection>, StoreError> {
    crate::factory_run::select_phase_ready(&self.pool, request).await
  }
}
