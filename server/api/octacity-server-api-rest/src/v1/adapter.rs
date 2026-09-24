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
  DeleteProjectCommandOutcome, DependencyPolicyProjection as ApplicationDependencyPolicy,
  InternalTriggerDefinitionInput, InternalTriggerPageProjection, InternalTriggerProjection,
  InternalTriggerSourceStrategy as ApplicationInternalTriggerSource, JobEventPageProjection, JobProjection,
  ManagementInputError, ManagementInputFactory, ManualTriggerDefinitionInput, ManualTriggerInput, ManualTriggerOutcome,
  MutationDisposition as ApplicationMutationDisposition, NetworkPolicyProjection as ApplicationNetworkPolicy,
  ParameterTypeProjection as ApplicationParameterType, PipelineCommandOutcome, PipelineProjection,
  PlatformArchitectureProjection as ApplicationPlatformArchitecture, PlatformOsProjection as ApplicationPlatformOs,
  ProjectCommandOutcome, ProjectPageProjection, ProjectProjection, RepositoryCommandOutcome, RepositoryProjection,
  RetryBuildCommandOutcome, RetryClassProjection as ApplicationRetryClass,
  RuntimeClassProjection as ApplicationRuntimeClass, ScheduleProjection, ScheduledTriggerDefinitionInput,
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
  CreateRepositoryRequest, CreateScheduledTriggerDefinitionRequest, Cursor, CursorPage, DagEdgeResource,
  DeleteAgentPoolResponse, DeleteProjectResponse, DependencyPolicy, DrainAgentRequest, ErrorCode,
  IDEMPOTENCY_KEY_HEADER, IdempotencyKey, InternalTriggerDefinitionRequest, InternalTriggerOutcome,
  InternalTriggerResource, InternalTriggerSource, IssueAgentEnrollmentRequest, IssueAgentEnrollmentResponse,
  JobAssignmentResource, JobEventPage, JobEventResource, JobExecution, JobQueueResource, JobResource,
  JobTerminalResource, ManualSource, MoveProjectRequest, MutationDisposition, MutationResponse, NetworkPolicy,
  OPTIMISTIC_PRECONDITION_HEADER, ParameterDefinition, ParameterSchema, ParameterType, PipelineDag, PipelineEdge,
  PipelineNode, PipelineResource, PlatformArchitecture, PlatformOs, ProjectDetails, ProjectPolicyResource,
  ProjectResource, PublishAgentPoolVersionRequest, PublishBuildConfigurationVersionRequest,
  PublishPipelineVersionRequest, PublishProjectPolicyRequest, PublishRepositoryVersionRequest,
  ReassignAgentPoolRequest, RenameProjectRequest, RepositoryDefinition, RepositoryResource, RepositorySelectionPolicy,
  RetryBuildResponse, RetryClass, RetryPolicy, RuntimeClass, RuntimePolicy, ScheduleResource,
  TriggerDefinitionResource, TriggerEvaluationResponse, TriggerKind, VersionPrecondition,
};
use crate::RequestId;

mod agent;
mod agent_pool;
mod application;
mod artifact;
mod cache;
mod configuration;
mod error;
mod execution;
mod internal_trigger;
mod job_event;
mod manual_trigger;
mod pipeline;
mod project;
mod representations;
mod schedule;
mod webhook;

pub(crate) use artifact::DEFAULT_ARTIFACT_LIMIT;

use agent::{drain_agent, get_agent, issue_agent_enrollment, list_agents, reassign_agent_pool};
use agent_pool::{create_agent_pool, delete_agent_pool, get_agent_pool, list_agent_pools, publish_agent_pool};
use application::{AgentEndpoints, AgentPoolManagementApplication};
pub use application::{
  AgentManagementApplication, ArtifactManagementApplication, BuildManagementApplication, CacheManagementApplication,
  CatalogManagementApplication, ConfigurationManagementApplication, DefinitionManagementApplication,
  ExecutionManagementApplication, InternalTriggerManagementApplication, JobEventManagementApplication,
  ManagementApplicationHandlers, ManualTriggerManagementApplication, PipelineManagementApplication,
  ProjectManagementApplication, ScheduleManagementApplication,
};
use artifact::{authorize_artifact_download, get_artifact, list_build_artifacts};
use cache::{get_cache_session, list_build_cache_sessions};
use configuration::{
  create_build_configuration, create_repository, get_build_configuration, get_repository, publish_build_configuration,
  publish_repository,
};
use error::ApiError;
use execution::{cancel_build, get_attempt, get_build, get_job, retry_build};
use internal_trigger::{
  create_internal_trigger, get_internal_trigger, list_internal_triggers, publish_internal_trigger,
};
use job_event::read_job_events;
use manual_trigger::{accept_manual_trigger, create_manual_trigger_definition};
use pipeline::{create_pipeline, get_pipeline, publish_pipeline};
use project::{
  create_project, delete_project, get_project, list_projects, move_project, publish_project_policy, rename_project,
};
use representations::*;
use schedule::{create_scheduled_trigger_definition, get_schedule};
use webhook::{
  create_managed_webhook, create_unmanaged_webhook, delete_managed_webhook, observe_managed_webhook,
  rotate_managed_webhook,
};

const MAX_MANAGEMENT_BODY_BYTES: usize = 8 * 1024 * 1024;
pub(super) const DEFAULT_PAGE_LIMIT: u16 = 50;
pub(super) const DEFAULT_JOB_EVENT_LIMIT: u16 = 100;
pub(super) const DEFAULT_CACHE_SESSION_LIMIT: u16 = 50;

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
  schedules: ScheduleManagementApplication,
  internal_triggers: InternalTriggerManagementApplication,
  manual_triggers: ManualTriggerManagementApplication,
  job_events: JobEventManagementApplication,
  artifacts: ArtifactManagementApplication,
  cache: CacheManagementApplication,
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
      schedules,
      internal_triggers,
    } = catalog;
    let ExecutionManagementApplication {
      builds,
      manual_triggers,
      job_events,
      artifacts,
      cache,
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
      schedules,
      internal_triggers,
      manual_triggers,
      job_events,
      artifacts,
      cache,
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
    .route(
      &format!("{API_PREFIX}/trigger-definitions/scheduled"),
      post(create_scheduled_trigger_definition),
    )
    .route(
      &format!("{API_PREFIX}/trigger-definitions/internal"),
      post(create_internal_trigger).get(list_internal_triggers),
    )
    .route(
      &format!("{API_PREFIX}/trigger-definitions/internal/{{trigger_id}}/versions"),
      post(publish_internal_trigger),
    )
    .route(
      &format!("{API_PREFIX}/trigger-definitions/internal/{{trigger_id}}/versions/{{version}}"),
      get(get_internal_trigger),
    )
    .route(
      &format!("{API_PREFIX}/webhook-integrations/unmanaged"),
      post(create_unmanaged_webhook),
    )
    .route(
      &format!("{API_PREFIX}/webhook-integrations/managed"),
      post(create_managed_webhook),
    )
    .route(
      &format!("{API_PREFIX}/webhook-integrations/managed/{{integration_id}}/observe"),
      post(observe_managed_webhook),
    )
    .route(
      &format!("{API_PREFIX}/webhook-integrations/managed/{{integration_id}}/rotate"),
      post(rotate_managed_webhook),
    )
    .route(
      &format!("{API_PREFIX}/webhook-integrations/managed/{{integration_id}}"),
      axum::routing::delete(delete_managed_webhook),
    )
    .route(
      &format!("{API_PREFIX}/schedules/{{trigger_id}}/versions/{{version}}"),
      get(get_schedule),
    )
    .route(&format!("{API_PREFIX}/triggers/manual"), post(accept_manual_trigger))
    .route(&format!("{API_PREFIX}/builds/{{build_id}}"), get(get_build))
    .route(
      &format!("{API_PREFIX}/builds/{{build_id}}/artifacts"),
      get(list_build_artifacts),
    )
    .route(&format!("{API_PREFIX}/artifacts/{{artifact_id}}"), get(get_artifact))
    .route(
      &format!("{API_PREFIX}/artifacts/{{artifact_id}}/download"),
      post(authorize_artifact_download),
    )
    .route(
      &format!("{API_PREFIX}/builds/{{build_id}}/cache-sessions"),
      get(list_build_cache_sessions),
    )
    .route(
      &format!("{API_PREFIX}/cache-sessions/{{cache_session_id}}"),
      get(get_cache_session),
    )
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
    ApplicationFailure::CapabilityUnavailable => ApiError::new(
      StatusCode::UNPROCESSABLE_ENTITY,
      ErrorCode::CapabilityUnavailable,
      "the selected adapter does not support this operation",
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
