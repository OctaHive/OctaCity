//! Factory ports implemented by the concrete PostgreSQL stores.

use async_trait::async_trait;
use octacity_observability::Operation;
use octacity_server_domain::ProjectId;
use octacity_server_store::*;

use super::{PostgresAuthoritativeStore, PostgresStore};

#[async_trait]
impl FactoryBuildAcceptanceStore for PostgresAuthoritativeStore {
  async fn factory_build_for_operation(
    &self,
    operation: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<AcceptedFactoryBuild>, StoreError> {
    crate::telemetry::observe(
      Operation::Verify,
      crate::factory::execution::accept_factory_build::read(&self.store.pool, operation),
    )
    .await
  }
  async fn accept_factory_build(&self, request: AcceptFactoryBuild) -> Result<AcceptedFactoryBuild, StoreError> {
    crate::telemetry::observe(
      Operation::Accept,
      crate::factory::execution::accept_factory_build::accept(&self.store.pool, &self.job_spec_signer, request),
    )
    .await
  }
}

#[async_trait]
impl FactoryBuildCancellationStore for PostgresAuthoritativeStore {
  async fn cancel_factory_build_execution(
    &self,
    request: CancelFactoryBuildExecution,
  ) -> Result<CancellationDisposition, StoreError> {
    crate::telemetry::observe(
      Operation::Cancel,
      crate::cicd::builds::cancel_build::execute_factory(&self.store.pool, request),
    )
    .await
  }
}

#[async_trait]
impl FactoryConfigurationStore for PostgresStore {
  async fn replay_factory_configuration_mutation(
    &self,
    request: ManagementMutation<ReplayFactoryConfigurationMutation>,
  ) -> Result<Option<FactoryConfigurationMutationOutcome>, StoreError> {
    crate::factory::authoring::factory_configuration::replay_mutation(&self.pool, request).await
  }

  async fn factory_configuration_availability(
    &self,
    project_id: ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    let installed = self.factory_configuration_installation(project_id).await?;
    crate::factory::authoring::factory_runtime_settings::configured_availability(self.pool(), project_id, installed)
      .await
  }

  async fn factory_configuration_installation(
    &self,
    project_id: ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    crate::discovery::require_project(&self.pool, project_id).await?;
    let mut availability = self.factory_capabilities().availability(project_id).await?;
    if let FactoryConfigurationAvailability::Available(choices) = &mut availability {
      let deleted: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM factory_roles WHERE project_id=$1 AND deleted")
        .bind(project_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(crate::database::unavailable)?;
      for id in deleted {
        choices
          .exclude_role(octacity_server_factory::FactoryRoleId::from_uuid(id).map_err(|_| StoreError::Unavailable)?);
      }
    }
    Ok(availability)
  }

  async fn refresh_factory_role_models(
    &self,
    project: ProjectId,
    alias: &octacity_server_factory::FactoryKey,
  ) -> Result<(), StoreError> {
    crate::discovery::require_project(&self.pool, project).await?;
    self.factory_capabilities().refresh_role_models(project, alias).await
  }

  async fn create_factory_configuration(
    &self,
    request: ManagementMutation<CreateFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, StoreError> {
    crate::factory::authoring::factory_configuration::create(&self.pool, request).await
  }

  async fn replace_factory_configuration(
    &self,
    request: ManagementMutation<ReplaceFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, ReplaceFactoryConfigurationError> {
    crate::factory::authoring::factory_configuration::replace(&self.pool, request).await
  }

  async fn factory_configuration_version(
    &self,
    id: octacity_server_factory::FactoryConfigurationId,
    version: octacity_server_factory::FactoryConfigurationVersion,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    crate::factory::authoring::factory_configuration::read(&self.pool, id, version).await
  }

  async fn current_factory_configuration(
    &self,
    id: octacity_server_factory::FactoryConfigurationId,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    crate::factory::authoring::factory_configuration::read_current(&self.pool, id).await
  }
}

#[async_trait]
impl FactoryAdmissionStore for PostgresStore {
  async fn recover_factory_admission(
    &self,
    identity: &FactoryAdmissionIdentity,
  ) -> Result<Option<PublishedFactoryAdmission>, StoreError> {
    crate::factory::execution::factory_admission::recover(&self.pool, identity).await
  }

  async fn replay_factory_admission(
    &self,
    probe: &FactoryAdmissionProbe,
  ) -> Result<Option<FactoryAdmissionMutationOutcome>, StoreError> {
    crate::factory::execution::factory_admission::replay(&self.pool, probe).await
  }

  async fn factory_admission_context(
    &self,
    request: ReadFactoryAdmissionContext,
  ) -> Result<FactoryAdmissionContext, StoreError> {
    crate::factory::execution::factory_admission::context(&self.pool, request).await
  }

  async fn admit_factory_work(
    &self,
    request: ManagementMutation<AdmitFactoryWork>,
  ) -> Result<FactoryAdmissionMutationOutcome, StoreError> {
    crate::factory::execution::factory_admission::admit(&self.pool, request).await
  }
}

#[async_trait]
impl FactoryDiscoveryStore for PostgresStore {
  async fn list_phase_ready_entries(
    &self,
    request: octacity_server_store::ListPhaseReadyEntries,
  ) -> Result<octacity_server_store::PhaseReadyPage, StoreError> {
    crate::factory::execution::factory_run::list_phase_ready_entries(&self.pool, request).await
  }

  async fn list_project_factory_configurations(
    &self,
    request: ListProjectFactoryConfigurations,
  ) -> Result<CurrentFactoryConfigurationPage, StoreError> {
    crate::factory::authoring::factory_discovery::list_configurations(&self.pool, request).await
  }

  async fn list_factory_runs(&self, request: ListFactoryRuns) -> Result<FactoryRunPage, StoreError> {
    crate::factory::authoring::factory_discovery::list_runs(&self.pool, request).await
  }

  async fn factory_run_summary(
    &self,
    run_id: octacity_server_factory::FactoryRunId,
    visibility: FactoryRunListVisibility,
  ) -> Result<FactoryRunSummary, StoreError> {
    crate::factory::authoring::factory_discovery::read_run(&self.pool, run_id, visibility).await
  }
}

#[async_trait]
impl FactoryRunStore for PostgresStore {
  async fn factory_run_snapshot(
    &self,
    run_id: octacity_server_factory::FactoryRunId,
  ) -> Result<FactoryRunSnapshot, StoreError> {
    crate::factory::execution::factory_run::read_snapshot(&self.pool, run_id).await
  }

  async fn list_factory_run_diagnostics(
    &self,
    request: octacity_server_store::ListFactoryRunDiagnostics,
  ) -> Result<octacity_server_store::FactoryRunDiagnosticPage, StoreError> {
    crate::factory::execution::factory_run::list_diagnostics(&self.pool, request).await
  }

  async fn claim_factory_run(&self, request: ClaimFactoryRun) -> Result<ClaimFactoryRunOutcome, StoreError> {
    crate::factory::execution::factory_run::claim(&self.pool, request).await
  }

  async fn claim_factory_runs(&self, request: ClaimFactoryRuns) -> Result<Vec<ClaimedFactoryRun>, StoreError> {
    crate::factory::execution::factory_run::claim_batch(&self.pool, request).await
  }

  async fn commit_factory_run_transition(
    &self,
    request: CommitFactoryRunTransition,
  ) -> Result<CommitFactoryRunTransitionOutcome, StoreError> {
    crate::factory::execution::factory_run::commit_transition(&self.pool, request).await
  }

  async fn claim_factory_outbox(&self, request: ClaimFactoryOutbox) -> Result<Vec<ClaimedFactoryOutbox>, StoreError> {
    crate::factory::execution::factory_run::claim_outbox(&self.pool, request).await
  }

  async fn settle_factory_outbox(&self, request: SettleFactoryOutbox) -> Result<FactoryOutboxRecord, StoreError> {
    crate::factory::execution::factory_run::settle_outbox(&self.pool, request).await
  }
}

#[async_trait]
impl FactoryRunControlStore for PostgresStore {
  async fn apply_factory_run_control(
    &self,
    request: ManagementMutation<ApplyFactoryRunControl>,
  ) -> Result<FactoryRunControlOutcome, StoreError> {
    crate::factory::execution::factory_run::apply_control(&self.pool, request).await
  }
}

#[async_trait]
impl FactoryPhasePoolStore for PostgresStore {
  async fn ready_phase_policies(
    &self,
    after: Option<octacity_server_factory::FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<octacity_server_factory::FactoryDigest>, StoreError> {
    crate::factory::execution::factory_run::ready_phase_policies(&self.pool, after, limit).await
  }

  async fn phase_pool_selection_for_claim(
    &self,
    run: octacity_server_factory::FactoryRunId,
    claim: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<PhasePoolSelection>, StoreError> {
    let mut connection = self.pool.acquire().await.map_err(crate::database::unavailable)?;
    crate::factory::execution::factory_run::selection_for_claim(&mut connection, run, claim).await
  }
  async fn publish_phase_ready(
    &self,
    policy: octacity_server_factory::PhasePoolPolicy,
    input: PhasePoolInput,
  ) -> Result<PhasePoolEntry, StoreError> {
    crate::factory::execution::factory_run::publish_phase_ready(&self.pool, policy, input).await
  }
  async fn phase_ready_entries(
    &self,
    policy: octacity_server_factory::FactoryDigest,
    after: Option<octacity_server_factory::FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<PhasePoolEntry>, StoreError> {
    crate::factory::execution::factory_run::phase_ready_entries(&self.pool, policy, after, limit).await
  }
  async fn select_phase_ready(&self, request: SelectPhasePool) -> Result<Vec<PhasePoolSelection>, StoreError> {
    crate::factory::execution::factory_run::select_phase_ready(&self.pool, request).await
  }
}

#[async_trait]
impl octacity_server_store::FactoryFlowDefinitionStore for PostgresStore {
  async fn replay_factory_flow_definition(
    &self,
    request: ManagementMutation<octacity_server_store::ReplayFactoryFlowDefinition>,
  ) -> Result<Option<octacity_server_store::FactoryFlowDefinitionMutationOutcome>, StoreError> {
    crate::factory::authoring::factory_flow_definition::replay_mutation(&self.pool, request).await
  }
  async fn publish_factory_flow_definition(
    &self,
    request: ManagementMutation<octacity_server_store::PublishFactoryFlowDefinition>,
  ) -> Result<octacity_server_store::FactoryFlowDefinitionMutationOutcome, ReplaceFactoryConfigurationError> {
    crate::factory::authoring::factory_flow_definition::publish(&self.pool, request).await
  }
  async fn factory_flow_definition(
    &self,
    id: octacity_server_factory::FlowDefinitionId,
    version: Option<octacity_server_factory::FlowDefinitionVersion>,
  ) -> Result<octacity_server_store::PublishedFactoryFlowDefinition, StoreError> {
    crate::factory::authoring::factory_flow_definition::read(&self.pool, id, version).await
  }
  async fn list_factory_flow_definitions(
    &self,
    project: ProjectId,
    after: Option<octacity_server_store::FlowDefinitionPagePosition>,
    limit: u16,
    visibility: octacity_server_store::FlowDefinitionListVisibility,
  ) -> Result<octacity_server_store::FactoryFlowDefinitionPage, StoreError> {
    crate::factory::authoring::factory_flow_definition::list(&self.pool, project, after, limit, visibility).await
  }
}
