//! Safe Reporter read allowlist; private desired-state authority is never serialized here.
use super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = || json!({"type":"string","format":"uuid"});
  let digest = || schema_ref("FactoryDigest");
  let pin = || schema_ref("FactoryImmutableReferenceResource");
  schemas.insert("IssueSourceObservedStatus".into(), object([
    ("mapping",pin()), ("observation",digest()),
    ("state",json!({"type":"string","maxLength":octacity_server_application::MAX_WORK_SOURCE_PREDICATE_STATE_BYTES})),
    ("labels",json!({"type":"array","maxItems":octacity_server_application::MAX_SOURCE_REPORTING_EFFECT_LABELS,"items":{"type":"string","maxLength":octacity_server_application::MAX_SOURCE_REPORTING_TEXT_BYTES}})),
  ], &["mapping","observation","state","labels"]));
  schemas.insert("IssuePipelineActionProvenance".into(), object([
    ("issue_id",id()), ("requirements_version",positive_integer()),
    ("work_id",id()), ("run_id",id()), ("flow_run_id",id()), ("cycle_id",id()), ("node_attempt_id",id()),
    ("operation_id",digest()), ("candidate",nullable(schema_ref("FactoryCandidateSubject"))),
    ("evidence",json!({"type":"array","maxItems":256,"items":digest()})),
    ("outcome",json!({"type":"string","minLength":1,"maxLength":octacity_server_application::factory_configuration_document_bounds().key_bytes})),
  ], &["issue_id","requirements_version","work_id","run_id","flow_run_id","cycle_id","node_attempt_id","operation_id","candidate","outcome"]));
  schemas.insert(
    "IssueSourceProjection".into(),
    object(
      [
        ("operation_id", digest()),
        ("record_id", digest()),
        ("issue_version", positive_integer()),
        ("mapping", pin()),
        ("native_correlation", digest()),
        ("workflow", schema_ref("IssueWorkflowPin")),
        ("status_id", id()),
        ("effects", nullable(schema_ref("WorkSourceStatusEffects"))),
        ("pipeline", nullable(schema_ref("IssuePipelineActionProvenance"))),
        ("resolves", nullable(digest())),
        ("accepted_at", non_negative_integer()),
        ("state", string_enum(&["pending", "claimed", "delivered", "failed"])),
        (
          "failure",
          nullable(string_enum(&["conflict", "exhausted", "forbidden", "unsupported"])),
        ),
        ("attempt", json!({"type":"integer","minimum":0,"maximum":u16::MAX})),
        ("recorded_at", non_negative_integer()),
        ("available_at", non_negative_integer()),
      ],
      &[
        "operation_id",
        "record_id",
        "issue_version",
        "mapping",
        "native_correlation",
        "workflow",
        "status_id",
        "effects",
        "pipeline",
        "resolves",
        "accepted_at",
        "state",
        "failure",
        "attempt",
        "recorded_at",
        "available_at",
      ],
    ),
  );
  schemas.insert("IssueSourceProjectionPage".into(), object([
    ("items",json!({"type":"array","maxItems":octacity_server_application::MAX_ISSUE_HISTORY_PAGE_SIZE,"items":schema_ref("IssueSourceProjection")})),
    ("next_cursor",nullable(json!({"type":"string","minLength":1,"maxLength":1024}))),
    ("observation",json!({"const":"live-reporter-v1"})), ("observed_at",non_negative_integer()),
  ], &["items","next_cursor","observation","observed_at"]));
}
