use std::{any::TypeId, collections::BTreeSet, sync::Arc};

use axum::{
  Router,
  handler::Handler,
  routing::{MethodRouter, delete, get, post},
};
use octacity_server_application::{
  AcceptManualTriggerCommand, AuthorizeArtifactDownloadQuery, CancelBuildCommand, CreateAgentPoolCommand,
  CreateBuildConfigurationCommand, CreateInternalTriggerCommand, CreateManagedWebhookCommand, CreatePipelineCommand,
  CreateProjectCommand, CreateRepositoryCommand, CreateScheduleCommand, CreateTriggerDefinitionCommand,
  CreateUnmanagedWebhookCommand, DeleteAgentPoolCommand, DeleteManagedWebhookRegistrationCommand, DeleteProjectCommand,
  DrainAgentCommand, GetAgentPoolQuery, GetAgentQuery, GetArtifactQuery, GetAttemptQuery, GetBuildConfigurationQuery,
  GetBuildQuery, GetBuildResultRetentionQuery, GetCacheSessionQuery, GetInternalTriggerQuery, GetJobQuery,
  GetManualTriggerDefinitionQuery, GetOperationalMetadataQuery, GetPipelineQuery, GetProjectQuery, GetRepositoryQuery,
  GetScheduleQuery, IssueAgentEnrollmentCommand, ListAgentPoolsQuery, ListAgentsQuery, ListAuditFactsQuery,
  ListBuildArtifactsQuery, ListBuildCacheSessionsQuery, ListInternalTriggersQuery, ListProjectBuildConfigurationsQuery,
  ListProjectPipelinesQuery, ListProjectRepositoriesQuery, ListProjectTriggerDefinitionsQuery, ListProjectsQuery,
  ManagementAuthorizationMapping, ManagementAuthorizationTarget, MoveProjectCommand,
  ObserveManagedWebhookRegistrationCommand, PlaceBuildResultHoldCommand, PublishAgentPoolVersionCommand,
  PublishBuildConfigurationVersionCommand, PublishInternalTriggerVersionCommand, PublishPipelineVersionCommand,
  PublishProjectPolicyCommand, PublishRepositoryVersionCommand, ReadJobEventsQuery, ReassignAgentPoolCommand,
  ReleaseBuildResultHoldCommand, RenameProjectCommand, RetryBuildCommand, RotateManagedWebhookRegistrationCommand,
  SearchBuildLogsQuery,
};

use super::{
  MAX_MANAGEMENT_BODY_BYTES, ManagementApplication, accept_manual_trigger, api_not_found, authorize_artifact_download,
  cancel_build, create_agent_pool, create_build_configuration, create_internal_trigger, create_managed_webhook,
  create_manual_trigger_definition, create_pipeline, create_project, create_repository,
  create_scheduled_trigger_definition, create_unmanaged_webhook, delete_agent_pool, delete_managed_webhook,
  delete_project, drain_agent, get_agent, get_agent_pool, get_artifact, get_attempt, get_build,
  get_build_configuration, get_build_result_retention, get_cache_session, get_internal_trigger, get_job,
  get_manual_trigger_definition, get_operational_metadata, get_pipeline, get_project, get_repository, get_schedule,
  issue_agent_enrollment, list_agent_pools, list_agents, list_audit_facts, list_build_artifacts,
  list_build_cache_sessions, list_internal_triggers, list_project_build_configurations, list_project_pipelines,
  list_project_repositories, list_project_trigger_definitions, list_projects, move_project, observe_managed_webhook,
  openapi, place_build_result_hold, publish_agent_pool, publish_build_configuration, publish_internal_trigger,
  publish_pipeline, publish_project_policy, publish_repository, read_job_events, reassign_agent_pool,
  release_build_result_hold, rename_project, retry_build, rotate_managed_webhook, search_build_logs,
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

struct ManagementRoutes {
  router: Router<Arc<ManagementApplication>>,
  operations: Vec<ManagementAuthorizationOperation>,
}

impl ManagementRoutes {
  fn new() -> Self {
    Self {
      router: Router::new(),
      operations: Vec::new(),
    }
  }

  fn get<Request, HandlerType, HandlerArgs>(self, operation_id: &'static str, handler: HandlerType) -> Self
  where
    Request: ManagementAuthorizationTarget + 'static,
    HandlerType: Handler<HandlerArgs, Arc<ManagementApplication>>,
    HandlerArgs: 'static,
  {
    self.register::<Request>("GET", operation_id, get(handler))
  }

  fn post<Request, HandlerType, HandlerArgs>(self, operation_id: &'static str, handler: HandlerType) -> Self
  where
    Request: ManagementAuthorizationTarget + 'static,
    HandlerType: Handler<HandlerArgs, Arc<ManagementApplication>>,
    HandlerArgs: 'static,
  {
    self.register::<Request>("POST", operation_id, post(handler))
  }

  fn delete<Request, HandlerType, HandlerArgs>(self, operation_id: &'static str, handler: HandlerType) -> Self
  where
    Request: ManagementAuthorizationTarget + 'static,
    HandlerType: Handler<HandlerArgs, Arc<ManagementApplication>>,
    HandlerArgs: 'static,
  {
    self.register::<Request>("DELETE", operation_id, delete(handler))
  }

  fn register<Request>(
    mut self,
    method: &'static str,
    operation_id: &'static str,
    method_router: MethodRouter<Arc<ManagementApplication>>,
  ) -> Self
  where
    Request: ManagementAuthorizationTarget + 'static,
  {
    let operation = validated_operation::<Request>(method, operation_id);
    self.router = self.router.route(operation.path, method_router);
    self.operations.push(authorization_operation(operation));
    self
  }

  fn finish(self, application: ManagementApplication) -> Router {
    let expected = MANAGEMENT_OPERATIONS
      .iter()
      .map(|operation| operation.operation_id)
      .collect::<BTreeSet<_>>();
    let registered = self
      .operations
      .iter()
      .map(|operation| operation.operation_id)
      .collect::<BTreeSet<_>>();
    assert_eq!(registered, expected, "management router inventory is incomplete");
    assert_eq!(
      self.operations.len(),
      registered.len(),
      "management router inventory contains duplicate operations"
    );

    self
      .router
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
  registered_routes().operations
}

fn registered_routes() -> ManagementRoutes {
  ManagementRoutes::new()
    .get::<GetOperationalMetadataQuery, _, _>("getOperationalMetadata", get_operational_metadata)
    .post::<CreateProjectCommand, _, _>("createProject", create_project)
    .get::<ListProjectsQuery, _, _>("listProjects", list_projects)
    .get::<GetProjectQuery, _, _>("getProject", get_project)
    .get::<ListProjectPipelinesQuery, _, _>("listProjectPipelines", list_project_pipelines)
    .get::<ListProjectRepositoriesQuery, _, _>("listProjectRepositories", list_project_repositories)
    .get::<ListProjectBuildConfigurationsQuery, _, _>(
      "listProjectBuildConfigurations",
      list_project_build_configurations,
    )
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
}

/// Builds the registered management routes below `/api/v1`.
pub fn router(application: ManagementApplication) -> Router {
  registered_routes().finish(application)
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
