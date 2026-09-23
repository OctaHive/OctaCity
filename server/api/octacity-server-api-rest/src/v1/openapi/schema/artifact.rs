use serde_json::{Map, Value, json};

use super::super::{array, integer, non_empty_string, non_negative_integer, object, schema_ref};

pub(super) fn insert_artifact_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "ArtifactOutputType".to_owned(),
    json!({
      "oneOf": [
        object([("kind", json!({"const": "artifact"}))], &["kind"]),
        object(
          [("kind", json!({"const": "report"})), ("format", non_empty_string())],
          &["kind", "format"]
        )
      ]
    }),
  );
  schemas.insert(
    "ArtifactResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("build_id", non_empty_string()),
        ("attempt_id", non_empty_string()),
        ("job_id", non_empty_string()),
        ("name", non_empty_string()),
        ("output_type", schema_ref("ArtifactOutputType")),
        ("media_type", non_empty_string()),
        ("size_bytes", non_negative_integer()),
        ("sha256", json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})),
        ("published_at_unix_ms", integer()),
      ],
      &[
        "id",
        "build_id",
        "attempt_id",
        "job_id",
        "name",
        "output_type",
        "media_type",
        "size_bytes",
        "sha256",
        "published_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "ArtifactPage".to_owned(),
    object([("items", array(schema_ref("ArtifactResource")))], &["items"]),
  );
  schemas.insert(
    "ArtifactDownload".to_owned(),
    object(
      [
        ("artifact", schema_ref("ArtifactResource")),
        ("get_url", json!({"type": "string", "minLength": 1, "maxLength": 8192})),
        ("expires_at_unix_ms", integer()),
      ],
      &["artifact", "get_url", "expires_at_unix_ms"],
    ),
  );
}
