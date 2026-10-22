use super::super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let identity = || json!({"type":"string","format":"uuid"});
  schemas.insert(
    "IssueExecutionTarget".into(),
    object(
      [
        ("configuration_id", identity()),
        ("configuration_version", positive_integer()),
        ("repository_id", identity()),
        ("repository_version", positive_integer()),
        ("source", schema_ref("ManualSource")),
        ("mapping", string_enum(&["title_description_acceptance_v1"])),
      ],
      &[
        "configuration_id",
        "configuration_version",
        "repository_id",
        "repository_version",
        "source",
        "mapping",
      ],
    ),
  );
  schemas.insert(
    "RequestIssueExecutionRequest".into(),
    object(
      [
        ("target", schema_ref("IssueExecutionTarget")),
        ("classification", schema_ref("FactoryWorkClassification")),
      ],
      &["target", "classification"],
    ),
  );
  schemas.insert(
    "IssueExecutionMutationResponse".into(),
    object(
      [
        ("disposition", string_enum(&["applied", "replayed"])),
        ("resource", schema_ref("IssueExecutionResource")),
      ],
      &["disposition", "resource"],
    ),
  );
  schemas.insert(
    "IssueExecutionIntent".into(),
    object(
      [
        ("project_id", identity()),
        ("issue_id", identity()),
        ("expected_version", positive_integer()),
        ("target", schema_ref("IssueExecutionTarget")),
        ("classification", schema_ref("FactoryWorkClassification")),
      ],
      &["project_id", "issue_id", "expected_version", "target"],
    ),
  );
  schemas.insert(
    "IssueWorkAssociation".into(),
    object(
      [
        ("project_id", identity()),
        ("issue_id", identity()),
        ("request_id", nullable(identity())),
        ("requirements_version", nullable(positive_integer())),
        ("work_id", identity()),
        ("run_id", identity()),
      ],
      &[
        "project_id",
        "issue_id",
        "request_id",
        "requirements_version",
        "work_id",
        "run_id",
      ],
    ),
  );
  schemas.insert("IssueLegacyWorkHistoryPage".into(),object([
    ("items",json!({"type":"array","maxItems":octacity_server_application::MAX_ISSUE_HISTORY_PAGE_SIZE,"items":schema_ref("IssueWorkAssociation")})),
    ("next_cursor",nullable(json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_EXECUTION_CURSOR_BYTES}))),
    ("observation",schema_ref("FactoryDigest"))
  ],&["items","next_cursor","observation"]));
  schemas.insert(
    "IssueWorkExecutionProjection".into(),
    object(
      [
        ("association", schema_ref("IssueWorkAssociation")),
        ("run", schema_ref("IssueRunObservation")),
      ],
      &["association", "run"],
    ),
  );
  schemas.insert("IssueExecutionHistoryPage".into(),object([
    ("items",json!({"type":"array","maxItems":octacity_server_application::MAX_ISSUE_HISTORY_PAGE_SIZE,"items":schema_ref("IssueExecutionResource")})),
    ("next_cursor",nullable(json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_EXECUTION_CURSOR_BYTES}))),
    ("observation",string_enum(&["request-watermark-v1"])),("watermark",non_negative_integer()),
    ("issue_version",positive_integer())
  ],&["items","next_cursor","observation","watermark","issue_version"]));
  schemas.insert(
    "IssueExecutionResource".into(),
    object(
      [
        ("id", identity()),
        ("intent", schema_ref("IssueExecutionIntent")),
        ("task_digest", schema_ref("FactoryDigest")),
        ("task_size", positive_integer()),
        ("acceptance_digest", schema_ref("FactoryDigest")),
        ("acceptance_size", positive_integer()),
        ("title_digest", schema_ref("FactoryDigest")),
        ("description_digest", schema_ref("FactoryDigest")),
        ("requested_at", non_negative_integer()),
        ("association", nullable(schema_ref("IssueWorkAssociation"))),
        ("state", string_enum(&["pending", "admitted"])),
        (
          "run_url",
          nullable(json!({"type":"string","maxLength":128,"pattern":"^/api/v1/factory-runs/[0-9a-f-]{36}$"})),
        ),
      ],
      &[
        "id",
        "intent",
        "task_digest",
        "task_size",
        "acceptance_digest",
        "acceptance_size",
        "title_digest",
        "description_digest",
        "requested_at",
        "association",
        "state",
        "run_url",
      ],
    ),
  );
}
