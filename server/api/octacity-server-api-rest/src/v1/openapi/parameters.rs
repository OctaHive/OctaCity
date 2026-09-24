use serde_json::{Value, json};

use super::*;

pub(super) fn parameters(operation: &ManagementOperation) -> Vec<Value> {
  let mut parameters = Vec::new();
  for name in [
    "project_id",
    "pipeline_id",
    "repository_id",
    "configuration_id",
    "build_id",
    "attempt_id",
    "job_id",
    "artifact_id",
    "cache_session_id",
    "pool_id",
    "agent_id",
    "integration_id",
    "trigger_id",
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
  match operation.parameter_profile {
    ParameterProfile::None => {}
    ParameterProfile::ProjectList => parameters.extend([
      json!({"name": "parent_id", "in": "query", "required": false, "schema": non_empty_string()}),
      cursor_parameter(),
      limit_parameter(MAX_PROJECT_LIST_PAGE_SIZE, super::super::adapter::DEFAULT_PAGE_LIMIT),
    ]),
    ParameterProfile::AgentPoolList => parameters.extend([
      cursor_parameter(),
      limit_parameter(MAX_AGENT_POOL_LIST_PAGE_SIZE, super::super::adapter::DEFAULT_PAGE_LIMIT),
    ]),
    ParameterProfile::AgentList => parameters.extend([
      cursor_parameter(),
      limit_parameter(MAX_AGENT_LIST_PAGE_SIZE, super::super::adapter::DEFAULT_PAGE_LIMIT),
    ]),
    ParameterProfile::JobEvents => parameters.extend([
      json!({
        "name": "after",
        "in": "query",
        "required": false,
        "schema": {"type": "integer", "format": "uint64", "minimum": 0, "default": 0}
      }),
      limit_parameter(MAX_JOB_EVENT_PAGE_SIZE, super::super::adapter::DEFAULT_JOB_EVENT_LIMIT),
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
    ]),
    ParameterProfile::ArtifactList => parameters.push(limit_parameter(
      MAX_ARTIFACT_LIST_PAGE_SIZE,
      super::super::adapter::DEFAULT_ARTIFACT_LIMIT,
    )),
    ParameterProfile::CacheSessionList => parameters.push(limit_parameter(
      octacity_server_application::MAX_CACHE_SESSION_LIST_PAGE_SIZE,
      super::super::adapter::DEFAULT_CACHE_SESSION_LIMIT,
    )),
    ParameterProfile::InternalTriggerList => parameters.extend([
      cursor_parameter(),
      limit_parameter(
        MAX_INTERNAL_TRIGGER_LIST_PAGE_SIZE,
        super::super::adapter::DEFAULT_PAGE_LIMIT,
      ),
    ]),
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

fn cursor_parameter() -> Value {
  json!({
    "name": "after",
    "in": "query",
    "required": false,
    "schema": {"type": "string", "minLength": 1, "maxLength": MAX_CURSOR_BYTES}
  })
}

fn limit_parameter(maximum: u16, default: u16) -> Value {
  json!({
    "name": "limit",
    "in": "query",
    "required": false,
    "schema": {
      "type": "integer",
      "minimum": 1,
      "maximum": maximum,
      "default": default
    }
  })
}
