use serde_json::{Map, Value, json};

use super::{
  MAX_CURSOR_BYTES, array, boolean, integer, mutation_response, non_empty_string, non_negative_integer, nullable,
  object, positive_integer, schema_ref, string_enum, string_map, unique_array, versioned_resource,
};

pub(super) fn component_schemas() -> Value {
  let mut schemas = Map::new();
  insert_common_schemas(&mut schemas);
  insert_operational_schemas(&mut schemas);
  insert_project_schemas(&mut schemas);
  insert_pipeline_schemas(&mut schemas);
  insert_repository_schemas(&mut schemas);
  insert_configuration_schemas(&mut schemas);
  insert_trigger_schemas(&mut schemas);
  insert_job_event_schemas(&mut schemas);
  Value::Object(schemas)
}

fn insert_job_event_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "JobEventResource".to_owned(),
    object(
      [
        ("sequence", positive_integer()),
        ("kind", json!({"type": "string", "minLength": 1, "maxLength": 64})),
        ("occurred_at_unix_ms", integer()),
        ("payload", json!({})),
      ],
      &["sequence", "kind", "occurred_at_unix_ms", "payload"],
    ),
  );
  schemas.insert(
    "JobEventPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("JobEventResource"))),
        ("cursor", non_negative_integer()),
      ],
      &["items", "cursor"],
    ),
  );
}

fn insert_operational_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "ManagementSecurityMode".to_owned(),
    string_enum(&["trusted_network_unauthenticated"]),
  );
  schemas.insert(
    "AuthenticatedIngressMode".to_owned(),
    string_enum(&["registration_credentials", "provider_verification"]),
  );
  schemas.insert(
    "CapabilityStatus".to_owned(),
    string_enum(&["available", "unavailable"]),
  );
  schemas.insert(
    "ManagementSecurity".to_owned(),
    object(
      [
        ("mode", schema_ref("ManagementSecurityMode")),
        ("operator_authentication", boolean()),
        ("externally_reachable", boolean()),
        ("external_access_acknowledged", boolean()),
      ],
      &[
        "mode",
        "operator_authentication",
        "externally_reachable",
        "external_access_acknowledged",
      ],
    ),
  );
  schemas.insert(
    "AuthenticatedIngressSecurity".to_owned(),
    object(
      [
        ("mode", schema_ref("AuthenticatedIngressMode")),
        ("authentication_required", boolean()),
      ],
      &["mode", "authentication_required"],
    ),
  );
  schemas.insert(
    "DeploymentSecurity".to_owned(),
    object(
      [
        ("management", schema_ref("ManagementSecurity")),
        ("agent", schema_ref("AuthenticatedIngressSecurity")),
        ("webhook", schema_ref("AuthenticatedIngressSecurity")),
      ],
      &["management", "agent", "webhook"],
    ),
  );
  schemas.insert(
    "IngressMetadata".to_owned(),
    object(
      [
        ("management_enabled", boolean()),
        ("agent_enabled", boolean()),
        ("webhook_enabled", boolean()),
        ("listeners_separate", boolean()),
      ],
      &[
        "management_enabled",
        "agent_enabled",
        "webhook_enabled",
        "listeners_separate",
      ],
    ),
  );
  schemas.insert(
    "CapabilityMetadata".to_owned(),
    object(
      [("name", non_empty_string()), ("status", schema_ref("CapabilityStatus"))],
      &["name", "status"],
    ),
  );
  schemas.insert(
    "OperationalMetadata".to_owned(),
    object(
      [
        ("api_version", json!({"const": "v1"})),
        ("security", schema_ref("DeploymentSecurity")),
        ("ingress", schema_ref("IngressMetadata")),
        ("capabilities", array(schema_ref("CapabilityMetadata"))),
      ],
      &["api_version", "security", "ingress", "capabilities"],
    ),
  );
}

fn insert_common_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "ErrorCode".to_owned(),
    json!({
      "type": "string",
      "enum": [
        "invalid_request", "unsupported_media_type", "unsupported_api_version", "payload_too_large",
        "invalid_idempotency_key", "idempotency_conflict", "precondition_required", "precondition_failed",
        "not_found", "conflict", "capability_unavailable", "unavailable", "rate_limited", "internal"
      ]
    }),
  );
  schemas.insert(
    "ErrorResponse".to_owned(),
    object(
      [
        ("code", schema_ref("ErrorCode")),
        ("message", non_empty_string()),
        ("request_id", non_empty_string()),
      ],
      &["code", "message", "request_id"],
    ),
  );
  schemas.insert(
    "MutationDisposition".to_owned(),
    json!({"type": "string", "enum": ["applied", "replayed"]}),
  );
}

fn insert_project_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "CreateProjectRequest".to_owned(),
    object(
      [
        ("parent_id", nullable(non_empty_string())),
        ("name", non_empty_string()),
      ],
      &["parent_id", "name"],
    ),
  );
  schemas.insert(
    "RenameProjectRequest".to_owned(),
    object([("name", non_empty_string())], &["name"]),
  );
  schemas.insert(
    "MoveProjectRequest".to_owned(),
    object([("parent_id", nullable(non_empty_string()))], &["parent_id"]),
  );
  schemas.insert(
    "ProjectResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("parent_id", nullable(non_empty_string())),
        ("name", non_empty_string()),
        ("version", positive_integer()),
        ("created_at_unix_ms", integer()),
        ("updated_at_unix_ms", integer()),
      ],
      &[
        "id",
        "parent_id",
        "name",
        "version",
        "created_at_unix_ms",
        "updated_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "ProjectMutationResponse".to_owned(),
    mutation_response("ProjectResource"),
  );
  schemas.insert(
    "ProjectDetails".to_owned(),
    object(
      [
        ("project", schema_ref("ProjectResource")),
        ("ancestors", array(schema_ref("ProjectResource"))),
      ],
      &["project", "ancestors"],
    ),
  );
  schemas.insert(
    "ProjectPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("ProjectResource"))),
        (
          "next_cursor",
          nullable(json!({"type": "string", "minLength": 1, "maxLength": MAX_CURSOR_BYTES})),
        ),
      ],
      &["items", "next_cursor"],
    ),
  );
  schemas.insert(
    "DeleteProjectResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("project_id", non_empty_string()),
      ],
      &["disposition", "project_id"],
    ),
  );
}

fn insert_pipeline_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "DependencyPolicy".to_owned(),
    string_enum(&["all_succeeded", "all_completed", "any_succeeded"]),
  );
  schemas.insert(
    "JobExecution".to_owned(),
    object(
      [
        ("octafile", nullable(non_empty_string())),
        ("commands", array(non_empty_string())),
        ("arguments", array(json!({"type": "string"}))),
        ("concurrency", nullable(positive_integer())),
        ("parallel", boolean()),
        ("failfast", boolean()),
      ],
      &["commands"],
    ),
  );
  schemas.insert(
    "PipelineNode".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("name", non_empty_string()),
        ("dependency_policy", schema_ref("DependencyPolicy")),
        ("required_capabilities", array(non_empty_string())),
        ("execution", schema_ref("JobExecution")),
      ],
      &["id", "name", "dependency_policy", "required_capabilities", "execution"],
    ),
  );
  schemas.insert(
    "PipelineEdge".to_owned(),
    object(
      [("predecessor", non_empty_string()), ("dependent", non_empty_string())],
      &["predecessor", "dependent"],
    ),
  );
  schemas.insert(
    "PipelineDag".to_owned(),
    object(
      [
        ("nodes", array(schema_ref("PipelineNode"))),
        ("edges", array(schema_ref("PipelineEdge"))),
      ],
      &["nodes", "edges"],
    ),
  );
  schemas.insert(
    "CreatePipelineRequest".to_owned(),
    object(
      [
        ("project_id", non_empty_string()),
        ("name", non_empty_string()),
        ("dag", schema_ref("PipelineDag")),
      ],
      &["project_id", "name", "dag"],
    ),
  );
  schemas.insert(
    "PublishPipelineVersionRequest".to_owned(),
    object([("dag", schema_ref("PipelineDag"))], &["dag"]),
  );
  schemas.insert(
    "PipelineResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("project_id", non_empty_string()),
        ("name", non_empty_string()),
        ("version", positive_integer()),
        ("dag", schema_ref("PipelineDag")),
        ("published_at_unix_ms", integer()),
      ],
      &["id", "project_id", "name", "version", "dag", "published_at_unix_ms"],
    ),
  );
  schemas.insert(
    "PipelineMutationResponse".to_owned(),
    mutation_response("PipelineResource"),
  );
}

fn insert_repository_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "RepositorySelectionPolicy".to_owned(),
    object(
      [
        ("allowed_references", unique_array(non_empty_string())),
        ("default_reference", nullable(non_empty_string())),
        ("allow_exact_revision", boolean()),
      ],
      &["allowed_references", "default_reference", "allow_exact_revision"],
    ),
  );
  schemas.insert(
    "RepositoryDefinition".to_owned(),
    object(
      [
        ("vcs_integration_id", non_empty_string()),
        ("repository_locator", non_empty_string()),
        ("selection", schema_ref("RepositorySelectionPolicy")),
      ],
      &["vcs_integration_id", "repository_locator", "selection"],
    ),
  );
  schemas.insert(
    "CreateRepositoryRequest".to_owned(),
    object(
      [
        ("project_id", non_empty_string()),
        ("name", non_empty_string()),
        ("definition", schema_ref("RepositoryDefinition")),
      ],
      &["project_id", "name", "definition"],
    ),
  );
  schemas.insert(
    "PublishRepositoryVersionRequest".to_owned(),
    object([("definition", schema_ref("RepositoryDefinition"))], &["definition"]),
  );
  schemas.insert(
    "RepositoryResource".to_owned(),
    versioned_resource("RepositoryDefinition"),
  );
  schemas.insert(
    "RepositoryMutationResponse".to_owned(),
    mutation_response("RepositoryResource"),
  );
}

fn insert_configuration_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "ParameterType".to_owned(),
    string_enum(&["string", "integer", "boolean"]),
  );
  schemas.insert(
    "ParameterDefinition".to_owned(),
    object(
      [
        ("value_type", schema_ref("ParameterType")),
        ("required", boolean()),
        ("default", nullable(json!({}))),
      ],
      &["value_type", "required", "default"],
    ),
  );
  schemas.insert(
    "ParameterSchema".to_owned(),
    object(
      [
        (
          "parameters",
          json!({"type": "object", "additionalProperties": schema_ref("ParameterDefinition")}),
        ),
        ("deny_unknown", boolean()),
      ],
      &["parameters", "deny_unknown"],
    ),
  );
  schemas.insert(
    "TriggerKind".to_owned(),
    string_enum(&["manual", "scheduled", "external", "internal"]),
  );
  schemas.insert(
    "AgentRequirements".to_owned(),
    object(
      [
        ("capabilities", unique_array(non_empty_string())),
        ("labels", string_map()),
        ("minimum_cpu_millis", non_negative_integer()),
        ("minimum_memory_bytes", non_negative_integer()),
        ("minimum_disk_bytes", non_negative_integer()),
      ],
      &[
        "capabilities",
        "labels",
        "minimum_cpu_millis",
        "minimum_memory_bytes",
        "minimum_disk_bytes",
      ],
    ),
  );
  schemas.insert(
    "RuntimeClass".to_owned(),
    string_enum(&["native", "oci_process", "oci_hypervisor"]),
  );
  schemas.insert("PlatformOs".to_owned(), string_enum(&["linux", "windows", "macos"]));
  schemas.insert("PlatformArchitecture".to_owned(), string_enum(&["amd64", "arm64"]));
  schemas.insert(
    "NetworkPolicy".to_owned(),
    json!({
      "oneOf": [
        object([("mode", json!({"const": "disabled"}))], &["mode"]),
        object([("mode", json!({"const": "unrestricted"}))], &["mode"]),
        object(
          [
            ("mode", json!({"const": "restricted"})),
            ("allowed_hosts", unique_array(non_empty_string()))
          ],
          &["mode", "allowed_hosts"]
        )
      ],
      "discriminator": {"propertyName": "mode"}
    }),
  );
  schemas.insert(
    "RuntimePolicy".to_owned(),
    object(
      [
        ("class", schema_ref("RuntimeClass")),
        ("operating_system", schema_ref("PlatformOs")),
        ("architecture", schema_ref("PlatformArchitecture")),
        ("immutable_image", nullable(non_empty_string())),
        ("cpu_millis", non_negative_integer()),
        ("memory_bytes", non_negative_integer()),
        ("writable_disk_bytes", non_negative_integer()),
        ("timeout_seconds", non_negative_integer()),
        ("network", schema_ref("NetworkPolicy")),
        ("workload_identity_profile", nullable(non_empty_string())),
      ],
      &[
        "class",
        "operating_system",
        "architecture",
        "immutable_image",
        "cpu_millis",
        "memory_bytes",
        "writable_disk_bytes",
        "timeout_seconds",
        "network",
        "workload_identity_profile",
      ],
    ),
  );
  schemas.insert(
    "CachePolicy".to_owned(),
    object(
      [
        ("namespace", nullable(non_empty_string())),
        ("read", boolean()),
        ("write", boolean()),
      ],
      &["namespace", "read", "write"],
    ),
  );
  schemas.insert(
    "ArtifactPolicy".to_owned(),
    object(
      [
        ("artifact_count", non_negative_integer()),
        ("artifact_bytes", non_negative_integer()),
        ("report_count", non_negative_integer()),
        ("report_bytes", non_negative_integer()),
        ("single_output_bytes", non_negative_integer()),
      ],
      &[
        "artifact_count",
        "artifact_bytes",
        "report_count",
        "report_bytes",
        "single_output_bytes",
      ],
    ),
  );
  schemas.insert(
    "RetryClass".to_owned(),
    string_enum(&["execution_failure", "infrastructure_failure"]),
  );
  schemas.insert(
    "RetryPolicy".to_owned(),
    object(
      [
        ("max_attempts", positive_integer()),
        ("retry_on", unique_array(schema_ref("RetryClass"))),
      ],
      &["max_attempts", "retry_on"],
    ),
  );
  schemas.insert(
    "BuildConfigurationDefinition".to_owned(),
    object(
      [
        ("enabled", boolean()),
        ("repository_id", non_empty_string()),
        ("repository_version", positive_integer()),
        ("pipeline_id", non_empty_string()),
        ("pipeline_version", positive_integer()),
        ("parameters", schema_ref("ParameterSchema")),
        ("triggers", unique_array(schema_ref("TriggerKind"))),
        ("agent_requirements", schema_ref("AgentRequirements")),
        ("allowed_pools", unique_array(non_empty_string())),
        ("runtime", schema_ref("RuntimePolicy")),
        ("cache", schema_ref("CachePolicy")),
        ("artifacts", schema_ref("ArtifactPolicy")),
        ("retry", schema_ref("RetryPolicy")),
      ],
      &[
        "enabled",
        "repository_id",
        "repository_version",
        "pipeline_id",
        "pipeline_version",
        "parameters",
        "triggers",
        "agent_requirements",
        "allowed_pools",
        "runtime",
        "cache",
        "artifacts",
        "retry",
      ],
    ),
  );
  schemas.insert(
    "CreateBuildConfigurationRequest".to_owned(),
    object(
      [
        ("project_id", non_empty_string()),
        ("name", non_empty_string()),
        ("definition", schema_ref("BuildConfigurationDefinition")),
      ],
      &["project_id", "name", "definition"],
    ),
  );
  schemas.insert(
    "PublishBuildConfigurationVersionRequest".to_owned(),
    object(
      [("definition", schema_ref("BuildConfigurationDefinition"))],
      &["definition"],
    ),
  );
  schemas.insert(
    "BuildConfigurationResource".to_owned(),
    versioned_resource("BuildConfigurationDefinition"),
  );
  schemas.insert(
    "BuildConfigurationMutationResponse".to_owned(),
    mutation_response("BuildConfigurationResource"),
  );
}

fn insert_trigger_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "ManualSource".to_owned(),
    json!({
      "oneOf": [
        object([("kind", json!({"const": "default_reference"}))], &["kind"]),
        object(
          [("kind", json!({"const": "reference"})), ("value", non_empty_string())],
          &["kind", "value"]
        ),
        object(
          [("kind", json!({"const": "exact_revision"})), ("value", non_empty_string())],
          &["kind", "value"]
        )
      ],
      "discriminator": {"propertyName": "kind"}
    }),
  );
  schemas.insert(
    "AcceptManualTriggerRequest".to_owned(),
    object(
      [
        ("trigger_id", non_empty_string()),
        ("trigger_version", positive_integer()),
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("deduplication_identity", non_empty_string()),
        ("source", schema_ref("ManualSource")),
        ("parameters", json!({"type": "object", "additionalProperties": true})),
        ("priority", integer()),
      ],
      &[
        "trigger_id",
        "trigger_version",
        "configuration_id",
        "configuration_version",
        "deduplication_identity",
        "source",
        "parameters",
        "priority",
      ],
    ),
  );
  schemas.insert(
    "TriggerEvaluationResponse".to_owned(),
    json!({
      "oneOf": [
        object(
          [
            ("outcome", json!({"const": "accepted"})),
            ("disposition", schema_ref("MutationDisposition")),
            ("trigger_occurrence_id", non_empty_string()),
            ("build_id", non_empty_string()),
            ("attempt_id", non_empty_string()),
            ("ready_job_ids", array(non_empty_string()))
          ],
          &["outcome", "disposition", "trigger_occurrence_id", "build_id", "attempt_id", "ready_job_ids"]
        ),
        object(
          [
            ("outcome", json!({"const": "suppressed"})),
            ("disposition", schema_ref("MutationDisposition")),
            ("trigger_occurrence_id", non_empty_string())
          ],
          &["outcome", "disposition", "trigger_occurrence_id"]
        )
      ],
      "discriminator": {"propertyName": "outcome"}
    }),
  );
}
