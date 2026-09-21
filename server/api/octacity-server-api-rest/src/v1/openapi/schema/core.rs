use serde_json::{Map, Value, json};

use super::super::{
  MAX_CURSOR_BYTES, array, boolean, integer, mutation_response, non_empty_string, non_negative_integer, nullable,
  object, positive_integer, schema_ref, string_enum, string_map, unique_array, versioned_resource,
};
use super::{agent, execution, operational, trigger};

pub(crate) fn component_schemas() -> Value {
  let mut schemas = Map::new();
  insert_common_schemas(&mut schemas);
  operational::insert_operational_schemas(&mut schemas);
  insert_project_schemas(&mut schemas);
  insert_agent_pool_schemas(&mut schemas);
  agent::insert_agent_detail_schemas(&mut schemas);
  insert_agent_schemas(&mut schemas);
  insert_pipeline_schemas(&mut schemas);
  insert_repository_schemas(&mut schemas);
  insert_configuration_schemas(&mut schemas);
  trigger::insert_trigger_schemas(&mut schemas);
  execution::insert_execution_detail_schemas(&mut schemas);
  insert_execution_schemas(&mut schemas);
  operational::insert_job_event_schemas(&mut schemas);
  Value::Object(schemas)
}

fn insert_execution_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "AttemptSummaryResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("build_id", non_empty_string()),
        ("number", positive_integer()),
        ("retry_of_attempt_id", nullable(non_empty_string())),
        ("state", string_enum(&["running", "succeeded", "failed", "cancelled"])),
        ("version", positive_integer()),
        ("created_at_unix_ms", integer()),
        ("updated_at_unix_ms", integer()),
      ],
      &[
        "id",
        "build_id",
        "number",
        "retry_of_attempt_id",
        "state",
        "version",
        "created_at_unix_ms",
        "updated_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "BuildResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("project_id", non_empty_string()),
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("pipeline_id", non_empty_string()),
        ("pipeline_version", positive_integer()),
        ("repository_id", non_empty_string()),
        ("repository_version", positive_integer()),
        ("immutable_revision", non_empty_string()),
        ("parameters", schema_ref("BuildParameters")),
        ("source", schema_ref("ManualSource")),
        ("effective_policy", schema_ref("EffectiveProjectPolicy")),
        ("priority", integer()),
        ("state", string_enum(&["running", "succeeded", "failed", "cancelled"])),
        ("version", positive_integer()),
        ("trigger", schema_ref("TriggerHistory")),
        ("created_at_unix_ms", integer()),
        ("updated_at_unix_ms", integer()),
        ("current_attempt", schema_ref("AttemptSummaryResource")),
      ],
      &[
        "id",
        "project_id",
        "configuration_id",
        "configuration_version",
        "pipeline_id",
        "pipeline_version",
        "repository_id",
        "repository_version",
        "immutable_revision",
        "parameters",
        "source",
        "effective_policy",
        "priority",
        "state",
        "version",
        "trigger",
        "created_at_unix_ms",
        "updated_at_unix_ms",
        "current_attempt",
      ],
    ),
  );
  schemas.insert(
    "JobQueueResource".to_owned(),
    object(
      [("priority", integer()), ("enqueued_at_unix_ms", integer())],
      &["priority", "enqueued_at_unix_ms"],
    ),
  );
  schemas.insert(
    "JobAssignmentResource".to_owned(),
    object(
      [
        ("selected_pool_id", non_empty_string()),
        ("assigned_agent_id", non_empty_string()),
      ],
      &["selected_pool_id", "assigned_agent_id"],
    ),
  );
  schemas.insert(
    "JobTerminalResource".to_owned(),
    object(
      [
        ("state", string_enum(&["succeeded", "failed", "cancelled", "skipped"])),
        (
          "failure_classification",
          nullable(string_enum(&[
            "execution",
            "infrastructure",
            "cancelled",
            "dependency_policy",
          ])),
        ),
        ("completed_at_unix_ms", integer()),
      ],
      &["state", "failure_classification", "completed_at_unix_ms"],
    ),
  );
  schemas.insert(
    "JobResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("attempt_id", non_empty_string()),
        ("pipeline_node_id", non_empty_string()),
        ("dependency_job_ids", unique_array(non_empty_string())),
        ("dependency_policy", schema_ref("DependencyPolicy")),
        ("allowed_pool_ids", unique_array(non_empty_string())),
        ("placement", schema_ref("JobPlacement")),
        (
          "state",
          string_enum(&[
            "blocked",
            "ready",
            "leased",
            "running",
            "cancelling",
            "succeeded",
            "failed",
            "cancelled",
            "skipped",
          ]),
        ),
        ("version", positive_integer()),
        ("created_at_unix_ms", integer()),
        ("updated_at_unix_ms", integer()),
        ("queue", nullable(schema_ref("JobQueueResource"))),
        ("assignment", nullable(schema_ref("JobAssignmentResource"))),
        ("terminal", nullable(schema_ref("JobTerminalResource"))),
        ("event_cursor", non_negative_integer()),
        ("outputs", array(schema_ref("JobOutput"))),
      ],
      &[
        "id",
        "attempt_id",
        "pipeline_node_id",
        "dependency_job_ids",
        "dependency_policy",
        "allowed_pool_ids",
        "placement",
        "state",
        "version",
        "created_at_unix_ms",
        "updated_at_unix_ms",
        "queue",
        "assignment",
        "terminal",
        "event_cursor",
        "outputs",
      ],
    ),
  );
  schemas.insert(
    "DagEdgeResource".to_owned(),
    object(
      [
        ("predecessor_job_id", non_empty_string()),
        ("dependent_job_id", non_empty_string()),
        ("dependency_policy", schema_ref("DependencyPolicy")),
      ],
      &["predecessor_job_id", "dependent_job_id", "dependency_policy"],
    ),
  );
  schemas.insert(
    "AttemptResource".to_owned(),
    object(
      [
        ("attempt", schema_ref("AttemptSummaryResource")),
        ("jobs", array(schema_ref("JobResource"))),
        ("edges", array(schema_ref("DagEdgeResource"))),
      ],
      &["attempt", "jobs", "edges"],
    ),
  );
  schemas.insert(
    "CancelBuildResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("build_id", non_empty_string()),
        ("attempt_id", non_empty_string()),
        ("cancelled_job_ids", unique_array(non_empty_string())),
        ("cancelling_job_ids", unique_array(non_empty_string())),
      ],
      &[
        "disposition",
        "build_id",
        "attempt_id",
        "cancelled_job_ids",
        "cancelling_job_ids",
      ],
    ),
  );
  schemas.insert(
    "RetryBuildResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("build_id", non_empty_string()),
        ("source_attempt_id", non_empty_string()),
        ("attempt_id", non_empty_string()),
        ("attempt_number", positive_integer()),
        ("ready_job_ids", unique_array(non_empty_string())),
      ],
      &[
        "disposition",
        "build_id",
        "source_attempt_id",
        "attempt_id",
        "attempt_number",
        "ready_job_ids",
      ],
    ),
  );
}

fn insert_agent_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "IssueAgentEnrollmentRequest".to_owned(),
    object(
      [
        ("pool_id", non_empty_string()),
        ("pool_version", positive_integer()),
        ("expected_platform", nullable(schema_ref("AgentPlatform"))),
      ],
      &["pool_id", "pool_version", "expected_platform"],
    ),
  );
  schemas.insert(
    "IssueAgentEnrollmentResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("credential", non_empty_string()),
        ("pool_id", non_empty_string()),
        ("pool_version", positive_integer()),
        ("expires_at_unix_ms", integer()),
      ],
      &[
        "disposition",
        "credential",
        "pool_id",
        "pool_version",
        "expires_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "AgentStatus".to_owned(),
    string_enum(&["online", "offline", "draining"]),
  );
  schemas.insert(
    "AgentCapacity".to_owned(),
    object(
      [
        ("logical_cpu_count", positive_integer()),
        ("total_memory_bytes", positive_integer()),
        ("work_disk_total_bytes", positive_integer()),
        ("state_disk_total_bytes", positive_integer()),
        ("virtualization_available", boolean()),
      ],
      &[
        "logical_cpu_count",
        "total_memory_bytes",
        "work_disk_total_bytes",
        "state_disk_total_bytes",
        "virtualization_available",
      ],
    ),
  );
  schemas.insert(
    "AgentInventory".to_owned(),
    object(
      [
        ("agent_version", non_empty_string()),
        ("coordinator_protocols", unique_array(positive_integer())),
        ("labels", string_map()),
        ("host_platform", schema_ref("ProtocolPlatform")),
        ("runtimes", array(schema_ref("AgentRuntimeCapability"))),
        ("octa", schema_ref("AgentOctaInventory")),
        ("source_plugins", array(schema_ref("AgentSourcePluginInventory"))),
        ("cache", nullable(schema_ref("AgentCacheCapability"))),
      ],
      &[
        "agent_version",
        "coordinator_protocols",
        "labels",
        "host_platform",
        "runtimes",
        "octa",
        "source_plugins",
        "cache",
      ],
    ),
  );
  schemas.insert(
    "AgentResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("name", non_empty_string()),
        ("version", positive_integer()),
        ("pool_id", non_empty_string()),
        ("pool_version", positive_integer()),
        ("inventory", schema_ref("AgentInventory")),
        ("capacity", schema_ref("AgentCapacity")),
        ("status", schema_ref("AgentStatus")),
        ("last_seen_at_unix_ms", integer()),
      ],
      &[
        "id",
        "name",
        "version",
        "pool_id",
        "pool_version",
        "inventory",
        "capacity",
        "status",
        "last_seen_at_unix_ms",
      ],
    ),
  );
  schemas.insert("AgentMutationResponse".to_owned(), mutation_response("AgentResource"));
  schemas.insert(
    "AgentPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("AgentResource"))),
        ("next_cursor", nullable(non_empty_string())),
      ],
      &["items", "next_cursor"],
    ),
  );
  schemas.insert(
    "ReassignAgentPoolRequest".to_owned(),
    object([("pool_id", non_empty_string())], &["pool_id"]),
  );
  schemas.insert(
    "DrainAgentRequest".to_owned(),
    object([("mode", string_enum(&["graceful", "forced"]))], &["mode"]),
  );
}

fn insert_agent_pool_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "AgentPlatform".to_owned(),
    object(
      [
        ("operating_system", non_empty_string()),
        ("architecture", non_empty_string()),
      ],
      &["operating_system", "architecture"],
    ),
  );
  schemas.insert(
    "AgentPoolDrainState".to_owned(),
    string_enum(&["accepting", "graceful_drain", "forced_drain", "drained"]),
  );
  schemas.insert(
    "AgentPoolAdmissionPolicy".to_owned(),
    json!({
      "oneOf": [
        object([("mode", json!({"const": "any"}))], &["mode"]),
        object(
          [
            ("mode", json!({"const": "allowlist"})),
            (
              "platforms",
              json!({
                "type": "array",
                "items": schema_ref("AgentPlatform"),
                "uniqueItems": true,
                "minItems": 1,
                "maxItems": super::super::MAX_AGENT_POOL_ADMISSION_PLATFORMS
              })
            )
          ],
          &["mode", "platforms"]
        )
      ],
      "discriminator": {"propertyName": "mode"}
    }),
  );
  schemas.insert(
    "AgentPoolDefinition".to_owned(),
    object(
      [
        ("enabled", boolean()),
        ("drain_state", schema_ref("AgentPoolDrainState")),
        ("admission_policy", schema_ref("AgentPoolAdmissionPolicy")),
        (
          "concurrency_limit",
          json!({"type": "integer", "minimum": 1, "maximum": super::super::MAX_AGENT_POOL_STATIC_CAPACITY}),
        ),
        (
          "fairness_policy",
          json!({"type": "string", "enum": ["priority_fifo", "configuration_fair"]}),
        ),
        (
          "static_capacity_limit",
          json!({"type": "integer", "minimum": 1, "maximum": super::super::MAX_AGENT_POOL_STATIC_CAPACITY}),
        ),
      ],
      &[
        "enabled",
        "drain_state",
        "admission_policy",
        "concurrency_limit",
        "fairness_policy",
        "static_capacity_limit",
      ],
    ),
  );
  schemas.insert(
    "CreateAgentPoolRequest".to_owned(),
    object(
      [
        ("name", non_empty_string()),
        ("definition", schema_ref("AgentPoolDefinition")),
      ],
      &["name", "definition"],
    ),
  );
  schemas.insert(
    "PublishAgentPoolVersionRequest".to_owned(),
    object([("definition", schema_ref("AgentPoolDefinition"))], &["definition"]),
  );
  schemas.insert(
    "AgentPoolResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("name", non_empty_string()),
        ("version", positive_integer()),
        ("definition", schema_ref("AgentPoolDefinition")),
        ("published_at_unix_ms", integer()),
      ],
      &["id", "name", "version", "definition", "published_at_unix_ms"],
    ),
  );
  schemas.insert(
    "AgentPoolMutationResponse".to_owned(),
    mutation_response("AgentPoolResource"),
  );
  schemas.insert(
    "AgentPoolPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("AgentPoolResource"))),
        (
          "next_cursor",
          nullable(json!({"type": "string", "minLength": 1, "maxLength": MAX_CURSOR_BYTES})),
        ),
      ],
      &["items", "next_cursor"],
    ),
  );
  schemas.insert(
    "DeleteAgentPoolResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("pool_id", non_empty_string()),
      ],
      &["disposition", "pool_id"],
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
    "PublishProjectPolicyRequest".to_owned(),
    object([("policy", json!({"type": "object"}))], &["policy"]),
  );
  schemas.insert(
    "ProjectPolicyResource".to_owned(),
    object(
      [("project_id", non_empty_string()), ("version", positive_integer())],
      &["project_id", "version"],
    ),
  );
  schemas.insert(
    "ProjectPolicyMutationResponse".to_owned(),
    mutation_response("ProjectPolicyResource"),
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
        (
          "job_concurrency_limit",
          json!({"type": "integer", "minimum": 1, "maximum": 10000}),
        ),
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
        "job_concurrency_limit",
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
