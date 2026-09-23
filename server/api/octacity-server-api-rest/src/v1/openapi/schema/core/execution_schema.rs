use super::*;

pub(super) fn insert_execution_schemas(schemas: &mut Map<String, Value>) {
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
