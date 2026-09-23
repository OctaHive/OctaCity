use serde_json::{Map, Value};

use super::super::{
  array, boolean, integer, non_empty_string, non_negative_integer, nullable, object, positive_integer, schema_ref,
  string_enum,
};

pub(super) fn insert_cache_session_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "CacheSessionState".to_owned(),
    string_enum(&["active", "revoked", "expired", "fenced"]),
  );
  schemas.insert(
    "CacheSessionResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("project_id", non_empty_string()),
        ("build_id", non_empty_string()),
        ("job_id", non_empty_string()),
        ("agent_id", non_empty_string()),
        ("registration_epoch", positive_integer()),
        ("lease_id", non_empty_string()),
        ("namespace", non_empty_string()),
        ("read", boolean()),
        ("write", boolean()),
        ("quota_bytes", non_negative_integer()),
        ("created_at_unix_ms", integer()),
        ("expires_at_unix_ms", integer()),
        ("retention_until_unix_ms", integer()),
        ("state", schema_ref("CacheSessionState")),
        ("revoked_at_unix_ms", nullable(integer())),
      ],
      &[
        "id",
        "project_id",
        "build_id",
        "job_id",
        "agent_id",
        "registration_epoch",
        "lease_id",
        "namespace",
        "read",
        "write",
        "quota_bytes",
        "created_at_unix_ms",
        "expires_at_unix_ms",
        "retention_until_unix_ms",
        "state",
        "revoked_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "CacheSessionPage".to_owned(),
    object([("items", array(schema_ref("CacheSessionResource")))], &["items"]),
  );
}
