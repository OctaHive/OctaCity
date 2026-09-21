use std::{
  sync::Arc,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
  Json, Router,
  extract::{
    DefaultBodyLimit, Extension, OriginalUri, Path, Query, State,
    rejection::{JsonRejection, QueryRejection},
  },
  http::{HeaderMap, StatusCode},
  response::IntoResponse,
  routing::{get, post},
};
use octacity_server_application::{
  AgentCommandOutcome, AgentEnrollmentSecretKey, AgentPageProjection, AgentPoolCommandOutcome, AgentPoolPageProjection,
  AgentPoolProjection, AgentProjection, ApplicationFailure, AttemptDetailsProjection, BuildConfigurationCommandOutcome,
  BuildConfigurationProjection, BuildDetailsProjection, CancelBuildCommandOutcome, DeleteAgentPoolCommandOutcome,
  DeleteProjectCommandOutcome, DependencyPolicyProjection as ApplicationDependencyPolicy, JobEventPageProjection,
  JobProjection, ManagementInputError, ManagementInputFactory, ManualTriggerDefinitionInput, ManualTriggerInput,
  ManualTriggerOutcome, MutationDisposition as ApplicationMutationDisposition,
  NetworkPolicyProjection as ApplicationNetworkPolicy, ParameterTypeProjection as ApplicationParameterType,
  PipelineCommandOutcome, PipelineProjection, PlatformArchitectureProjection as ApplicationPlatformArchitecture,
  PlatformOsProjection as ApplicationPlatformOs, ProjectCommandOutcome, ProjectPageProjection, ProjectProjection,
  RepositoryCommandOutcome, RepositoryProjection, RetryBuildCommandOutcome,
  RetryClassProjection as ApplicationRetryClass, RuntimeClassProjection as ApplicationRuntimeClass,
  TriggerKindProjection as ApplicationTriggerKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
  API_PREFIX, AcceptManualTriggerRequest, AgentCapacity, AgentDrainMode, AgentPlatform, AgentPoolAdmissionPolicy,
  AgentPoolDefinition, AgentPoolDrainState, AgentPoolFairnessPolicy, AgentPoolResource, AgentRequirements,
  AgentResource, AgentStatus, ArtifactPolicy, AttemptResource, AttemptSummaryResource, BuildConfigurationDefinition,
  BuildConfigurationResource, BuildResource, CachePolicy, CancelBuildResponse, CreateAgentPoolRequest,
  CreateBuildConfigurationRequest, CreateManualTriggerDefinitionRequest, CreatePipelineRequest, CreateProjectRequest,
  CreateRepositoryRequest, Cursor, CursorPage, DagEdgeResource, DeleteAgentPoolResponse, DeleteProjectResponse,
  DependencyPolicy, DrainAgentRequest, ErrorCode, IDEMPOTENCY_KEY_HEADER, IdempotencyKey, IssueAgentEnrollmentRequest,
  IssueAgentEnrollmentResponse, JobAssignmentResource, JobEventPage, JobEventResource, JobExecution, JobQueueResource,
  JobResource, JobTerminalResource, MoveProjectRequest, MutationDisposition, MutationResponse, NetworkPolicy,
  OPTIMISTIC_PRECONDITION_HEADER, ParameterDefinition, ParameterSchema, ParameterType, PipelineDag, PipelineEdge,
  PipelineNode, PipelineResource, PlatformArchitecture, PlatformOs, ProjectDetails, ProjectPolicyResource,
  ProjectResource, PublishAgentPoolVersionRequest, PublishBuildConfigurationVersionRequest,
  PublishPipelineVersionRequest, PublishProjectPolicyRequest, PublishRepositoryVersionRequest,
  ReassignAgentPoolRequest, RenameProjectRequest, RepositoryDefinition, RepositoryResource, RepositorySelectionPolicy,
  RetryBuildResponse, RetryClass, RetryPolicy, RuntimeClass, RuntimePolicy, TriggerDefinitionResource,
  TriggerEvaluationResponse, TriggerKind, VersionPrecondition,
};
use crate::RequestId;

mod agent;
mod agent_pool;
mod application;
mod error;
mod execution;
mod representations;

use agent::{drain_agent, get_agent, issue_agent_enrollment, list_agents, reassign_agent_pool};
use agent_pool::{create_agent_pool, delete_agent_pool, get_agent_pool, list_agent_pools, publish_agent_pool};
use application::{AgentEndpoints, AgentPoolManagementApplication};
pub use application::{
  AgentManagementApplication, BuildManagementApplication, CatalogManagementApplication,
  ConfigurationManagementApplication, DefinitionManagementApplication, ExecutionManagementApplication,
  JobEventManagementApplication, ManagementApplicationHandlers, ManualTriggerManagementApplication,
  PipelineManagementApplication, ProjectManagementApplication,
};
use error::ApiError;
use execution::{cancel_build, get_attempt, get_build, get_job, retry_build};
use representations::*;

const MAX_MANAGEMENT_BODY_BYTES: usize = 8 * 1024 * 1024;
pub(super) const DEFAULT_PAGE_LIMIT: u16 = 50;
pub(super) const DEFAULT_JOB_EVENT_LIMIT: u16 = 100;

/// Typed application handlers used by the v1 management REST adapter.
///
/// Construction accepts only application-layer command and query handlers. The
/// REST crate has no dependency on an infrastructure adapter.
pub struct ManagementApplication {
  inputs: ManagementInputFactory,
  projects: ProjectManagementApplication,
  pipelines: PipelineManagementApplication,
  configurations: ConfigurationManagementApplication,
  agent_pools: AgentPoolManagementApplication,
  agents: AgentEndpoints,
  builds: BuildManagementApplication,
  definitions: DefinitionManagementApplication,
  manual_triggers: ManualTriggerManagementApplication,
  job_events: JobEventManagementApplication,
}

impl ManagementApplication {
  /// Creates the management adapter registered through the static Agent Pool slice.
  pub fn new(
    supported_pipeline_capabilities: impl IntoIterator<Item = String>,
    agent_enrollment_lifetime: Duration,
    agent_enrollment_secret_key: AgentEnrollmentSecretKey,
    handlers: ManagementApplicationHandlers,
  ) -> Result<Self, ManagementInputError> {
    let ManagementApplicationHandlers {
      catalog,
      agents: agent_management,
      execution,
    } = handlers;
    let CatalogManagementApplication {
      projects,
      pipelines,
      configurations,
      definitions,
    } = catalog;
    let ExecutionManagementApplication {
      builds,
      manual_triggers,
      job_events,
    } = execution;
    let AgentManagementApplication {
      pools: agent_pools,
      agents,
    } = agent_management;
    Ok(Self {
      inputs: ManagementInputFactory::new(
        supported_pipeline_capabilities,
        agent_enrollment_lifetime,
        agent_enrollment_secret_key,
      )?,
      projects,
      pipelines,
      configurations,
      agent_pools,
      agents,
      builds,
      definitions,
      manual_triggers,
      job_events,
    })
  }
}

/// Builds the registered management routes below `/api/v1`.
pub fn router(application: ManagementApplication) -> Router {
  Router::new()
    .route(&format!("{API_PREFIX}/openapi.json"), get(openapi))
    .route(
      &format!("{API_PREFIX}/projects"),
      post(create_project).get(list_projects),
    )
    .route(
      &format!("{API_PREFIX}/projects/{{project_id}}"),
      get(get_project).delete(delete_project),
    )
    .route(
      &format!("{API_PREFIX}/projects/{{project_id}}/rename"),
      post(rename_project),
    )
    .route(
      &format!("{API_PREFIX}/projects/{{project_id}}/move"),
      post(move_project),
    )
    .route(
      &format!("{API_PREFIX}/projects/{{project_id}}/policy-versions"),
      post(publish_project_policy),
    )
    .route(&format!("{API_PREFIX}/pipelines"), post(create_pipeline))
    .route(
      &format!("{API_PREFIX}/pipelines/{{pipeline_id}}/versions"),
      post(publish_pipeline),
    )
    .route(
      &format!("{API_PREFIX}/pipelines/{{pipeline_id}}/versions/{{version}}"),
      get(get_pipeline),
    )
    .route(&format!("{API_PREFIX}/repositories"), post(create_repository))
    .route(
      &format!("{API_PREFIX}/repositories/{{repository_id}}/versions"),
      post(publish_repository),
    )
    .route(
      &format!("{API_PREFIX}/repositories/{{repository_id}}/versions/{{version}}"),
      get(get_repository),
    )
    .route(
      &format!("{API_PREFIX}/build-configurations"),
      post(create_build_configuration),
    )
    .route(
      &format!("{API_PREFIX}/build-configurations/{{configuration_id}}/versions"),
      post(publish_build_configuration),
    )
    .route(
      &format!("{API_PREFIX}/build-configurations/{{configuration_id}}/versions/{{version}}"),
      get(get_build_configuration),
    )
    .route(
      &format!("{API_PREFIX}/trigger-definitions/manual"),
      post(create_manual_trigger_definition),
    )
    .route(&format!("{API_PREFIX}/triggers/manual"), post(accept_manual_trigger))
    .route(&format!("{API_PREFIX}/builds/{{build_id}}"), get(get_build))
    .route(&format!("{API_PREFIX}/builds/{{build_id}}/cancel"), post(cancel_build))
    .route(&format!("{API_PREFIX}/builds/{{build_id}}/retry"), post(retry_build))
    .route(&format!("{API_PREFIX}/attempts/{{attempt_id}}"), get(get_attempt))
    .route(
      &format!("{API_PREFIX}/agent-pools"),
      post(create_agent_pool).get(list_agent_pools),
    )
    .route(
      &format!("{API_PREFIX}/agent-pools/{{pool_id}}"),
      axum::routing::delete(delete_agent_pool),
    )
    .route(
      &format!("{API_PREFIX}/agent-pools/{{pool_id}}/versions"),
      post(publish_agent_pool),
    )
    .route(
      &format!("{API_PREFIX}/agent-pools/{{pool_id}}/versions/{{version}}"),
      get(get_agent_pool),
    )
    .route(&format!("{API_PREFIX}/agents"), get(list_agents))
    .route(&format!("{API_PREFIX}/agent-enrollments"), post(issue_agent_enrollment))
    .route(&format!("{API_PREFIX}/agents/{{agent_id}}"), get(get_agent))
    .route(
      &format!("{API_PREFIX}/agents/{{agent_id}}/pool"),
      post(reassign_agent_pool),
    )
    .route(&format!("{API_PREFIX}/agents/{{agent_id}}/drain"), post(drain_agent))
    .route(&format!("{API_PREFIX}/jobs/{{job_id}}"), get(get_job))
    .route(&format!("{API_PREFIX}/jobs/{{job_id}}/events"), get(read_job_events))
    .fallback(api_not_found)
    .layer(DefaultBodyLimit::max(MAX_MANAGEMENT_BODY_BYTES))
    .with_state(Arc::new(application))
}

async fn openapi() -> Json<Value> {
  Json(super::openapi_document())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectListParameters {
  parent_id: Option<String>,
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JobEventReadParameters {
  #[serde(default)]
  after: u64,
  #[serde(default = "default_job_event_limit")]
  limit: u16,
  #[serde(default)]
  wait_ms: u64,
}

async fn create_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<CreateProjectRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let idempotency_key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_project(
      Uuid::new_v4(),
      body.parent_id.as_deref(),
      body.name,
      idempotency_key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .projects
    .create
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::CREATED, Json(project_mutation(outcome))))
}

async fn rename_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<RenameProjectRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .rename_project(
      &project_id,
      expected.version(),
      body.name,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .projects
    .rename
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(project_mutation(outcome))))
}

async fn move_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<MoveProjectRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .move_project(
      &project_id,
      expected.version(),
      body.parent_id.as_deref(),
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .projects
    .move_project
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(project_mutation(outcome))))
}

async fn delete_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .delete_project(&project_id, expected.version(), key.as_str(), now_unix_ms(&request_id)?)
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .projects
    .delete
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(delete_project_response(outcome))))
}

async fn publish_project_policy(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<PublishProjectPolicyRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = optional_precondition(&headers, &request_id)?.map(VersionPrecondition::version);
  let command = application
    .inputs
    .publish_project_policy(
      &project_id,
      expected,
      body.policy,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .definitions
    .publish_project_policy
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((
    StatusCode::CREATED,
    Json(MutationResponse {
      disposition: mutation_disposition(outcome.disposition),
      resource: ProjectPolicyResource {
        project_id: outcome.project_id.to_string(),
        version: outcome.version.get(),
      },
    }),
  ))
}

async fn get_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_project(&project_id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .projects
    .get
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(project_details(projection))))
}

async fn list_projects(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  parameters: Result<Query<ProjectListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_projects(
      parameters.parent_id.as_deref(),
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .projects
    .list
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(project_page(page, &request_id)?)))
}

async fn create_pipeline(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<CreatePipelineRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_pipeline(
      Uuid::new_v4(),
      &body.project_id,
      body.name,
      pipeline_document(body.dag),
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .pipelines
    .create
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::CREATED, Json(pipeline_mutation(outcome))))
}

async fn publish_pipeline(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(pipeline_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<PublishPipelineVersionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .publish_pipeline(
      &pipeline_id,
      expected.version(),
      pipeline_document(body.dag),
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .pipelines
    .publish
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(pipeline_mutation(outcome))))
}

async fn get_pipeline(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path((pipeline_id, version)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_pipeline(&pipeline_id, version)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .pipelines
    .get
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(pipeline_resource(projection))))
}

async fn create_repository(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<CreateRepositoryRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_repository(
      Uuid::new_v4(),
      &body.project_id,
      body.name,
      encode(body.definition, &request_id)?,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .configurations
    .create_repository
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::CREATED, Json(repository_mutation(outcome))))
}

async fn publish_repository(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(repository_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<PublishRepositoryVersionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .publish_repository(
      &repository_id,
      expected.version(),
      encode(body.definition, &request_id)?,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .configurations
    .publish_repository
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(repository_mutation(outcome))))
}

async fn get_repository(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path((repository_id, version)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_repository(&repository_id, version)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .configurations
    .get_repository
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(repository_resource(projection))))
}

async fn create_build_configuration(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<CreateBuildConfigurationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_build_configuration(
      Uuid::new_v4(),
      &body.project_id,
      body.name,
      configuration_document(body.definition, &request_id)?,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .configurations
    .create_configuration
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::CREATED, Json(configuration_mutation(outcome))))
}

async fn publish_build_configuration(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(configuration_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<PublishBuildConfigurationVersionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .publish_build_configuration(
      &configuration_id,
      expected.version(),
      configuration_document(body.definition, &request_id)?,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .configurations
    .publish_configuration
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(configuration_mutation(outcome))))
}

async fn get_build_configuration(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path((configuration_id, version)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_build_configuration(&configuration_id, version)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .configurations
    .get_configuration
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(configuration_resource(projection))))
}

async fn accept_manual_trigger(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<AcceptManualTriggerRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  if key.as_str() != body.deduplication_identity {
    return Err(ApiError::new(
      StatusCode::CONFLICT,
      ErrorCode::IdempotencyConflict,
      "manual Trigger Idempotency-Key must equal deduplication_identity",
      &request_id,
    ));
  }
  let command = application
    .inputs
    .accept_manual_trigger(
      ManualTriggerInput {
        trigger_id: body.trigger_id,
        trigger_version: body.trigger_version,
        configuration_id: body.configuration_id,
        configuration_version: body.configuration_version,
        deduplication_identity: body.deduplication_identity,
        source: encode(body.source, &request_id)?,
        parameters: body.parameters,
        priority: body.priority,
      },
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .manual_triggers
    .0
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(trigger_response(outcome))))
}

async fn create_manual_trigger_definition(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<CreateManualTriggerDefinitionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_manual_trigger_definition(
      ManualTriggerDefinitionInput {
        id: Uuid::new_v4(),
        configuration_id: body.configuration_id,
        configuration_version: body.configuration_version,
        enabled: body.enabled,
        definition: body.definition,
      },
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .definitions
    .create_trigger
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((
    StatusCode::CREATED,
    Json(MutationResponse {
      disposition: mutation_disposition(outcome.disposition),
      resource: TriggerDefinitionResource {
        id: outcome.trigger_id.to_string(),
        version: outcome.version.get(),
      },
    }),
  ))
}

async fn read_job_events(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(job_id): Path<String>,
  parameters: Result<Query<JobEventReadParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .read_job_events(
      &job_id,
      parameters.after,
      parameters.limit,
      std::time::Duration::from_millis(parameters.wait_ms),
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .job_events
    .0
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(job_event_page(page))))
}

async fn api_not_found(Extension(request_id): Extension<RequestId>, OriginalUri(uri): OriginalUri) -> ApiError {
  let unsupported_version = uri
    .path()
    .strip_prefix("/api/")
    .and_then(|path| path.split('/').next())
    .is_some_and(|version| version != "v1");
  if unsupported_version {
    ApiError::new(
      StatusCode::NOT_FOUND,
      ErrorCode::UnsupportedApiVersion,
      "management API version is not supported",
      &request_id,
    )
  } else {
    ApiError::new(
      StatusCode::NOT_FOUND,
      ErrorCode::NotFound,
      "management route was not found",
      &request_id,
    )
  }
}

fn body<T>(payload: Result<Json<T>, JsonRejection>, request_id: &RequestId) -> Result<T, ApiError> {
  payload.map(|Json(value)| value).map_err(|error| {
    let (status, code) = match error.status() {
      StatusCode::UNSUPPORTED_MEDIA_TYPE => (StatusCode::UNSUPPORTED_MEDIA_TYPE, ErrorCode::UnsupportedMediaType),
      StatusCode::PAYLOAD_TOO_LARGE => (StatusCode::PAYLOAD_TOO_LARGE, ErrorCode::PayloadTooLarge),
      _ => (StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest),
    };
    ApiError::new(status, code, "request body is not valid for this operation", request_id)
  })
}

fn query_error(_error: QueryRejection, request_id: &RequestId) -> ApiError {
  ApiError::new(
    StatusCode::BAD_REQUEST,
    ErrorCode::InvalidRequest,
    "query parameters are invalid",
    request_id,
  )
}

fn idempotency_key(headers: &HeaderMap, request_id: &RequestId) -> Result<IdempotencyKey, ApiError> {
  headers
    .get(IDEMPOTENCY_KEY_HEADER)
    .ok_or_else(|| {
      ApiError::new(
        StatusCode::BAD_REQUEST,
        ErrorCode::InvalidIdempotencyKey,
        "Idempotency-Key is required",
        request_id,
      )
    })
    .and_then(|value| {
      IdempotencyKey::from_header(value).map_err(|_| {
        ApiError::new(
          StatusCode::BAD_REQUEST,
          ErrorCode::InvalidIdempotencyKey,
          "Idempotency-Key is invalid",
          request_id,
        )
      })
    })
}

fn precondition(headers: &HeaderMap, request_id: &RequestId) -> Result<VersionPrecondition, ApiError> {
  headers
    .get(OPTIMISTIC_PRECONDITION_HEADER)
    .ok_or_else(|| {
      ApiError::new(
        StatusCode::PRECONDITION_REQUIRED,
        ErrorCode::PreconditionRequired,
        "If-Match is required",
        request_id,
      )
    })
    .and_then(|value| {
      VersionPrecondition::from_header(value).map_err(|_| {
        ApiError::new(
          StatusCode::BAD_REQUEST,
          ErrorCode::InvalidRequest,
          "If-Match must contain one strong positive version",
          request_id,
        )
      })
    })
}

fn optional_precondition(headers: &HeaderMap, request_id: &RequestId) -> Result<Option<VersionPrecondition>, ApiError> {
  headers
    .get(OPTIMISTIC_PRECONDITION_HEADER)
    .map(|value| {
      VersionPrecondition::from_header(value).map_err(|_| {
        ApiError::new(
          StatusCode::BAD_REQUEST,
          ErrorCode::InvalidRequest,
          "If-Match must contain one strong positive version",
          request_id,
        )
      })
    })
    .transpose()
}

fn invalid_input(error: ManagementInputError, request_id: &RequestId) -> ApiError {
  ApiError::new(
    StatusCode::BAD_REQUEST,
    ErrorCode::InvalidRequest,
    &error.to_string(),
    request_id,
  )
}

fn application_error(failure: ApplicationFailure, request_id: &RequestId) -> ApiError {
  match failure {
    ApplicationFailure::Invalid => ApiError::new(
      StatusCode::BAD_REQUEST,
      ErrorCode::InvalidRequest,
      "application command is invalid",
      request_id,
    ),
    ApplicationFailure::NotFound => ApiError::new(
      StatusCode::NOT_FOUND,
      ErrorCode::NotFound,
      "requested resource was not found",
      request_id,
    ),
    ApplicationFailure::Conflict => ApiError::new(
      StatusCode::CONFLICT,
      ErrorCode::Conflict,
      "authoritative state conflicts with the request",
      request_id,
    ),
    ApplicationFailure::Unavailable => ApiError::new(
      StatusCode::SERVICE_UNAVAILABLE,
      ErrorCode::Unavailable,
      "a required server dependency is unavailable",
      request_id,
    ),
    ApplicationFailure::Internal => ApiError::new(
      StatusCode::INTERNAL_SERVER_ERROR,
      ErrorCode::Internal,
      "the server could not safely represent the result",
      request_id,
    ),
  }
}

fn default_page_limit() -> u16 {
  DEFAULT_PAGE_LIMIT
}

fn default_job_event_limit() -> u16 {
  DEFAULT_JOB_EVENT_LIMIT
}

fn now_unix_ms(request_id: &RequestId) -> Result<i64, ApiError> {
  let milliseconds = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|_| clock_error(request_id))?
    .as_millis();
  i64::try_from(milliseconds).map_err(|_| clock_error(request_id))
}

fn clock_error(request_id: &RequestId) -> ApiError {
  ApiError::new(
    StatusCode::INTERNAL_SERVER_ERROR,
    ErrorCode::Internal,
    "the server clock cannot represent the current time",
    request_id,
  )
}
