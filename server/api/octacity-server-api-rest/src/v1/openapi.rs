use serde_json::{Map, Value, json};

use octacity_server_application::{
  MAX_AGENT_LIST_PAGE_SIZE, MAX_AGENT_POOL_LIST_PAGE_SIZE, MAX_ARTIFACT_LIST_PAGE_SIZE,
  MAX_INTERNAL_TRIGGER_LIST_PAGE_SIZE, MAX_JOB_EVENT_PAGE_SIZE, MAX_JOB_EVENT_WAIT, MAX_PROJECT_LIST_PAGE_SIZE,
  MAX_SCHEDULE_CATCH_UP, MAX_SCHEDULE_EXPRESSION_BYTES, MAX_SCHEDULE_TIMEZONE_BYTES, MAX_WEBHOOK_VERIFICATION_HEADERS,
};

use super::{API_PREFIX, MAX_CURSOR_BYTES, MAX_IDEMPOTENCY_KEY_BYTES};

pub(super) use operations::ParameterProfile;
pub use operations::{MANAGEMENT_OPERATIONS, ManagementOperation};

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
  if operation.precondition_failed_response {
    responses.insert("412".to_owned(), error_response("Optimistic precondition failed"));
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

mod operations;
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
