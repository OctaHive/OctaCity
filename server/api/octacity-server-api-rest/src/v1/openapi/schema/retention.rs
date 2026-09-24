use serde_json::{Map, Value, json};

use octacity_server_application::MAX_BUILD_RESULT_HOLD_REASON_BYTES;

use super::super::{boolean, integer, non_empty_string, nullable, object, positive_integer, schema_ref, string_enum};

pub(super) fn insert_retention_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "PlaceBuildResultHoldRequest".to_owned(),
    object(
      [
        (
          "reason",
          json!({
            "type": "string",
            "minLength": 1,
            "description": "Operator reason bounded by UTF-8 encoded size",
            "x-max-utf8-bytes": MAX_BUILD_RESULT_HOLD_REASON_BYTES
          }),
        ),
        ("expires_at_unix_ms", nullable(integer())),
      ],
      &["reason"],
    ),
  );
  schemas.insert(
    "BuildResultRetentionDeadlines".to_owned(),
    object(
      [
        ("metadata_at_unix_ms", integer()),
        ("logs_at_unix_ms", integer()),
        ("artifacts_at_unix_ms", integer()),
        ("reports_at_unix_ms", integer()),
      ],
      &[
        "metadata_at_unix_ms",
        "logs_at_unix_ms",
        "artifacts_at_unix_ms",
        "reports_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "BuildResultVisibility".to_owned(),
    object(
      [
        ("metadata", boolean()),
        ("logs", boolean()),
        ("artifacts", boolean()),
        ("reports", boolean()),
      ],
      &["metadata", "logs", "artifacts", "reports"],
    ),
  );
  schemas.insert(
    "RetentionAuditIdentity".to_owned(),
    object(
      [
        ("actor_kind", non_empty_string()),
        ("actor_identity", nullable(non_empty_string())),
        ("request_identity", non_empty_string()),
      ],
      &["actor_kind", "actor_identity", "request_identity"],
    ),
  );
  schemas.insert(
    "BuildResultHoldResource".to_owned(),
    object(
      [
        ("version", positive_integer()),
        (
          "reason",
          json!({
            "type": "string",
            "minLength": 1,
            "description": "Operator reason bounded by UTF-8 encoded size",
            "x-max-utf8-bytes": MAX_BUILD_RESULT_HOLD_REASON_BYTES
          }),
        ),
        ("created_at_unix_ms", integer()),
        ("expires_at_unix_ms", nullable(integer())),
        ("released_at_unix_ms", nullable(integer())),
        ("state", string_enum(&["active", "released", "expired"])),
        ("creation_audit", schema_ref("RetentionAuditIdentity")),
        ("release_audit", nullable(schema_ref("RetentionAuditIdentity"))),
      ],
      &[
        "version",
        "reason",
        "created_at_unix_ms",
        "expires_at_unix_ms",
        "released_at_unix_ms",
        "state",
        "creation_audit",
        "release_audit",
      ],
    ),
  );
  schemas.insert(
    "BuildResultRetentionResource".to_owned(),
    object(
      [
        ("build_id", non_empty_string()),
        ("deadlines", schema_ref("BuildResultRetentionDeadlines")),
        ("visibility", schema_ref("BuildResultVisibility")),
        ("hold", nullable(schema_ref("BuildResultHoldResource"))),
      ],
      &["build_id", "deadlines", "visibility", "hold"],
    ),
  );
  schemas.insert(
    "BuildResultRetentionMutationResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("retention", schema_ref("BuildResultRetentionResource")),
      ],
      &["disposition", "retention"],
    ),
  );
}
