use serde_json::{Map, Value, json};

use super::super::{array, nullable, object, schema_ref, string_enum};

pub(super) fn insert_operator_attention_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "OperatorAttentionCategory".to_owned(),
    string_enum(&["build", "agent", "agent_pool", "critical_system"]),
  );
  schemas.insert(
    "OperatorAttentionSeverity".to_owned(),
    string_enum(&["warning", "critical"]),
  );
  schemas.insert(
    "OperatorAttentionTargetKind".to_owned(),
    string_enum(&["build", "agent", "agent_pool"]),
  );
  schemas.insert(
    "OperatorAttentionTarget".to_owned(),
    object(
      [
        ("kind", schema_ref("OperatorAttentionTargetKind")),
        ("id", json!({"type": "string", "format": "uuid"})),
      ],
      &["kind", "id"],
    ),
  );
  schemas.insert(
    "OperatorAttentionItem".to_owned(),
    object(
      [
        ("id", json!({"type": "string", "format": "uuid"})),
        ("category", schema_ref("OperatorAttentionCategory")),
        ("severity", schema_ref("OperatorAttentionSeverity")),
        (
          "code",
          json!({"type": "string", "minLength": 1, "x-max-utf8-bytes": octacity_server_application::MAX_OPERATOR_ATTENTION_CODE_BYTES}),
        ),
        (
          "summary",
          json!({"type": "string", "minLength": 1, "x-max-utf8-bytes": octacity_server_application::MAX_OPERATOR_ATTENTION_SUMMARY_BYTES}),
        ),
        ("occurred_at_unix_ms", json!({"type": "integer", "format": "int64"})),
        (
          "resolved_at_unix_ms",
          nullable(json!({"type": "integer", "format": "int64"})),
        ),
        ("target", nullable(schema_ref("OperatorAttentionTarget"))),
      ],
      &[
        "id",
        "category",
        "severity",
        "code",
        "summary",
        "occurred_at_unix_ms",
        "resolved_at_unix_ms",
        "target",
      ],
    ),
  );
  schemas.insert(
    "OperatorAttentionPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("OperatorAttentionItem"))),
        (
          "next_cursor",
          nullable(json!({
            "type": "string",
            "minLength": 1,
            "maxLength": octacity_server_application::MAX_OPERATOR_ATTENTION_CURSOR_BYTES,
            "pattern": "^[A-Za-z0-9_-]+$"
          })),
        ),
      ],
      &["items", "next_cursor"],
    ),
  );
}
