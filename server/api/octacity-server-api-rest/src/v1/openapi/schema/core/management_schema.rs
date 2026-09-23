use super::*;

pub(super) fn insert_common_schemas(schemas: &mut Map<String, Value>) {
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

pub(super) fn insert_project_schemas(schemas: &mut Map<String, Value>) {
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

pub(super) fn insert_pipeline_schemas(schemas: &mut Map<String, Value>) {
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
