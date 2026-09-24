use serde_json::{Map, Value, json};

use octacity_server_application::{
  MAX_AGENT_LIST_PAGE_SIZE, MAX_AGENT_POOL_LIST_PAGE_SIZE, MAX_ARTIFACT_LIST_PAGE_SIZE,
  MAX_INTERNAL_TRIGGER_LIST_PAGE_SIZE, MAX_JOB_EVENT_PAGE_SIZE, MAX_JOB_EVENT_WAIT, MAX_PROJECT_LIST_PAGE_SIZE,
  MAX_SCHEDULE_CATCH_UP, MAX_SCHEDULE_EXPRESSION_BYTES, MAX_SCHEDULE_TIMEZONE_BYTES, MAX_WEBHOOK_VERIFICATION_HEADERS,
};

use super::{API_PREFIX, MAX_CURSOR_BYTES, MAX_IDEMPOTENCY_KEY_BYTES};

/// One management operation in the versioned REST contract.
///
/// The router and OpenAPI drift test use this registry as the stable inventory
/// that later feature tasks extend when they add complete application use cases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManagementOperation {
  /// Uppercase HTTP method.
  pub method: &'static str,
  /// Templated OpenAPI path.
  pub path: &'static str,
  /// Stable OpenAPI operation identifier.
  pub operation_id: &'static str,
  /// OpenAPI tag grouping the operation.
  pub tag: &'static str,
  /// Short operator-facing operation summary.
  pub summary: &'static str,
  /// Request component schema, when the operation accepts JSON.
  pub request_schema: Option<&'static str>,
  /// Successful response component schema.
  pub response_schema: &'static str,
  /// Successful HTTP status code.
  pub success_status: &'static str,
  /// Whether the operation requires `Idempotency-Key`.
  pub idempotent_mutation: bool,
  /// Whether the operation requires an optimistic `If-Match` precondition.
  pub optimistic_precondition: bool,
  parameter_profile: ParameterProfile,
  capability_unavailable_response: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ParameterProfile {
  #[default]
  None,
  ProjectList,
  AgentPoolList,
  AgentList,
  JobEvents,
  ArtifactList,
  CacheSessionList,
  InternalTriggerList,
  BuildLogSearch,
}

impl ManagementOperation {
  const fn with_parameters(mut self, profile: ParameterProfile) -> Self {
    self.parameter_profile = profile;
    self
  }

  const fn with_capability_unavailable_response(mut self) -> Self {
    self.capability_unavailable_response = true;
    self
  }
}

macro_rules! operation {
  ($method:literal, $path:literal, $id:literal, $tag:literal, $summary:literal, $request:expr, $response:literal, $status:literal, $mutation:literal, $precondition:literal) => {
    ManagementOperation {
      method: $method,
      path: $path,
      operation_id: $id,
      tag: $tag,
      summary: $summary,
      request_schema: $request,
      response_schema: $response,
      success_status: $status,
      idempotent_mutation: $mutation,
      optimistic_precondition: $precondition,
      parameter_profile: ParameterProfile::None,
      capability_unavailable_response: false,
    }
  };
}

/// Complete inventory of registered management operations.
pub const MANAGEMENT_OPERATIONS: &[ManagementOperation] = &[
  operation!(
    "GET",
    "/api/v1/operations/metadata",
    "getOperationalMetadata",
    "Operations",
    "Get deployment security and capability status",
    None,
    "OperationalMetadata",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/projects",
    "createProject",
    "Projects",
    "Create a Project",
    Some("CreateProjectRequest"),
    "ProjectMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "GET",
    "/api/v1/projects",
    "listProjects",
    "Projects",
    "List Projects",
    None,
    "ProjectPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::ProjectList),
  operation!(
    "GET",
    "/api/v1/projects/{project_id}",
    "getProject",
    "Projects",
    "Get a Project",
    None,
    "ProjectDetails",
    "200",
    false,
    false
  ),
  operation!(
    "DELETE",
    "/api/v1/projects/{project_id}",
    "deleteProject",
    "Projects",
    "Delete a Project",
    None,
    "DeleteProjectResponse",
    "200",
    true,
    true
  ),
  operation!(
    "POST",
    "/api/v1/projects/{project_id}/rename",
    "renameProject",
    "Projects",
    "Rename a Project",
    Some("RenameProjectRequest"),
    "ProjectMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "POST",
    "/api/v1/projects/{project_id}/move",
    "moveProject",
    "Projects",
    "Move a Project",
    Some("MoveProjectRequest"),
    "ProjectMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "POST",
    "/api/v1/projects/{project_id}/policy-versions",
    "publishProjectPolicyVersion",
    "Projects",
    "Publish a Project policy version",
    Some("PublishProjectPolicyRequest"),
    "ProjectPolicyMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/pipelines",
    "createPipeline",
    "Pipelines",
    "Create a Pipeline",
    Some("CreatePipelineRequest"),
    "PipelineMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/pipelines/{pipeline_id}/versions",
    "publishPipelineVersion",
    "Pipelines",
    "Publish a Pipeline version",
    Some("PublishPipelineVersionRequest"),
    "PipelineMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "GET",
    "/api/v1/pipelines/{pipeline_id}/versions/{version}",
    "getPipelineVersion",
    "Pipelines",
    "Get a Pipeline version",
    None,
    "PipelineResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/repositories",
    "createRepository",
    "Repositories",
    "Create a Repository",
    Some("CreateRepositoryRequest"),
    "RepositoryMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/repositories/{repository_id}/versions",
    "publishRepositoryVersion",
    "Repositories",
    "Publish a Repository version",
    Some("PublishRepositoryVersionRequest"),
    "RepositoryMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "GET",
    "/api/v1/repositories/{repository_id}/versions/{version}",
    "getRepositoryVersion",
    "Repositories",
    "Get a Repository version",
    None,
    "RepositoryResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/build-configurations",
    "createBuildConfiguration",
    "Build Configurations",
    "Create a Build Configuration",
    Some("CreateBuildConfigurationRequest"),
    "BuildConfigurationMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/build-configurations/{configuration_id}/versions",
    "publishBuildConfigurationVersion",
    "Build Configurations",
    "Publish a Build Configuration version",
    Some("PublishBuildConfigurationVersionRequest"),
    "BuildConfigurationMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "GET",
    "/api/v1/build-configurations/{configuration_id}/versions/{version}",
    "getBuildConfigurationVersion",
    "Build Configurations",
    "Get a Build Configuration version",
    None,
    "BuildConfigurationResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/trigger-definitions/manual",
    "createManualTriggerDefinition",
    "Triggers",
    "Create a manual Trigger definition",
    Some("CreateManualTriggerDefinitionRequest"),
    "TriggerDefinitionMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/trigger-definitions/scheduled",
    "createScheduledTriggerDefinition",
    "Triggers",
    "Create a durable scheduled Trigger definition",
    Some("CreateScheduledTriggerDefinitionRequest"),
    "TriggerDefinitionMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/trigger-definitions/internal",
    "createInternalTriggerDefinition",
    "Triggers",
    "Create a source-scoped internal Trigger",
    Some("InternalTriggerDefinitionRequest"),
    "InternalTriggerMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "GET",
    "/api/v1/trigger-definitions/internal",
    "listInternalTriggerDefinitions",
    "Triggers",
    "List current internal Trigger versions",
    None,
    "InternalTriggerPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::InternalTriggerList),
  operation!(
    "POST",
    "/api/v1/trigger-definitions/internal/{trigger_id}/versions",
    "publishInternalTriggerDefinitionVersion",
    "Triggers",
    "Publish an internal Trigger version",
    Some("InternalTriggerDefinitionRequest"),
    "InternalTriggerMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "GET",
    "/api/v1/trigger-definitions/internal/{trigger_id}/versions/{version}",
    "getInternalTriggerDefinitionVersion",
    "Triggers",
    "Get an internal Trigger version",
    None,
    "InternalTriggerResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/webhook-integrations/unmanaged",
    "createUnmanagedWebhookIntegration",
    "Webhook Integrations",
    "Create an unmanaged webhook integration",
    Some("CreateUnmanagedWebhookRequest"),
    "UnmanagedWebhookResource",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/webhook-integrations/managed",
    "createManagedWebhookIntegration",
    "Webhook Integrations",
    "Create a provider-managed webhook integration",
    Some("CreateManagedWebhookRequest"),
    "ManagedWebhookResource",
    "201",
    true,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    "POST",
    "/api/v1/webhook-integrations/managed/{integration_id}/observe",
    "observeManagedWebhookIntegration",
    "Webhook Integrations",
    "Observe a managed remote webhook registration",
    None,
    "ManagedWebhookResource",
    "200",
    true,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    "POST",
    "/api/v1/webhook-integrations/managed/{integration_id}/rotate",
    "rotateManagedWebhookIntegration",
    "Webhook Integrations",
    "Rotate a managed remote webhook registration",
    None,
    "ManagedWebhookResource",
    "200",
    true,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    "DELETE",
    "/api/v1/webhook-integrations/managed/{integration_id}",
    "deleteManagedWebhookIntegration",
    "Webhook Integrations",
    "Delete a managed remote webhook registration",
    None,
    "ManagedWebhookResource",
    "200",
    true,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    "GET",
    "/api/v1/schedules/{trigger_id}/versions/{version}",
    "getSchedule",
    "Triggers",
    "Get a durable schedule",
    None,
    "ScheduleResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/triggers/manual",
    "acceptManualTrigger",
    "Triggers",
    "Accept a manual Trigger",
    Some("AcceptManualTriggerRequest"),
    "TriggerEvaluationResponse",
    "200",
    true,
    false
  ),
  operation!(
    "GET",
    "/api/v1/builds/{build_id}",
    "getBuild",
    "Builds",
    "Get a Build and its current Attempt",
    None,
    "BuildResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/builds/{build_id}/cancel",
    "cancelBuild",
    "Builds",
    "Cancel an active Build",
    None,
    "CancelBuildResponse",
    "200",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/builds/{build_id}/retry",
    "retryBuild",
    "Builds",
    "Retry a failed Build",
    None,
    "RetryBuildResponse",
    "201",
    true,
    false
  ),
  operation!(
    "GET",
    "/api/v1/attempts/{attempt_id}",
    "getAttempt",
    "Builds",
    "Get an Attempt diagnostic DAG",
    None,
    "AttemptResource",
    "200",
    false,
    false
  ),
  operation!(
    "GET",
    "/api/v1/jobs/{job_id}",
    "getJob",
    "Jobs",
    "Get Job execution diagnostics",
    None,
    "JobResource",
    "200",
    false,
    false
  ),
  operation!(
    "GET",
    "/api/v1/jobs/{job_id}/events",
    "readJobEvents",
    "Jobs",
    "Read or follow ordered Job events",
    None,
    "JobEventPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::JobEvents),
  operation!(
    "GET",
    "/api/v1/projects/{project_id}/build-logs/search",
    "searchBuildLogs",
    "Build Logs",
    "Search redacted committed Build logs",
    None,
    "BuildLogSearchPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::BuildLogSearch),
  operation!(
    "POST",
    "/api/v1/agent-pools",
    "createAgentPool",
    "Agent Pools",
    "Create a static Agent Pool",
    Some("CreateAgentPoolRequest"),
    "AgentPoolMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "GET",
    "/api/v1/agent-pools",
    "listAgentPools",
    "Agent Pools",
    "List current Agent Pool versions",
    None,
    "AgentPoolPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::AgentPoolList),
  operation!(
    "POST",
    "/api/v1/agent-pools/{pool_id}/versions",
    "publishAgentPoolVersion",
    "Agent Pools",
    "Publish an Agent Pool version",
    Some("PublishAgentPoolVersionRequest"),
    "AgentPoolMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "GET",
    "/api/v1/agent-pools/{pool_id}/versions/{version}",
    "getAgentPoolVersion",
    "Agent Pools",
    "Get an Agent Pool version",
    None,
    "AgentPoolResource",
    "200",
    false,
    false
  ),
  operation!(
    "DELETE",
    "/api/v1/agent-pools/{pool_id}",
    "deleteAgentPool",
    "Agent Pools",
    "Delete an unreferenced Agent Pool",
    None,
    "DeleteAgentPoolResponse",
    "200",
    true,
    true
  ),
  operation!(
    "POST",
    "/api/v1/agent-enrollments",
    "issueAgentEnrollment",
    "Agents",
    "Issue a single-use Agent enrollment credential",
    Some("IssueAgentEnrollmentRequest"),
    "IssueAgentEnrollmentResponse",
    "201",
    true,
    false
  ),
  operation!(
    "GET",
    "/api/v1/agents",
    "listAgents",
    "Agents",
    "List enrolled Agents",
    None,
    "AgentPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::AgentList),
  operation!(
    "GET",
    "/api/v1/agents/{agent_id}",
    "getAgent",
    "Agents",
    "Get an enrolled Agent",
    None,
    "AgentResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/agents/{agent_id}/pool",
    "reassignAgentPool",
    "Agents",
    "Move an idle Agent to another Pool",
    Some("ReassignAgentPoolRequest"),
    "AgentMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "POST",
    "/api/v1/agents/{agent_id}/drain",
    "drainAgent",
    "Agents",
    "Drain an Agent and its current Lease",
    Some("DrainAgentRequest"),
    "AgentMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "GET",
    "/api/v1/builds/{build_id}/artifacts",
    "listBuildArtifacts",
    "Artifacts",
    "List published logical outputs for a Build",
    None,
    "ArtifactPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::ArtifactList),
  operation!(
    "GET",
    "/api/v1/artifacts/{artifact_id}",
    "getArtifact",
    "Artifacts",
    "Get published logical output metadata",
    None,
    "ArtifactResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/artifacts/{artifact_id}/download",
    "authorizeArtifactDownload",
    "Artifacts",
    "Create a short-lived Artifact download capability",
    None,
    "ArtifactDownload",
    "200",
    false,
    false
  ),
  operation!(
    "GET",
    "/api/v1/builds/{build_id}/cache-sessions",
    "listBuildCacheSessions",
    "Cache",
    "List secret-free cache-session diagnostics for a Build",
    None,
    "CacheSessionPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::CacheSessionList),
  operation!(
    "GET",
    "/api/v1/cache-sessions/{cache_session_id}",
    "getCacheSession",
    "Cache",
    "Get secret-free cache-session diagnostics",
    None,
    "CacheSessionResource",
    "200",
    false,
    false
  ),
];

/// Builds the OpenAPI 3.1 document served by the running v1 adapter.
#[must_use]
pub fn openapi_document() -> Value {
  let mut paths = Map::new();
  paths.insert(
    format!("{API_PREFIX}/openapi.json"),
    json!({
      "get": {
        "operationId": "getOpenApiDocument",
        "summary": "Get this OpenAPI document",
        "tags": ["Discovery"],
        "security": [],
        "responses": {
          "200": {
            "description": "OpenAPI 3.1 document",
            "content": {"application/json": {"schema": {"type": "object"}}}
          }
        }
      }
    }),
  );
  for operation in MANAGEMENT_OPERATIONS {
    let path = paths
      .entry(operation.path.to_owned())
      .or_insert_with(|| Value::Object(Map::new()));
    path
      .as_object_mut()
      .expect("OpenAPI path item is an object")
      .insert(operation.method.to_ascii_lowercase(), operation_document(operation));
  }

  json!({
    "openapi": "3.1.0",
    "info": {
      "title": "OctaCity Management API",
      "version": "1.0.0",
      "description": "Trusted-network management API. This deployment mode intentionally performs no operator authentication; network isolation is required. Agent and webhook authentication are separate contracts."
    },
    "servers": [{"url": "/", "description": "Current OctaCity server"}],
    "security": [],
    "x-octacity-management-security": {
      "mode": "trusted_network_unauthenticated",
      "operator_authentication": false,
      "network_isolation_required": true,
      "agent_and_webhook_authentication_unchanged": true
    },
    "paths": paths,
    "components": {
      "schemas": component_schemas(),
      "responses": {
        "ManagementError": {
          "description": "Stable management API error",
          "content": {
            "application/json": {"schema": {"$ref": "#/components/schemas/ErrorResponse"}}
          }
        }
      }
    }
  })
}

fn operation_document(operation: &ManagementOperation) -> Value {
  let mut document = Map::from_iter([
    ("operationId".to_owned(), json!(operation.operation_id)),
    ("summary".to_owned(), json!(operation.summary)),
    ("tags".to_owned(), json!([operation.tag])),
    ("security".to_owned(), json!([])),
    ("responses".to_owned(), responses(operation)),
  ]);
  let parameters = parameters(operation);
  if !parameters.is_empty() {
    document.insert("parameters".to_owned(), Value::Array(parameters));
  }
  if let Some(schema) = operation.request_schema {
    document.insert(
      "requestBody".to_owned(),
      json!({
        "required": true,
        "content": {"application/json": {"schema": schema_ref(schema)}}
      }),
    );
  }
  Value::Object(document)
}

fn responses(operation: &ManagementOperation) -> Value {
  let mut responses = Map::new();
  responses.insert(
    operation.success_status.to_owned(),
    json!({
      "description": "Successful operation",
      "content": {"application/json": {"schema": schema_ref(operation.response_schema)}}
    }),
  );
  for (status, description) in [
    ("400", "Invalid request"),
    ("404", "Resource not found"),
    ("409", "Durable state conflict"),
    ("500", "Internal server error"),
    ("503", "Required dependency unavailable"),
  ] {
    responses.insert(status.to_owned(), error_response(description));
  }
  if operation.request_schema.is_some() {
    responses.insert("413".to_owned(), error_response("Request body too large"));
    responses.insert("415".to_owned(), error_response("Unsupported request media type"));
  }
  if operation.optimistic_precondition {
    responses.insert("428".to_owned(), error_response("Optimistic precondition required"));
  }
  if operation.capability_unavailable_response {
    responses.insert(
      "422".to_owned(),
      error_response("Selected adapter capability unavailable"),
    );
  }
  Value::Object(responses)
}

fn error_response(description: &str) -> Value {
  json!({"description": description, "$ref": "#/components/responses/ManagementError"})
}

mod parameters;
mod schema;

use parameters::parameters;
use schema::component_schemas;

fn versioned_resource(definition_schema: &str) -> Value {
  object(
    [
      ("id", non_empty_string()),
      ("project_id", non_empty_string()),
      ("name", non_empty_string()),
      ("version", positive_integer()),
      ("definition", schema_ref(definition_schema)),
      ("published_at_unix_ms", integer()),
    ],
    &[
      "id",
      "project_id",
      "name",
      "version",
      "definition",
      "published_at_unix_ms",
    ],
  )
}

fn mutation_response(resource_schema: &str) -> Value {
  object(
    [
      ("disposition", schema_ref("MutationDisposition")),
      ("resource", schema_ref(resource_schema)),
    ],
    &["disposition", "resource"],
  )
}

fn object<const N: usize>(properties: [(&str, Value); N], required: &[&str]) -> Value {
  let properties = Map::from_iter(properties.into_iter().map(|(name, schema)| (name.to_owned(), schema)));
  json!({
    "type": "object",
    "properties": properties,
    "required": required,
    "additionalProperties": false
  })
}

fn schema_ref(name: &str) -> Value {
  json!({"$ref": format!("#/components/schemas/{name}")})
}

fn nullable(schema: Value) -> Value {
  json!({"anyOf": [schema, {"type": "null"}]})
}

fn string_enum(values: &[&str]) -> Value {
  json!({"type": "string", "enum": values})
}

fn non_empty_string() -> Value {
  json!({"type": "string", "minLength": 1})
}

fn integer() -> Value {
  json!({"type": "integer", "format": "int64"})
}

fn non_negative_integer() -> Value {
  json!({"type": "integer", "minimum": 0})
}

fn positive_integer() -> Value {
  json!({"type": "integer", "minimum": 1})
}

fn boolean() -> Value {
  json!({"type": "boolean"})
}

fn array(items: Value) -> Value {
  json!({"type": "array", "items": items})
}

fn unique_array(items: Value) -> Value {
  json!({"type": "array", "items": items, "uniqueItems": true})
}

fn string_map() -> Value {
  json!({"type": "object", "additionalProperties": {"type": "string"}})
}
