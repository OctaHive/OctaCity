//! Bounded display-only cards and the available and expired reference variants.
use super::super::*;
use octacity_server_application::{
  ISSUE_OUTCOME_DUPLICATE_COVERAGE, ISSUE_OUTCOME_REVIEW_VERDICTS, ISSUE_OUTCOME_TRIAGE_DECISIONS,
  MAX_ISSUE_OUTCOME_CARDS, MAX_ISSUE_OUTCOME_CONCLUSION_BYTES, MAX_ISSUE_OUTCOME_REFERENCES,
  MAX_ISSUE_OUTCOME_TITLE_BYTES, MAX_WORK_PRIORITY,
};

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = || json!({"type":"string","format":"uuid"});
  let bounded_text = |max| json!({"type":"string","minLength":1,"maxLength":max,"x-max-utf8-bytes":max});
  let kind = |value: &str| json!({"type":"string","const":value});
  schemas.insert(
    "IssueOutcomePresentation".into(),
    json!({"oneOf":[
      object([
        ("kind", kind("triage")),
        ("decision", string_enum(ISSUE_OUTCOME_TRIAGE_DECISIONS)),
        ("duplicate_coverage", string_enum(ISSUE_OUTCOME_DUPLICATE_COVERAGE)),
        ("recommended_priority", json!({"type":"integer","minimum":u8::MIN,"maximum":MAX_WORK_PRIORITY})),
        ("recommended_size", non_empty_string()),
        ("component", non_empty_string()),
      ], &["kind","decision"]),
      object([("kind", kind("specifications"))], &["kind"]),
      object([("kind", kind("review")), ("verdict", string_enum(ISSUE_OUTCOME_REVIEW_VERDICTS))], &["kind","verdict"]),
      object([("kind", kind("custom")), ("title", bounded_text(MAX_ISSUE_OUTCOME_TITLE_BYTES))], &["kind","title"]),
    ]}),
  );
  let artifact = |value| {
    object(
      [
        ("kind", kind(value)),
        ("artifact_id", id()),
        ("title", non_empty_string()),
      ],
      &["kind", "artifact_id"],
    )
  };
  schemas.insert("IssueOutcomeLink".into(), json!({"oneOf":[
    artifact("document"),
    artifact("artifact"),
    object([("kind", kind("work_artifact")), ("work_id", id()), ("artifact_id", id()), ("title", non_empty_string())], &["kind","work_id","artifact_id"]),
    object([("kind", kind("review")), ("review_id", schema_ref("FactoryDigest")), ("title", non_empty_string())], &["kind","review_id"]),
    object([("kind", kind("unavailable")), ("title", non_empty_string())], &["kind"]),
  ]}));
  schemas.insert(
    "IssueOutcomeCard".into(),
    object(
      [
        ("slot", non_empty_string()),
        ("conclusion", bounded_text(MAX_ISSUE_OUTCOME_CONCLUSION_BYTES)),
        ("run_id", id()),
        ("presentation", schema_ref("IssueOutcomePresentation")),
        (
          "links",
          json!({"type":"array","maxItems":MAX_ISSUE_OUTCOME_REFERENCES,"items":schema_ref("IssueOutcomeLink")}),
        ),
      ],
      &["slot", "conclusion", "run_id", "presentation", "links"],
    ),
  );
  schemas.insert(
    "IssueOutcomes".into(),
    object(
      [(
        "items",
        json!({"type":"array","maxItems":MAX_ISSUE_OUTCOME_CARDS,"items":schema_ref("IssueOutcomeCard")}),
      )],
      &["items"],
    ),
  );
}
