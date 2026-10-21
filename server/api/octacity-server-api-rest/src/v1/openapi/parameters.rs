use serde_json::{Value, json};

use super::*;

pub(super) fn parameters(operation: &ManagementOperation) -> Vec<Value> {
  let mut parameters = Vec::new();
  for name in [
    "project_id",
    "property_version",
    "workflow_id",
    "issue_id",
    "review_id",
    "request_id",
    "sequence",
    "comment_id",
    "pipeline_id",
    "repository_id",
    "configuration_id",
    "factory_configuration_id",
    "flow_definition_id",
    "factory_run_id",
    "node_attempt_id",
    "build_id",
    "attempt_id",
    "job_id",
    "artifact_id",
    "cache_session_id",
    "pool_id",
    "agent_id",
    "integration_id",
    "trigger_id",
    "input_id",
    "group_id",
    "version",
  ] {
    if operation.path.contains(&format!("{{{name}}}")) {
      parameters.push(json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": if name == "review_id" { schema_ref("FactoryDigest") }
          else if name == "sequence" { positive_integer() }
          else if name == "property_version" { non_negative_integer() }
          else if name == "version" { positive_integer() }
          else if matches!(name, "workflow_id" | "issue_id" | "request_id" | "comment_id") { json!({"type":"string","format":"uuid"}) }
          else { non_empty_string() }
      }));
    }
  }
  if operation.path.contains("{collection}") {
    parameters.push(json!({"name":"collection","in":"path","required":true,"schema":{"type":"string","enum":octacity_server_application::FactoryDiagnosticCollection::ALL.into_iter().map(|kind| kind.as_str()).collect::<Vec<_>>()}}));
  }
  match operation.parameter_profile {
    ParameterProfile::IssueJournal | ParameterProfile::IssueJournalEvents | ParameterProfile::IssueJournalDetail => {
      if operation.parameter_profile != ParameterProfile::IssueJournalDetail {
        parameters.extend([cursor_parameter_with_max(octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES),
          limit_parameter(octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE, super::super::adapter::DEFAULT_PAGE_LIMIT)]);
      }
      if operation.parameter_profile == ParameterProfile::IssueJournal {
        parameters.push(json!({"name":"collection","in":"query","required":false,"schema":{"type":"string","default":"flow-runs",
          "enum":octacity_server_application::FactoryDiagnosticCollection::ALL.into_iter().map(|c|c.as_str()).collect::<Vec<_>>()}}));
      } else {
        parameters.extend([json!({"name":"build_execution","in":"query","required":true,"schema":schema_ref("FactoryDigest")}),
          json!({"name":"job_id","in":"query","required":true,"schema":{"type":"string","format":"uuid"}})]);
      }
    },
    ParameterProfile::FactoryDiagnostics => parameters.extend([cursor_parameter_with_max(256),
      limit_parameter(octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE, super::super::adapter::DEFAULT_PAGE_LIMIT)]),
    ParameterProfile::WorkSourceInputChoices => parameters.extend([
      json!({"name":"connection_id","in":"query","required":true,"schema":{"type":"string","format":"uuid"}}),
      json!({"name":"connection_version","in":"query","required":true,"schema":positive_integer()}),
    ]),
    ParameterProfile::WorkSourceDiagnostics => parameters.extend([cursor_parameter_with_max(512),limit_parameter(64,super::super::adapter::DEFAULT_PAGE_LIMIT)]),
    ParameterProfile::WorkSourceConfigurations => parameters.extend([cursor_parameter_with_max(256), limit_parameter(octacity_server_application::MAX_WORK_SOURCE_CONFIGURATION_PAGE_SIZE,super::super::adapter::DEFAULT_PAGE_LIMIT)]),
    ParameterProfile::None => {}
    ParameterProfile::IntegrationDiscovery => parameters.extend([cursor_parameter_with_max(128),limit_parameter(octacity_server_application::MAX_INTEGRATION_DISCOVERY_PAGE_SIZE,super::super::adapter::DEFAULT_PAGE_LIMIT)]),
    ParameterProfile::ProjectList => parameters.extend([
      json!({"name": "parent_id", "in": "query", "required": false, "schema": non_empty_string()}),
      cursor_parameter(),
      limit_parameter(MAX_PROJECT_LIST_PAGE_SIZE, super::super::adapter::DEFAULT_PAGE_LIMIT),
    ]),
    ParameterProfile::IssueTimeline | ParameterProfile::IssueResults => {
      parameters.push(cursor_parameter_with_max(octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES));
      if matches!(operation.parameter_profile,ParameterProfile::IssueTimeline) {
        parameters.extend([limit_parameter(octacity_server_application::MAX_ISSUE_HISTORY_PAGE_SIZE,super::super::adapter::DEFAULT_PAGE_LIMIT),json!({"name":"kinds","in":"query","required":false,"schema":{"type":"array","minItems":1,"maxItems":octacity_server_application::IssueTimelineKind::ALL.len(),"uniqueItems":true,"items":{"type":"string","enum":octacity_server_application::IssueTimelineKind::ALL}},"style":"form","explode":false})]);
      } else {parameters.extend([limit_parameter(octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,super::super::adapter::DEFAULT_PAGE_LIMIT),json!({"name":"work_id","in":"query","required":true,"schema":{"type":"string","format":"uuid"}}),json!({"name":"collection","in":"query","required":true,"schema":{"type":"string","enum":octacity_server_application::FactoryDiagnosticCollection::ALL.into_iter().map(|c|c.as_str()).collect::<Vec<_>>()}})]);}
    },
    ParameterProfile::IssueChecks | ParameterProfile::IssueArtifacts | ParameterProfile::IssueArtifactContent => {
      parameters.push(json!({"name":"work_id","in":"query","required":true,"schema":{"type":"string","format":"uuid"}}));
      if matches!(operation.parameter_profile,ParameterProfile::IssueArtifactContent) {
        parameters.push(json!({"name":"mode","in":"query","required":true,"schema":{"type":"string","enum":["preview","download"]}}));
      } else {
        parameters.extend([cursor_parameter_with_max(octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES),limit_parameter(octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,super::super::adapter::DEFAULT_PAGE_LIMIT)]);
        if matches!(operation.parameter_profile,ParameterProfile::IssueChecks) {parameters.push(json!({"name":"record","in":"query","required":true,"schema":schema_ref("FactoryDigest")}));}
      }
    },
    ParameterProfile::IssueExternalChecks=>parameters.push(cursor_parameter_with_max(octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES)),
    ParameterProfile::IssueObservations => parameters.extend([cursor_parameter_with_max(octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES),limit_parameter(octacity_server_application::MAX_ISSUE_HISTORY_PAGE_SIZE,super::super::adapter::DEFAULT_PAGE_LIMIT)]),
    ParameterProfile::IssueExecutions => parameters.extend([
      cursor_parameter_with_max(octacity_server_application::MAX_ISSUE_EXECUTION_CURSOR_BYTES),
      limit_parameter(octacity_server_application::MAX_ISSUE_HISTORY_PAGE_SIZE,super::super::adapter::DEFAULT_PAGE_LIMIT)
    ]),
    ParameterProfile::IssueHistory => parameters.extend([json!({"name":"after","in":"query","required":false,"schema":positive_integer()}),limit_parameter(200,super::super::adapter::DEFAULT_PAGE_LIMIT)]),
    ParameterProfile::CurrentDefinitionList => parameters.extend([
      cursor_parameter(),
      limit_parameter(
        octacity_server_application::MAX_CURRENT_DEFINITION_PAGE_SIZE,
        super::super::adapter::DEFAULT_PAGE_LIMIT,
      ),
    ]),
    ParameterProfile::FactoryPhaseReady=>parameters.extend([
      json!({"name":"policy_digest","in":"query","required":true,"schema":schema_ref("FactoryDigest")}),
      cursor_parameter(),limit_parameter(200,super::super::adapter::DEFAULT_PAGE_LIMIT),
    ]),
    ParameterProfile::FactoryRunList=>parameters.extend([
      json!({"name":"configuration_id","in":"query","required":false,"schema":non_empty_string()}),
      json!({"name":"source","in":"query","required":false,"schema":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[a-z0-9][a-z0-9._-]*$"}}),
      json!({"name":"state","in":"query","required":false,"schema":schema_ref("FactoryRunState")}),
      json!({"name":"decision","in":"query","required":false,"schema":schema_ref("FactoryDecisionOutcome")}),
      json!({"name":"delivery_state","in":"query","required":false,"schema":schema_ref("FactoryDeliveryState")}),
      json!({"name":"admitted_from_unix_ms","in":"query","required":false,"schema":{"type":"integer","minimum":0}}),
      json!({"name":"admitted_before_unix_ms","in":"query","required":false,"schema":{"type":"integer","minimum":0}}),
      cursor_parameter_with_max(128),limit_parameter(200,super::super::adapter::DEFAULT_PAGE_LIMIT),
    ]),
    ParameterProfile::FactoryConfigurationChoices => parameters.extend([
      json!({"name":"kind","in":"query","required":false,"schema":schema_ref("FactoryChoiceKind")}),
      cursor_parameter(),
      limit_parameter(octacity_server_application::MAX_CURRENT_DEFINITION_PAGE_SIZE,super::super::adapter::DEFAULT_PAGE_LIMIT),
    ]),
    ParameterProfile::AgentPoolList => parameters.extend([
      cursor_parameter(),
      limit_parameter(MAX_AGENT_POOL_LIST_PAGE_SIZE, super::super::adapter::DEFAULT_PAGE_LIMIT),
    ]),
    ParameterProfile::AgentList => parameters.extend([
      json!({"name": "pool_id", "in": "query", "required": false, "schema": non_empty_string()}),
      cursor_parameter(),
      limit_parameter(MAX_AGENT_LIST_PAGE_SIZE, super::super::adapter::DEFAULT_PAGE_LIMIT),
    ]),
    ParameterProfile::ProjectBuildList => parameters.extend([
      json!({"name": "configuration_id", "in": "query", "required": false, "schema": non_empty_string()}),
      json!({"name": "state", "in": "query", "required": false, "schema": schema_ref("BuildState")}),
      cursor_parameter_with_max(octacity_server_application::MAX_BUILD_PAGE_CURSOR_BYTES),
      limit_parameter(
        octacity_server_application::MAX_BUILD_LIST_PAGE_SIZE,
        super::super::adapter::DEFAULT_PAGE_LIMIT,
      ),
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
    ParameterProfile::BuildLogSearch => parameters.extend([
      json!({
        "name": "query",
        "in": "query",
        "required": true,
        "schema": {
          "type": "string",
          "minLength": 1,
          "maxLength": octacity_server_application::MAX_LOG_SEARCH_QUERY_BYTES
        }
      }),
      json!({
        "name": "mode",
        "in": "query",
        "required": true,
        "schema": schema_ref("BuildLogSearchMode")
      }),
      json!({"name": "build_id", "in": "query", "required": false, "schema": non_empty_string()}),
      json!({"name": "attempt_id", "in": "query", "required": false, "schema": non_empty_string()}),
      json!({"name": "job_id", "in": "query", "required": false, "schema": non_empty_string()}),
      json!({
        "name": "stream",
        "in": "query",
        "required": false,
        "schema": schema_ref("BuildLogStream")
      }),
      json!({"name": "occurred_from_unix_ms", "in": "query", "required": false, "schema": {"type": "integer", "format": "int64"}}),
      json!({"name": "occurred_through_unix_ms", "in": "query", "required": false, "schema": {"type": "integer", "format": "int64"}}),
      cursor_parameter(),
      limit_parameter(
        octacity_server_application::MAX_LOG_SEARCH_PAGE_SIZE,
        super::super::adapter::DEFAULT_LOG_SEARCH_LIMIT,
      ),
    ]),
    ParameterProfile::ResourceSearch => parameters.extend([
      json!({
        "name": "query",
        "in": "query",
        "required": true,
        "schema": {
          "type": "string",
          "minLength": 1,
          "x-max-utf8-bytes": octacity_server_application::MAX_RESOURCE_SEARCH_QUERY_BYTES
        }
      }),
      json!({
        "name": "kinds",
        "in": "query",
        "required": false,
        "description": "Optional repeatable resource-kind allowlist.",
        "style": "form",
        "explode": true,
        "schema": {
          "type": "array",
          "minItems": 1,
          "maxItems": 4,
          "uniqueItems": true,
          "items": schema_ref("ResourceSearchKind")
        }
      }),
      cursor_parameter_with_max(octacity_server_application::MAX_RESOURCE_SEARCH_CURSOR_BYTES),
      limit_parameter(
        octacity_server_application::MAX_RESOURCE_SEARCH_RESULT_PAGE_SIZE,
        super::super::adapter::DEFAULT_PAGE_LIMIT,
      ),
    ]),
    ParameterProfile::OperatorAttention => parameters.extend([
      repeated_identity_parameter("build_ids", "Selected Build identities."),
      repeated_identity_parameter("agent_ids", "Selected Agent identities."),
      repeated_identity_parameter("pool_ids", "Selected Agent Pool identities."),
      json!({
        "name": "include_critical_conditions",
        "in": "query",
        "required": false,
        "schema": {"type": "boolean", "default": false}
      }),
      json!({"name": "occurred_from_unix_ms", "in": "query", "required": false, "schema": {"type": "integer", "format": "int64"}}),
      json!({"name": "occurred_through_unix_ms", "in": "query", "required": false, "schema": {"type": "integer", "format": "int64"}}),
      cursor_parameter_with_max(octacity_server_application::MAX_OPERATOR_ATTENTION_CURSOR_BYTES),
      limit_parameter(
        octacity_server_application::MAX_OPERATOR_ATTENTION_RESULT_PAGE_SIZE,
        super::super::adapter::DEFAULT_PAGE_LIMIT,
      ),
    ]),
    ParameterProfile::AuditFacts => parameters.extend([
      json!({"name": "actor_kind", "in": "query", "required": false, "schema": schema_ref("AuditActorKind")}),
      bounded_utf8_parameter("actor_identity", octacity_server_application::MAX_AUDIT_ACTOR_IDENTITY_BYTES),
      bounded_utf8_parameter("operation", octacity_server_application::MAX_AUDIT_OPERATION_BYTES),
      bounded_utf8_parameter("target_kind", octacity_server_application::MAX_AUDIT_TARGET_KIND_BYTES),
      bounded_utf8_parameter("target_identity", octacity_server_application::MAX_AUDIT_TARGET_IDENTITY_BYTES),
      bounded_utf8_parameter("request_identity", octacity_server_application::MAX_AUDIT_REQUEST_IDENTITY_BYTES),
      json!({"name": "occurred_from_unix_ms", "in": "query", "required": false, "schema": {"type": "integer", "format": "int64"}}),
      json!({"name": "occurred_through_unix_ms", "in": "query", "required": false, "schema": {"type": "integer", "format": "int64"}}),
      cursor_parameter(),
      limit_parameter(
        octacity_server_application::MAX_AUDIT_PAGE_SIZE,
        super::super::adapter::DEFAULT_AUDIT_LIMIT,
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
      "required": !operation.protected_ingress,
      "description": "Canonical strong ETag carrying a positive resource version, for example \"7\".",
      "schema": {"type": "string", "pattern": "^\\\"[1-9][0-9]*\\\"$"}
    }));
  }
  if operation.protected_ingress && operation.optimistic_precondition {
    parameters.push(json!({"name":"If-None-Match","in":"header","required":false,"schema":{"type":"string","const":"*"},
      "description":"Required for initial metadata/administrative publication; otherwise use If-Match matching the intent version/revision."}));
  }
  parameters
}

fn cursor_parameter() -> Value {
  cursor_parameter_with_max(MAX_CURSOR_BYTES)
}

fn repeated_identity_parameter(name: &str, description: &str) -> Value {
  json!({
    "name": name,
    "in": "query",
    "required": false,
    "description": description,
    "style": "form",
    "explode": true,
    "schema": {
      "type": "array",
      "minItems": 1,
      "maxItems": octacity_server_application::MAX_OPERATOR_ATTENTION_SCOPE_TARGETS,
      "uniqueItems": true,
      "items": {"type": "string", "format": "uuid"}
    }
  })
}

fn cursor_parameter_with_max(maximum: usize) -> Value {
  json!({
    "name": "after",
    "in": "query",
    "required": false,
    "schema": {"type": "string", "minLength": 1, "maxLength": maximum}
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

fn bounded_utf8_parameter(name: &str, maximum_bytes: usize) -> Value {
  json!({
    "name": name,
    "in": "query",
    "required": false,
    "schema": {"type": "string", "minLength": 1, "x-max-utf8-bytes": maximum_bytes}
  })
}
