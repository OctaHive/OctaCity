use serde_json::{Map, Value, json};

use octacity_server_application::{MAX_JOB_EVENT_PAGE_SIZE, MAX_JOB_EVENT_WAIT, MAX_PROJECT_LIST_PAGE_SIZE};

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
    }
  };
}

/// Complete inventory of management operations registered by section 4.
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
  ),
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
    "/api/v1/jobs/{job_id}/events",
    "readJobEvents",
    "Jobs",
    "Read or follow ordered Job events",
    None,
    "JobEventPage",
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

fn parameters(operation: &ManagementOperation) -> Vec<Value> {
  let mut parameters = Vec::new();
  for name in [
    "project_id",
    "pipeline_id",
    "repository_id",
    "configuration_id",
    "job_id",
    "version",
  ] {
    if operation.path.contains(&format!("{{{name}}}")) {
      parameters.push(json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": if name == "version" { positive_integer() } else { non_empty_string() }
      }));
    }
  }
  if operation.operation_id == "listProjects" {
    parameters.extend([
      json!({"name": "parent_id", "in": "query", "required": false, "schema": non_empty_string()}),
      json!({
        "name": "after",
        "in": "query",
        "required": false,
        "schema": {"type": "string", "minLength": 1, "maxLength": MAX_CURSOR_BYTES}
      }),
      json!({
        "name": "limit",
        "in": "query",
        "required": false,
        "schema": {
          "type": "integer",
          "minimum": 1,
          "maximum": MAX_PROJECT_LIST_PAGE_SIZE,
          "default": super::adapter::DEFAULT_PAGE_LIMIT
        }
      }),
    ]);
  }
  if operation.operation_id == "readJobEvents" {
    parameters.extend([
      json!({
        "name": "after",
        "in": "query",
        "required": false,
        "schema": {"type": "integer", "format": "uint64", "minimum": 0, "default": 0}
      }),
      json!({
        "name": "limit",
        "in": "query",
        "required": false,
        "schema": {
          "type": "integer",
          "minimum": 1,
          "maximum": MAX_JOB_EVENT_PAGE_SIZE,
          "default": super::adapter::DEFAULT_JOB_EVENT_LIMIT
        }
      }),
      json!({
        "name": "wait_ms",
        "in": "query",
        "required": false,
        "schema": {
          "type": "integer",
          "format": "uint64",
          "minimum": 0,
          "maximum": MAX_JOB_EVENT_WAIT.as_millis(),
          "default": 0
        }
      }),
    ]);
  }
  if operation.idempotent_mutation {
    parameters.push(json!({
      "name": "Idempotency-Key",
      "in": "header",
      "required": true,
      "schema": {"type": "string", "minLength": 1, "maxLength": MAX_IDEMPOTENCY_KEY_BYTES}
    }));
  }
  if operation.optimistic_precondition {
    parameters.push(json!({
      "name": "If-Match",
      "in": "header",
      "required": true,
      "description": "Canonical strong ETag carrying a positive resource version, for example \"7\".",
      "schema": {"type": "string", "pattern": "^\\\"[1-9][0-9]*\\\"$"}
    }));
  }
  parameters
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
  Value::Object(responses)
}

fn error_response(description: &str) -> Value {
  json!({"description": description, "$ref": "#/components/responses/ManagementError"})
}

mod schema;

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
