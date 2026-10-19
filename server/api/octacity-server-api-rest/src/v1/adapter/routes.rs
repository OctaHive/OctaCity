use octacity_server_application::{
  CancelFactoryRunCommand, ConfirmFactoryNodeOperationCommand, ListFactoryDiagnosticsQuery, RetryFactoryNodeCommand,
  SubmitFactoryNodeResponseCommand, ValidateFactoryFlowDataSchemaQuery,
};
use std::{any::TypeId, collections::BTreeSet, sync::Arc};

use axum::{
  Router,
  handler::Handler,
  routing::{MethodRouter, delete, get, post},
};
use octacity_server_application::{
  AcceptManualTriggerCommand, AdmitManualFactoryWorkCommand, AuthorizeArtifactDownloadQuery, CancelBuildCommand,
  CreateAgentPoolCommand, CreateBuildConfigurationCommand, CreateFactoryConfigurationCommand,
  CreateFactoryFlowDefinitionCommand, CreateInternalTriggerCommand, CreateManagedWebhookCommand, CreatePipelineCommand,
  CreateProjectCommand, CreateRepositoryCommand, CreateScheduleCommand, CreateTriggerDefinitionCommand,
  CreateUnmanagedWebhookCommand, DeleteAgentPoolCommand, DeleteManagedWebhookRegistrationCommand, DeleteProjectCommand,
  DrainAgentCommand, GetAgentPoolQuery, GetAgentQuery, GetArtifactQuery, GetAttemptQuery, GetBuildConfigurationQuery,
  GetBuildQuery, GetBuildResultRetentionQuery, GetCacheSessionQuery, GetCurrentFactoryConfigurationQuery,
  GetFactoryConfigurationQuery, GetFactoryFlowDefinitionQuery, GetFactoryRunQuery, GetInternalTriggerQuery,
  GetJobQuery, GetManualTriggerDefinitionQuery, GetOperationalMetadataQuery, GetPipelineQuery, GetProjectQuery,
  GetRepositoryQuery, GetScheduleQuery, IssueAgentEnrollmentCommand, ListAgentPoolsQuery, ListAgentsQuery,
  ListAuditFactsQuery, ListBuildArtifactsQuery, ListBuildCacheSessionsQuery, ListFactoryConfigurationChoicesQuery,
  ListFactoryPhaseReadyQuery, ListFactoryRunsQuery, ListInternalTriggersQuery, ListOperatorAttentionQuery,
  ListProjectBuildConfigurationsQuery, ListProjectBuildsQuery, ListProjectFactoryConfigurationsQuery,
  ListProjectFactoryFlowDefinitionsQuery, ListProjectPipelinesQuery, ListProjectRepositoriesQuery,
  ListProjectTriggerDefinitionsQuery, ListProjectsQuery, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  MoveProjectCommand, ObserveManagedWebhookRegistrationCommand, PlaceBuildResultHoldCommand,
  PublishAgentPoolVersionCommand, PublishBuildConfigurationVersionCommand, PublishInternalTriggerVersionCommand,
  PublishPipelineVersionCommand, PublishProjectPolicyCommand, PublishRepositoryVersionCommand, ReadJobEventsQuery,
  ReassignAgentPoolCommand, ReleaseBuildResultHoldCommand, RenameProjectCommand, ReplaceFactoryConfigurationCommand,
  ReplaceFactoryFlowDefinitionCommand, RetryBuildCommand, RotateManagedWebhookRegistrationCommand,
  SearchBuildLogsQuery, SearchResourcesQuery, ValidateFactoryConfigurationQuery, ValidateFactoryFlowDefinitionQuery,
};

use super::{
  MAX_MANAGEMENT_BODY_BYTES, ManagementApplication, accept_manual_trigger, admit_manual_factory_work, api_not_found,
  authorize_artifact_download, cancel_build, cancel_factory_run, confirm_factory_node_operation, create_agent_pool,
  create_build_configuration, create_factory_configuration, create_factory_flow_definition, create_internal_trigger,
  create_managed_webhook, create_manual_trigger_definition, create_pipeline, create_project, create_repository,
  create_scheduled_trigger_definition, create_unmanaged_webhook, delete_agent_pool, delete_managed_webhook,
  delete_project, drain_agent, get_agent, get_agent_pool, get_artifact, get_attempt, get_build,
  get_build_configuration, get_build_result_retention, get_cache_session, get_current_agent_pool,
  get_current_factory_configuration, get_current_factory_flow_definition, get_factory_configuration,
  get_factory_flow_definition, get_factory_run, get_internal_trigger, get_job, get_manual_trigger_definition,
  get_operational_metadata, get_pipeline, get_project, get_repository, get_schedule, issue_agent_enrollment,
  list_agent_pools, list_agents, list_audit_facts, list_build_artifacts, list_build_cache_sessions,
  list_factory_configuration_choices, list_factory_diagnostics, list_factory_phase_ready, list_factory_runs,
  list_internal_triggers, list_operator_attention, list_project_build_configurations, list_project_builds,
  list_project_factory_configurations, list_project_factory_flow_definitions, list_project_factory_phase_ready,
  list_project_factory_runs, list_project_pipelines, list_project_repositories, list_project_trigger_definitions,
  list_projects, move_project, observe_managed_webhook, openapi, place_build_result_hold, publish_agent_pool,
  publish_build_configuration, publish_internal_trigger, publish_pipeline, publish_project_policy, publish_repository,
  read_job_events, reassign_agent_pool, release_build_result_hold, rename_project, replace_factory_configuration,
  replace_factory_flow_definition, retry_build, retry_factory_node, rotate_managed_webhook, search_build_logs,
  search_resources, submit_factory_node_response, validate_factory_configuration, validate_factory_flow_data_schema,
  validate_factory_flow_definition,
};
use crate::v1::{API_PREFIX, MANAGEMENT_OPERATIONS, ManagementOperation};

/// One registered route and its statically declared authorization target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManagementAuthorizationOperation {
  /// Uppercase HTTP method.
  pub method: &'static str,
  /// Templated management path.
  pub path: &'static str,
  /// Stable OpenAPI operation identifier.
  pub operation_id: &'static str,
  /// Typed action and resource mapping declared by that request.
  pub authorization: ManagementAuthorizationMapping,
}

struct ManagementRoutes<S = Arc<ManagementApplication>> {
  router: Router<S>,
  operations: Vec<ManagementAuthorizationOperation>,
}

impl<S: Clone + Send + Sync + 'static> ManagementRoutes<S> {
  fn new() -> Self {
    Self {
      router: Router::new(),
      operations: Vec::new(),
    }
  }

  fn get<Request, HandlerType, HandlerArgs>(self, operation_id: &'static str, handler: HandlerType) -> Self
  where
    Request: ManagementAuthorizationTarget + 'static,
    HandlerType: Handler<HandlerArgs, S>,
    HandlerArgs: 'static,
  {
    self.register::<Request>("GET", operation_id, get(handler))
  }

  fn post<Request, HandlerType, HandlerArgs>(self, operation_id: &'static str, handler: HandlerType) -> Self
  where
    Request: ManagementAuthorizationTarget + 'static,
    HandlerType: Handler<HandlerArgs, S>,
    HandlerArgs: 'static,
  {
    self.register::<Request>("POST", operation_id, post(handler))
  }

  fn delete<Request, HandlerType, HandlerArgs>(self, operation_id: &'static str, handler: HandlerType) -> Self
  where
    Request: ManagementAuthorizationTarget + 'static,
    HandlerType: Handler<HandlerArgs, S>,
    HandlerArgs: 'static,
  {
    self.register::<Request>("DELETE", operation_id, delete(handler))
  }

  fn patch<Request, HandlerType, HandlerArgs>(self, operation_id: &'static str, handler: HandlerType) -> Self
  where
    Request: ManagementAuthorizationTarget + 'static,
    HandlerType: Handler<HandlerArgs, S>,
    HandlerArgs: 'static,
  {
    self.register::<Request>("PATCH", operation_id, axum::routing::patch(handler))
  }

  fn register<Request>(
    mut self,
    method: &'static str,
    operation_id: &'static str,
    method_router: MethodRouter<S>,
  ) -> Self
  where
    Request: ManagementAuthorizationTarget + 'static,
  {
    let operation = validated_operation::<Request>(method, operation_id);
    self.router = self.router.route(operation.path, method_router);
    self.operations.push(authorization_operation(operation));
    self
  }
}
impl ManagementRoutes<Arc<ManagementApplication>> {
  fn finish(self, mut application: ManagementApplication) -> Router {
    let protected = registered_integration_routes();
    let expected = MANAGEMENT_OPERATIONS
      .iter()
      .map(|operation| operation.operation_id)
      .collect::<BTreeSet<_>>();
    let registered = self
      .operations
      .iter()
      .chain(protected.operations.iter())
      .map(|operation| operation.operation_id)
      .collect::<BTreeSet<_>>();
    assert_eq!(registered, expected, "management router inventory is incomplete");
    assert_eq!(
      self.operations.len() + protected.operations.len(),
      registered.len(),
      "management router inventory contains duplicate operations"
    );

    let integration_application = application
      .integrations
      .take()
      .unwrap_or_else(super::integrations::IntegrationManagementApplication::unavailable);
    let protected = protected
      .router
      .layer(axum::extract::DefaultBodyLimit::max(32 * 1024))
      .layer(axum::middleware::from_fn(super::integrations::protected_ingress))
      .with_state::<Arc<ManagementApplication>>(Arc::new(integration_application));
    self
      .router
      .merge(protected)
      .route(&format!("{API_PREFIX}/openapi.json"), get(openapi))
      .fallback(api_not_found)
      .layer(axum::extract::DefaultBodyLimit::max(MAX_MANAGEMENT_BODY_BYTES))
      .with_state(Arc::new(application))
  }
}

fn validated_operation<Request>(method: &str, operation_id: &str) -> &'static ManagementOperation
where
  Request: ManagementAuthorizationTarget + 'static,
{
  let mut matching = MANAGEMENT_OPERATIONS
    .iter()
    .filter(|operation| operation.operation_id == operation_id);
  let operation = matching
    .next()
    .expect("registered route must name an OpenAPI operation");
  assert!(
    matching.next().is_none(),
    "OpenAPI operation identifiers must be unique"
  );
  assert_eq!(operation.method, method, "route method must match OpenAPI");
  assert_eq!(
    operation.authorization().request_type_id(),
    TypeId::of::<Request>(),
    "route request type must match its OpenAPI authorization target"
  );
  assert_eq!(
    operation.authorization().mapping(),
    Request::AUTHORIZATION,
    "route authorization mapping must match its request type"
  );
  assert!(
    Request::AUTHORIZATION.is_supported(),
    "route mapping must use a supported resource shape"
  );
  if let Some(visibility) = operation.visibility() {
    assert_eq!(
      visibility.query_type_id(),
      TypeId::of::<Request>(),
      "route request type must match its visibility declaration"
    );
  }
  assert!(
    operation.has_valid_visibility_contract(),
    "paged operations must declare exactly one typed visibility scope"
  );
  operation
}

fn authorization_operation(operation: &'static ManagementOperation) -> ManagementAuthorizationOperation {
  ManagementAuthorizationOperation {
    method: operation.method,
    path: operation.path,
    operation_id: operation.operation_id,
    authorization: operation.authorization().mapping(),
  }
}

/// Returns authorization metadata from the same declarations that build the router.
#[must_use]
pub fn management_authorization_operations() -> Vec<ManagementAuthorizationOperation> {
  registered_routes()
    .operations
    .into_iter()
    .chain(registered_integration_routes().operations)
    .collect()
}

fn registered_routes() -> ManagementRoutes {
  ManagementRoutes::new()
    .get::<octacity_server_application::ListIssueChildrenQuery, _, _>(
      "listIssueChildren",
      super::issue::planning::children,
    )
    .patch::<octacity_server_application::EditIssuePlanningCommand, _, _>(
      "editIssuePlanning",
      super::issue::planning::edit,
    )
    .get::<octacity_server_application::GetIssuePropertiesQuery, _, _>(
      "getIssueProperties",
      super::issue::planning::current,
    )
    .get::<octacity_server_application::GetIssuePropertiesQuery, _, _>(
      "getIssuePropertiesVersion",
      super::issue::planning::version,
    )
    .post::<octacity_server_application::PublishIssuePropertiesCommand, _, _>(
      "publishIssueProperties",
      super::issue::planning::publish,
    )
    .get::<octacity_server_application::GetIssuePropertyChoicesQuery, _, _>(
      "getIssuePropertyChoices",
      super::issue::planning::choices,
    )
    .post::<octacity_server_application::AdoptIssueWorkflowCommand, _, _>("adoptIssueWorkflow", super::issue::adopt)
    .get::<octacity_server_application::GetIssueTransitionsQuery, _, _>(
      "getIssueTransitions",
      super::issue::transitions,
    )
    .post::<octacity_server_application::TransitionIssueCommand, _, _>("transitionIssue", super::issue::transition)
    .get::<octacity_server_application::ListIssueStatusHistoryQuery, _, _>(
      "listIssueStatusHistory",
      super::issue::history,
    )
    .get::<octacity_server_application::ListIssueRevisionsQuery, _, _>("listIssueRevisions", super::issue::revisions)
    .get::<octacity_server_application::ListProjectIssuesQuery, _, _>("listProjectIssues", super::issue::list)
    .patch::<octacity_server_application::EditIssueCommand, _, _>("editIssue", super::issue::edit)
    .get::<octacity_server_application::GetIssueQuery, _, _>("getIssueRevision", super::issue::get_revision)
    .post::<octacity_server_application::CreateIssueCommand, _, _>("createIssue", super::issue::create)
    .get::<octacity_server_application::GetIssueQuery, _, _>("getIssue", super::issue::get)
    .get::<octacity_server_application::ListProjectIssueWorkflowsQuery, _, _>(
      "listProjectIssueWorkflows",
      super::issue_workflow::list,
    )
    .post::<octacity_server_application::SelectDefaultIssueWorkflowCommand, _, _>(
      "selectDefaultIssueWorkflow",
      super::issue_workflow::select_default,
    )
    .get::<octacity_server_application::GetIssueWorkflowQuery, _, _>(
      "getCurrentIssueWorkflow",
      super::issue_workflow::get_current,
    )
    .get::<octacity_server_application::GetIssueWorkflowQuery, _, _>(
      "getIssueWorkflowVersion",
      super::issue_workflow::get_version,
    )
    .post::<octacity_server_application::PublishIssueWorkflowCommand, _, _>(
      "createIssueWorkflow",
      super::issue_workflow::create,
    )
    .post::<octacity_server_application::PublishIssueWorkflowCommand, _, _>(
      "publishIssueWorkflowVersion",
      super::issue_workflow::publish,
    )
    .post::<octacity_server_application::ValidateIssueWorkflowQuery, _, _>(
      "validateIssueWorkflow",
      super::issue_workflow::validate,
    )
    .get::<octacity_server_application::GetDefaultIssueWorkflowQuery, _, _>(
      "getDefaultIssueWorkflow",
      super::issue_workflow::get_default,
    )
    .get::<GetOperationalMetadataQuery, _, _>("getOperationalMetadata", get_operational_metadata)
    .get::<SearchResourcesQuery, _, _>("searchResources", search_resources)
    .get::<ListOperatorAttentionQuery, _, _>("listOperatorAttention", list_operator_attention)
    .post::<CreateProjectCommand, _, _>("createProject", create_project)
    .get::<ListProjectsQuery, _, _>("listProjects", list_projects)
    .get::<GetProjectQuery, _, _>("getProject", get_project)
    .get::<ListProjectPipelinesQuery, _, _>("listProjectPipelines", list_project_pipelines)
    .get::<ListProjectRepositoriesQuery, _, _>("listProjectRepositories", list_project_repositories)
    .get::<ListProjectBuildConfigurationsQuery, _, _>(
      "listProjectBuildConfigurations",
      list_project_build_configurations,
    )
    .get::<ListProjectBuildsQuery, _, _>("listProjectBuilds", list_project_builds)
    .get::<ListProjectTriggerDefinitionsQuery, _, _>("listProjectTriggerDefinitions", list_project_trigger_definitions)
    .delete::<DeleteProjectCommand, _, _>("deleteProject", delete_project)
    .post::<RenameProjectCommand, _, _>("renameProject", rename_project)
    .post::<MoveProjectCommand, _, _>("moveProject", move_project)
    .post::<PublishProjectPolicyCommand, _, _>("publishProjectPolicyVersion", publish_project_policy)
    .post::<CreatePipelineCommand, _, _>("createPipeline", create_pipeline)
    .post::<PublishPipelineVersionCommand, _, _>("publishPipelineVersion", publish_pipeline)
    .get::<GetPipelineQuery, _, _>("getPipelineVersion", get_pipeline)
    .post::<CreateRepositoryCommand, _, _>("createRepository", create_repository)
    .post::<PublishRepositoryVersionCommand, _, _>("publishRepositoryVersion", publish_repository)
    .get::<GetRepositoryQuery, _, _>("getRepositoryVersion", get_repository)
    .post::<CreateBuildConfigurationCommand, _, _>("createBuildConfiguration", create_build_configuration)
    .post::<PublishBuildConfigurationVersionCommand, _, _>(
      "publishBuildConfigurationVersion",
      publish_build_configuration,
    )
    .get::<GetBuildConfigurationQuery, _, _>("getBuildConfigurationVersion", get_build_configuration)
    .post::<CreateTriggerDefinitionCommand, _, _>("createManualTriggerDefinition", create_manual_trigger_definition)
    .get::<GetManualTriggerDefinitionQuery, _, _>("getManualTriggerDefinitionVersion", get_manual_trigger_definition)
    .post::<CreateScheduleCommand, _, _>("createScheduledTriggerDefinition", create_scheduled_trigger_definition)
    .post::<CreateInternalTriggerCommand, _, _>("createInternalTriggerDefinition", create_internal_trigger)
    .get::<ListInternalTriggersQuery, _, _>("listInternalTriggerDefinitions", list_internal_triggers)
    .post::<PublishInternalTriggerVersionCommand, _, _>(
      "publishInternalTriggerDefinitionVersion",
      publish_internal_trigger,
    )
    .get::<GetInternalTriggerQuery, _, _>("getInternalTriggerDefinitionVersion", get_internal_trigger)
    .post::<CreateUnmanagedWebhookCommand, _, _>("createUnmanagedWebhookIntegration", create_unmanaged_webhook)
    .post::<CreateManagedWebhookCommand, _, _>("createManagedWebhookIntegration", create_managed_webhook)
    .post::<ObserveManagedWebhookRegistrationCommand, _, _>("observeManagedWebhookIntegration", observe_managed_webhook)
    .post::<RotateManagedWebhookRegistrationCommand, _, _>("rotateManagedWebhookIntegration", rotate_managed_webhook)
    .delete::<DeleteManagedWebhookRegistrationCommand, _, _>("deleteManagedWebhookIntegration", delete_managed_webhook)
    .get::<GetScheduleQuery, _, _>("getSchedule", get_schedule)
    .post::<AcceptManualTriggerCommand, _, _>("acceptManualTrigger", accept_manual_trigger)
    .get::<GetBuildQuery, _, _>("getBuild", get_build)
    .get::<GetBuildResultRetentionQuery, _, _>("getBuildResultRetention", get_build_result_retention)
    .post::<PlaceBuildResultHoldCommand, _, _>("placeBuildResultRetentionHold", place_build_result_hold)
    .post::<ReleaseBuildResultHoldCommand, _, _>("releaseBuildResultRetentionHold", release_build_result_hold)
    .get::<ListBuildArtifactsQuery, _, _>("listBuildArtifacts", list_build_artifacts)
    .get::<GetArtifactQuery, _, _>("getArtifact", get_artifact)
    .post::<AuthorizeArtifactDownloadQuery, _, _>("authorizeArtifactDownload", authorize_artifact_download)
    .get::<ListBuildCacheSessionsQuery, _, _>("listBuildCacheSessions", list_build_cache_sessions)
    .get::<GetCacheSessionQuery, _, _>("getCacheSession", get_cache_session)
    .post::<CancelBuildCommand, _, _>("cancelBuild", cancel_build)
    .post::<RetryBuildCommand, _, _>("retryBuild", retry_build)
    .get::<GetAttemptQuery, _, _>("getAttempt", get_attempt)
    .post::<CreateAgentPoolCommand, _, _>("createAgentPool", create_agent_pool)
    .get::<ListAgentPoolsQuery, _, _>("listAgentPools", list_agent_pools)
    .get::<GetAgentPoolQuery, _, _>("getCurrentAgentPool", get_current_agent_pool)
    .delete::<DeleteAgentPoolCommand, _, _>("deleteAgentPool", delete_agent_pool)
    .post::<PublishAgentPoolVersionCommand, _, _>("publishAgentPoolVersion", publish_agent_pool)
    .get::<GetAgentPoolQuery, _, _>("getAgentPoolVersion", get_agent_pool)
    .get::<ListAgentsQuery, _, _>("listAgents", list_agents)
    .post::<IssueAgentEnrollmentCommand, _, _>("issueAgentEnrollment", issue_agent_enrollment)
    .get::<GetAgentQuery, _, _>("getAgent", get_agent)
    .post::<ReassignAgentPoolCommand, _, _>("reassignAgentPool", reassign_agent_pool)
    .post::<DrainAgentCommand, _, _>("drainAgent", drain_agent)
    .get::<GetJobQuery, _, _>("getJob", get_job)
    .get::<ReadJobEventsQuery, _, _>("readJobEvents", read_job_events)
    .get::<SearchBuildLogsQuery, _, _>("searchBuildLogs", search_build_logs)
    .get::<ListAuditFactsQuery, _, _>("listAuditFacts", list_audit_facts)
    .get::<ListFactoryPhaseReadyQuery, _, _>("listFactoryPhaseReady", list_factory_phase_ready)
    .get::<ListFactoryPhaseReadyQuery, _, _>("listProjectFactoryPhaseReady", list_project_factory_phase_ready)
    .get::<ListFactoryDiagnosticsQuery, _, _>("listFactoryDiagnostics", list_factory_diagnostics)
    .get::<GetFactoryRunQuery, _, _>("getFactoryRun", get_factory_run)
    .post::<CancelFactoryRunCommand, _, _>("cancelFactoryRun", cancel_factory_run)
    .post::<SubmitFactoryNodeResponseCommand, _, _>("submitFactoryNodeResponse", submit_factory_node_response)
    .post::<RetryFactoryNodeCommand, _, _>("retryFactoryNode", retry_factory_node)
    .post::<ConfirmFactoryNodeOperationCommand, _, _>("confirmFactoryNodeOperation", confirm_factory_node_operation)
    .get::<ListFactoryRunsQuery, _, _>("listFactoryRuns", list_factory_runs)
    .get::<ListFactoryRunsQuery, _, _>("listProjectFactoryRuns", list_project_factory_runs)
    .post::<AdmitManualFactoryWorkCommand, _, _>("admitManualFactoryWork", admit_manual_factory_work)
    .post::<CreateFactoryFlowDefinitionCommand, _, _>("createFactoryFlowDefinition", create_factory_flow_definition)
    .post::<ReplaceFactoryFlowDefinitionCommand, _, _>("replaceFactoryFlowDefinition", replace_factory_flow_definition)
    .get::<GetFactoryFlowDefinitionQuery, _, _>("getFactoryFlowDefinition", get_factory_flow_definition)
    .get::<GetFactoryFlowDefinitionQuery, _, _>("getCurrentFactoryFlowDefinition", get_current_factory_flow_definition)
    .get::<ListProjectFactoryFlowDefinitionsQuery, _, _>(
      "listProjectFactoryFlowDefinitions",
      list_project_factory_flow_definitions,
    )
    .post::<ValidateFactoryFlowDefinitionQuery, _, _>("validateFactoryFlowDefinition", validate_factory_flow_definition)
    .post::<ValidateFactoryFlowDataSchemaQuery, _, _>(
      "validateFactoryFlowDataSchema",
      validate_factory_flow_data_schema,
    )
    .post::<ValidateFactoryConfigurationQuery, _, _>("validateFactoryConfiguration", validate_factory_configuration)
    .post::<ReplaceFactoryConfigurationCommand, _, _>("replaceFactoryConfiguration", replace_factory_configuration)
    .post::<CreateFactoryConfigurationCommand, _, _>("createFactoryConfiguration", create_factory_configuration)
    .get::<ListFactoryConfigurationChoicesQuery, _, _>(
      "listFactoryConfigurationChoices",
      list_factory_configuration_choices,
    )
    .get::<GetFactoryConfigurationQuery, _, _>("getFactoryConfiguration", get_factory_configuration)
    .get::<GetCurrentFactoryConfigurationQuery, _, _>(
      "getCurrentFactoryConfiguration",
      get_current_factory_configuration,
    )
    .get::<ListProjectFactoryConfigurationsQuery, _, _>(
      "listProjectFactoryConfigurations",
      list_project_factory_configurations,
    )
}

/// Builds the registered management routes below `/api/v1`.
pub fn router(application: ManagementApplication) -> Router {
  registered_routes().finish(application)
}

fn registered_integration_routes() -> ManagementRoutes<Arc<super::integrations::IntegrationManagementApplication>> {
  use super::integrations::*;
  use octacity_server_application::{
    CheckIntegrationConnectionQuery, GetWorkSourceConfigurationQuery, GetWorkSourceDiagnosticsQuery,
    ListWorkSourceConfigurationsQuery, ListWorkSourceInputChoicesQuery, PreviewWorkSourcesQuery,
    PublishWorkSourceConfigurationCommand, ValidateWorkSourceConfigurationQuery,
  };
  use octacity_server_application::{
    CleanupIntegrationPreparationsCommand, GetIntegrationAdministrationQuery, GetIntegrationConnectionQuery,
    GetIntegrationProjectAccessQuery, ListIntegrationConnectionsQuery, ListIntegrationInstallationsQuery,
    ListIntegrationProjectResourcesQuery, ManageIntegrationCredentialCommand, PublishIntegrationConnectionCommand,
    PublishIntegrationProjectAccessCommand, SetIntegrationConnectionStateCommand, ValidateIntegrationConnectionQuery,
  };
  ManagementRoutes::new()
    .get::<octacity_server_application::ListIntegrationDependenciesQuery, _, _>(
      "listIntegrationDependencies",
      list_dependencies,
    )
    .get::<ListIntegrationConnectionsQuery, _, _>("listIntegrationConnections", list_connections)
    .get::<ListIntegrationInstallationsQuery, _, _>("listIntegrationInstallations", list_installations)
    .post::<ValidateIntegrationConnectionQuery, _, _>("validateIntegrationConnection", validate_connection)
    .post::<PreviewWorkSourcesQuery, _, _>("previewWorkSources", preview_sources)
    .get::<GetWorkSourceDiagnosticsQuery, _, _>("getWorkSourceInputDiagnostics", source_input_diagnostics)
    .get::<GetWorkSourceDiagnosticsQuery, _, _>("getWorkSourceGroupDiagnostics", source_group_diagnostics)
    .post::<PublishWorkSourceConfigurationCommand, _, _>("publishWorkSourceInput", publish_source_input)
    .post::<PublishWorkSourceConfigurationCommand, _, _>("publishWorkSourceGroup", publish_source_group)
    .post::<ValidateWorkSourceConfigurationQuery, _, _>(
      "validateWorkSourceConfiguration",
      validate_source_configuration,
    )
    .get::<ListWorkSourceInputChoicesQuery, _, _>("listWorkSourceInputChoices", source_choices)
    .get::<ListWorkSourceConfigurationsQuery, _, _>("listWorkSourceConfigurations", list_source_configurations)
    .get::<GetWorkSourceConfigurationQuery, _, _>("getWorkSourceInput", get_source_input)
    .get::<GetWorkSourceConfigurationQuery, _, _>("getWorkSourceGroup", get_source_group)
    .post::<CheckIntegrationConnectionQuery, _, _>("checkIntegrationConnection", check_connection)
    .post::<PublishIntegrationProjectAccessCommand, _, _>("publishIntegrationProjectAccess", publish_project_access)
    .get::<GetIntegrationProjectAccessQuery, _, _>("getIntegrationProjectAccess", get_project_access)
    .get::<ListIntegrationProjectResourcesQuery, _, _>("listIntegrationProjectResources", list_project_resources)
    .post::<ManageIntegrationCredentialCommand, _, _>("manageIntegrationCredential", submit_credentials)
    .post::<PublishIntegrationConnectionCommand, _, _>("publishIntegrationConnection", publish_metadata)
    .get::<GetIntegrationConnectionQuery, _, _>("getIntegrationConnection", get_metadata)
    .get::<GetIntegrationConnectionQuery, _, _>("getIntegrationConnectionVersion", get_metadata_version)
    .post::<SetIntegrationConnectionStateCommand, _, _>("setIntegrationConnectionState", set_state)
    .get::<GetIntegrationAdministrationQuery, _, _>("getIntegrationAdministration", get_state)
    .post::<CleanupIntegrationPreparationsCommand, _, _>("cleanupIntegrationPreparations", cleanup_preparations)
}
pub(super) fn integration_router(application: super::integrations::IntegrationManagementApplication) -> Router {
  registered_integration_routes()
    .router
    .layer(axum::extract::DefaultBodyLimit::max(32 * 1024))
    .layer(axum::middleware::from_fn(super::integrations::protected_ingress))
    .with_state(Arc::new(application))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[should_panic(expected = "route request type must match its OpenAPI authorization target")]
  fn route_registration_rejects_the_wrong_typed_target() {
    let _ = validated_operation::<GetProjectQuery>("POST", "createProject");
  }
}
