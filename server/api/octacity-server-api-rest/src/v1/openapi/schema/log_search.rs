use serde_json::{Map, Value, json};

use super::super::{
  array, boolean, integer, non_empty_string, nullable, object, positive_integer, schema_ref, string_enum,
};

pub(super) fn insert_log_search_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert("BuildLogStream".to_owned(), string_enum(&["stdout", "stderr"]));
  schemas.insert("BuildLogSearchMode".to_owned(), string_enum(&["full_text", "literal"]));
  schemas.insert(
    "BuildLogSearchHit".to_owned(),
    object(
      [
        ("chunk_id", non_empty_string()),
        ("build_id", non_empty_string()),
        ("attempt_id", non_empty_string()),
        ("job_id", non_empty_string()),
        ("stream", schema_ref("BuildLogStream")),
        ("first_sequence", positive_integer()),
        ("last_sequence", positive_integer()),
        ("occurred_at_unix_ms", integer()),
        (
          "snippet",
          json!({
            "type": "string",
            "maxLength": octacity_server_application::MAX_LOG_SEARCH_SNIPPET_BYTES
          }),
        ),
      ],
      &[
        "chunk_id",
        "build_id",
        "attempt_id",
        "job_id",
        "stream",
        "first_sequence",
        "last_sequence",
        "occurred_at_unix_ms",
        "snippet",
      ],
    ),
  );
  schemas.insert(
    "BuildLogSearchFreshness".to_owned(),
    object(
      [
        ("indexed_through", nullable(positive_integer())),
        ("committed_through", nullable(positive_integer())),
        ("caught_up", boolean()),
      ],
      &["indexed_through", "committed_through", "caught_up"],
    ),
  );
  schemas.insert(
    "BuildLogSearchPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("BuildLogSearchHit"))),
        (
          "next_cursor",
          nullable(json!({
            "type": "string",
            "minLength": 1,
            "maxLength": super::super::MAX_CURSOR_BYTES
          })),
        ),
        ("freshness", schema_ref("BuildLogSearchFreshness")),
      ],
      &["items", "next_cursor", "freshness"],
    ),
  );
}
