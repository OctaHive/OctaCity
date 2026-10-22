use super::super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  schemas.insert("IssueContentRetentionScope".into(),json!({"oneOf":[
    object([("kind",string_enum(&["requirements"])),("through",positive_integer())],&["kind","through"]),
    object([("kind",string_enum(&["discussion"])),("through",json!({"type":"integer","minimum":1,"maximum":octacity_server_application::MAX_ISSUE_DISCUSSION_COMMENTS}))],&["kind","through"])
  ]}));
  schemas.insert("IssueContentHold".into(),object([("reason",json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_TITLE_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_ISSUE_TITLE_BYTES})),("expires_at",nullable(non_negative_integer()))],&["reason","expires_at"]));
  schemas.insert(
    "IssueContentRetentionPlan".into(),
    object(
      [
        ("scope", schema_ref("IssueContentRetentionScope")),
        ("expires_at", non_negative_integer()),
        ("hold", nullable(schema_ref("IssueContentHold"))),
      ],
      &["scope", "expires_at", "hold"],
    ),
  );
  schemas.insert(
    "SetIssueContentRetentionRequest".into(),
    object(
      [
        ("expected_version", non_negative_integer()),
        ("plan", schema_ref("IssueContentRetentionPlan")),
      ],
      &["expected_version", "plan"],
    ),
  );
  for (name, version, plan) in [
    (
      "IssueContentRetentionState",
      positive_integer(),
      schema_ref("IssueContentRetentionPlan"),
    ),
    (
      "IssueContentRetentionObservation",
      non_negative_integer(),
      nullable(schema_ref("IssueContentRetentionPlan")),
    ),
  ] {
    schemas.insert(
      name.into(),
      object(
        [
          ("project_id", json!({"type":"string","format":"uuid"})),
          ("issue_id", json!({"type":"string","format":"uuid"})),
          ("version", version),
          ("plan", plan),
        ],
        &["project_id", "issue_id", "version", "plan"],
      ),
    );
  }

  schemas.insert(
    "IssueRevisionHeader".into(),
    object(
      [
        ("version", positive_integer()),
        ("applicability_version", positive_integer()),
        ("updated_at_unix_ms", non_negative_integer()),
      ],
      &["version", "applicability_version", "updated_at_unix_ms"],
    ),
  );
  schemas.insert("IssueRevisionPage".into(),object([("items",json!({"type":"array","maxItems":octacity_server_application::MAX_CURRENT_DEFINITION_PAGE_SIZE,"items":schema_ref("IssueRevisionHeader")})),("next_cursor",nullable(positive_integer()))], &["items","next_cursor"]));
  let identity = || json!({"type":"string","format":"uuid"});
  schemas.insert(
    "IssueWorkflowPin".into(),
    object(
      [("id", identity()), ("version", positive_integer())],
      &["id", "version"],
    ),
  );
  let fields = || json!({"type":"array","maxItems":10,"uniqueItems":true,"items":string_enum(&["title","description","acceptance","priority","status","assignee","labels","due_date","properties","parent"])});
  schemas.insert(
    "IssuePushPolicy".into(),
    object(
      [
        ("mapping", schema_ref("FactoryImmutableReferenceResource")),
        ("fields", fields()),
      ],
      &["mapping", "fields"],
    ),
  );
  schemas.insert(
    "IssueFieldPolicy".into(),
    object(
      [
        ("source_owned", fields()),
        ("status_mapping_workflow", nullable(schema_ref("IssueWorkflowPin"))),
        ("push", nullable(schema_ref("IssuePushPolicy"))),
      ],
      &["source_owned", "status_mapping_workflow", "push"],
    ),
  );
  schemas.insert(
    "IssueMoveBlockReason".into(),
    string_enum(&[
      "permission_required",
      "source_owned",
      "origin_not_allowed",
      "evidence_required",
    ]),
  );
  schemas.insert(
    "IssueTransitionChoice".into(),
    object(
      [
        ("transition", schema_ref("IssueWorkflowTransition")),
        ("available", boolean()),
        ("reason", nullable(schema_ref("IssueMoveBlockReason"))),
      ],
      &["transition", "available", "reason"],
    ),
  );
  schemas.insert("IssueTransitionChoices".into(),object([("issue_version",positive_integer()),("workflow_id",identity()),("workflow_version",positive_integer()),("status_id",identity()),("field_policy",schema_ref("IssueFieldPolicy")),("items",json!({"type":"array","maxItems":octacity_server_application::MAX_ISSUE_WORKFLOW_TRANSITIONS,"items":schema_ref("IssueTransitionChoice")}))],&["issue_version","workflow_id","workflow_version","status_id","field_policy","items"]));
  schemas.insert(
    "TransitionIssueRequest".into(),
    object(
      [
        ("workflow_id", identity()),
        ("workflow_version", positive_integer()),
        ("expected_status_id", identity()),
        ("transition_id", identity()),
        (
          "reason",
          nullable(
            json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_TITLE_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_ISSUE_TITLE_BYTES}),
          ),
        ),
      ],
      &["workflow_id", "workflow_version", "expected_status_id", "transition_id"],
    ),
  );
  schemas.insert(
    "IssueStatusHistoryEntry".into(),
    object(
      [
        ("issue_id", identity()),
        ("version", positive_integer()),
        ("workflow_id", identity()),
        ("workflow_version", positive_integer()),
        ("from", nullable(identity())),
        ("to", identity()),
        ("transition_id", nullable(identity())),
        ("previous_workflow", nullable(schema_ref("IssueWorkflowPin"))),
        ("origin", string_enum(&["manual", "pipeline", "source"])),
        (
          "reason",
          nullable(json!({"type":"string","maxLength":octacity_server_application::MAX_ISSUE_TITLE_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_ISSUE_TITLE_BYTES})),
        ),
        ("actor", schema_ref("AuditActor")),
        ("occurred_at", non_negative_integer()),
      ],
      &[
        "issue_id",
        "version",
        "workflow_id",
        "workflow_version",
        "from",
        "to",
        "transition_id",
        "previous_workflow",
        "origin",
        "reason",
        "actor",
        "occurred_at",
      ],
    ),
  );
  schemas.insert("IssueStatusHistoryPage".into(),object([("items",json!({"type":"array","maxItems":octacity_server_application::MAX_CURRENT_DEFINITION_PAGE_SIZE,"items":schema_ref("IssueStatusHistoryEntry")})),("next_cursor",nullable(positive_integer()))],&["items","next_cursor"]));
  schemas.insert(
    "IssueTransitionResponse".into(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("resource", schema_ref("IssueResource")),
        ("transition", schema_ref("IssueStatusHistoryEntry")),
      ],
      &["disposition", "resource", "transition"],
    ),
  );
  schemas.insert("AdoptIssueWorkflowRequest".into(),object([("expected_workflow_id",identity()),("expected_workflow_version",positive_integer()),("expected_status_id",identity()),("target_workflow_id",identity()),("target_workflow_version",positive_integer()),("status_correspondence",json!({"type":"object","minProperties":1,"maxProperties":octacity_server_application::MAX_ISSUE_WORKFLOW_STATUSES,"propertyNames":{"format":"uuid"},"additionalProperties":identity()})),("expected_field_policy",schema_ref("IssueFieldPolicy"))],&["expected_workflow_id","expected_workflow_version","expected_status_id","target_workflow_id","target_workflow_version","status_correspondence","expected_field_policy"]));
  schemas.insert("IssueContent".into(),object([
    ("title",json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_TITLE_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_ISSUE_TITLE_BYTES})),
    ("description",json!({"type":"string","maxLength":octacity_server_application::MAX_ISSUE_DOCUMENT_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_ISSUE_DOCUMENT_BYTES})),
    ("acceptance",nullable(json!({"type":"string","maxLength":octacity_server_application::MAX_ISSUE_DOCUMENT_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_ISSUE_DOCUMENT_BYTES}))),
    ("priority",nullable(json!({"type":"integer","minimum":0,"maximum":255})))
  ], &["title","description"]));
  schemas.insert(
    "IssueResource".into(),
    object(
      [
        ("id", identity()),
        ("project_id", identity()),
        ("number", positive_integer()),
        ("display_key", json!({"type":"string","pattern":"^ISS-[1-9][0-9]*$"})),
        ("version", positive_integer()),
        ("applicability_version", positive_integer()),
        ("content_available", json!({"type":"boolean","default":true,"description":"False when retained task text has expired; identity and execution provenance remain available."})),
        ("content", schema_ref("IssueContent")),
        ("workflow_id", identity()),
        ("workflow_version", positive_integer()),
        ("status_id", identity()),
        ("created_at_unix_ms", non_negative_integer()),
        ("updated_at_unix_ms", non_negative_integer()),
      ],
      &[
        "id",
        "project_id",
        "number",
        "display_key",
        "version",
        "applicability_version",
        "content",
        "workflow_id",
        "workflow_version",
        "status_id",
        "created_at_unix_ms",
        "updated_at_unix_ms",
      ],
    ),
  );
  schemas.insert("IssueMutationResponse".into(), mutation_response("IssueResource"));
  schemas.insert(
    "IssueSummary".into(),
    object(
      [
        ("id", identity()),
        ("project_id", identity()),
        ("display_key", non_empty_string()),
        ("version", positive_integer()),
        (
          "title",
          json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_TITLE_BYTES}),
        ),
        ("workflow_id", identity()),
        ("workflow_version", positive_integer()),
        ("status_id", identity()),
        ("created_at_unix_ms", non_negative_integer()),
        ("updated_at_unix_ms", non_negative_integer()),
      ],
      &[
        "id",
        "project_id",
        "display_key",
        "version",
        "title",
        "workflow_id",
        "workflow_version",
        "status_id",
        "created_at_unix_ms",
        "updated_at_unix_ms",
      ],
    ),
  );
  schemas.insert("IssueSummaryPage".into(),object([("items",json!({"type":"array","maxItems":octacity_server_application::MAX_CURRENT_DEFINITION_PAGE_SIZE,"items":schema_ref("IssueSummary")})),("next_cursor",nullable(identity()))], &["items","next_cursor"]));
}
