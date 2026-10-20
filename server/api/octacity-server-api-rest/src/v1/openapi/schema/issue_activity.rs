//! Safe references reuse existing metadata and common diagnostic schemas.
use super::super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let identity = || json!({"type":"string","format":"uuid"});
  schemas.insert(
    "IssueTimelineBoundary".into(),
    object(
      [
        ("issue_version", positive_integer()),
        ("executions", non_negative_integer()),
        ("discussion", non_negative_integer()),
        ("source", nullable(schema_ref("FactoryDigest"))),
      ],
      &["issue_version", "executions", "discussion", "source"],
    ),
  );
  schemas.insert("IssueTimelineReference".into(),json!({"oneOf":[
    object([("kind",json!({"type":"string","const":"revision"})),("at",non_negative_integer()),("version",positive_integer())],&["kind","at","version"]),
    object([("kind",json!({"type":"string","const":"status"})),("at",non_negative_integer()),("version",positive_integer()),("from",nullable(identity())),("to",identity()),("actor",schema_ref("AuditActor"))],&["kind","at","version","from","to","actor"]),
    object([("kind",json!({"type":"string","const":"execution"})),("at",non_negative_integer()),("request_id",identity()),("requirements_version",positive_integer())],&["kind","at","request_id","requirements_version"]),
    object([("kind",json!({"type":"string","const":"discussion"})),("comment",schema_ref("IssueCommentMetadata"))],&["kind","comment"]),
    object([("kind",json!({"type":"string","const":"source"})),("at",non_negative_integer()),("id",schema_ref("FactoryDigest")),("expires_at",non_negative_integer())],&["kind","at","id","expires_at"])
  ]}));
  let cursor = || {
    nullable(
      json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES}),
    )
  };
  schemas.insert("IssueTimelinePage".into(),object([
    ("items",json!({"type":"array","maxItems":octacity_server_application::MAX_ISSUE_HISTORY_PAGE_SIZE,"items":schema_ref("IssueTimelineReference")})),
    ("next_cursor",cursor()),("observation",json!({"type":"string","const":"issue-timeline-watermark-v1"})),("boundary",schema_ref("IssueTimelineBoundary")),("current_version",positive_integer()),("source_stale",boolean()),("source_unavailable",boolean()),("observed_at",non_negative_integer())
  ],&["items","next_cursor","observation","boundary","current_version","source_stale","source_unavailable","observed_at"]));
  schemas.insert(
    "IssueResultCoverage".into(),
    string_enum(&["partial", "stale", "unavailable"]),
  );
  let items = schemas["FactoryDiagnosticPage"]["properties"]["items"].clone();
  schemas.insert(
    "IssueResultPage".into(),
    object(
      [
        ("association", schema_ref("IssueWorkAssociation")),
        (
          "observation",
          json!({"type":"string","const":"issue-result-run-version-v1"}),
        ),
        ("boundary", schema_ref("IssueTimelineBoundary")),
        ("current_version", positive_integer()),
        ("source_stale", boolean()),
        ("source_unavailable", boolean()),
        ("run_version", nullable(positive_integer())),
        ("coverage", schema_ref("IssueResultCoverage")),
        ("items", items),
        ("next_cursor", cursor()),
      ],
      &[
        "association",
        "observation",
        "boundary",
        "current_version",
        "source_stale",
        "source_unavailable",
        "run_version",
        "coverage",
        "items",
        "next_cursor",
      ],
    ),
  );
}
