use super::super::{non_negative_integer, object, positive_integer, schema_ref, string_enum};
use octacity_server_application::{
  MAX_ISSUE_WORKFLOW_NAME_BYTES, MAX_ISSUE_WORKFLOW_STATUSES, MAX_ISSUE_WORKFLOW_TRANSITIONS,
};
use serde_json::{Map, Value, json};
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "IssueWorkflowDefinitionRequest".into(),
    object([("definition", schema_ref("IssueWorkflowDefinition"))], &["definition"]),
  );
  schemas.insert(
    "IssueWorkflowMutationResponse".into(),
    super::super::mutation_response("IssueWorkflowResource"),
  );
  let identity = || json!({"type":"string","format":"uuid"});
  schemas.insert(
    "CreateIssueWorkflowRequest".into(),
    object(
      [
        ("id", identity()),
        ("definition", schema_ref("IssueWorkflowDefinition")),
      ],
      &["id", "definition"],
    ),
  );
  schemas.insert(
    "SelectDefaultIssueWorkflowRequest".into(),
    object(
      [("workflow_id", identity()), ("workflow_version", positive_integer())],
      &["workflow_id", "workflow_version"],
    ),
  );
  schemas.insert(
    "IssueWorkflowDefaultMutationResponse".into(),
    super::super::mutation_response("IssueWorkflowDefault"),
  );
  schemas.insert("IssueWorkflowSummaryPage".into(), object([
    ("items",json!({"type":"array","maxItems":octacity_server_application::MAX_CURRENT_DEFINITION_PAGE_SIZE,"items":schema_ref("IssueWorkflowSummary")})),
    ("next_cursor",json!({"type":["string","null"],"format":"uuid"}))], &["items","next_cursor"]));
  let name = json!({"type":"string","minLength":1,"maxLength":MAX_ISSUE_WORKFLOW_NAME_BYTES,
    "x-max-utf8-bytes":MAX_ISSUE_WORKFLOW_NAME_BYTES,"description":"Canonical UTF-8 label; no surrounding whitespace or control characters."});
  schemas.insert(
    "IssueWorkflowStatus".into(),
    object([("id", identity()), ("name", name.clone())], &["id", "name"]),
  );
  schemas.insert(
    "IssueWorkflowTransition".into(),
    object(
      [
        ("id", identity()),
        ("name", name.clone()),
        ("from", identity()),
        ("to", identity()),
        (
          "origins",
          json!({"type":"array","minItems":1,"maxItems":3,"uniqueItems":true,
      "items":string_enum(&["manual","pipeline","source"])}),
        ),
        (
          "evidence",
          json!({"type":"array","maxItems":2,"uniqueItems":true,
      "items":string_enum(&["accepted_execution","passed_required_checks"])}),
        ),
      ],
      &["id", "name", "from", "to", "origins", "evidence"],
    ),
  );
  schemas.insert("IssueWorkflowDefinition".into(), object([
    ("name",name.clone()),
    ("statuses",json!({"type":"array","minItems":1,"maxItems":MAX_ISSUE_WORKFLOW_STATUSES,"items":schema_ref("IssueWorkflowStatus")})),
    ("initial_status_id",identity()),
    ("terminal_status_ids",json!({"type":"array","maxItems":MAX_ISSUE_WORKFLOW_STATUSES,"uniqueItems":true,"items":identity()})),
    ("transitions",json!({"type":"array","maxItems":MAX_ISSUE_WORKFLOW_TRANSITIONS,"items":schema_ref("IssueWorkflowTransition")}))],
    &["name","statuses","initial_status_id","terminal_status_ids","transitions"]));
  schemas.insert(
    "IssueWorkflowSummary".into(),
    object(
      [
        ("project_id", identity()),
        ("id", identity()),
        ("version", positive_integer()),
        ("name", name),
        ("published_at_unix_ms", non_negative_integer()),
      ],
      &["project_id", "id", "version", "name", "published_at_unix_ms"],
    ),
  );
  schemas.insert(
    "IssueWorkflowResource".into(),
    object(
      [
        ("project_id", identity()),
        ("id", identity()),
        ("version", positive_integer()),
        ("definition", schema_ref("IssueWorkflowDefinition")),
        ("published_at_unix_ms", non_negative_integer()),
      ],
      &["project_id", "id", "version", "definition", "published_at_unix_ms"],
    ),
  );
  schemas.insert(
    "IssueWorkflowDefault".into(),
    object(
      [
        ("version", positive_integer()),
        ("workflow", schema_ref("IssueWorkflowResource")),
      ],
      &["version", "workflow"],
    ),
  );
}
