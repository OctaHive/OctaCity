use serde_json::{Map, Value, json};

use super::super::{array, integer, non_empty_string, nullable, object, schema_ref, string_enum};

pub(super) fn insert_audit_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "AuditActorKind".to_owned(),
    string_enum(&[
      "unauthenticated_management",
      "agent",
      "trigger",
      "orchestrator",
      "adapter",
      "worker",
    ]),
  );
  schemas.insert("AuditOutcome".to_owned(), string_enum(&["accepted"]));
  schemas.insert(
    "AuditActor".to_owned(),
    object(
      [
        ("kind", schema_ref("AuditActorKind")),
        ("identity", nullable(non_empty_string())),
      ],
      &["kind", "identity"],
    ),
  );
  schemas.insert(
    "AuditFactResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("actor", schema_ref("AuditActor")),
        ("operation", non_empty_string()),
        ("target_kind", non_empty_string()),
        ("target_identity", non_empty_string()),
        ("request_identity", nullable(non_empty_string())),
        ("idempotency_key", nullable(non_empty_string())),
        ("outcome", schema_ref("AuditOutcome")),
        ("metadata", json!({"type": "object"})),
        ("occurred_at_unix_ms", integer()),
      ],
      &[
        "id",
        "actor",
        "operation",
        "target_kind",
        "target_identity",
        "request_identity",
        "idempotency_key",
        "outcome",
        "metadata",
        "occurred_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "AuditFactPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("AuditFactResource"))),
        ("next_cursor", nullable(schema_ref("Cursor"))),
      ],
      &["items", "next_cursor"],
    ),
  );
}
