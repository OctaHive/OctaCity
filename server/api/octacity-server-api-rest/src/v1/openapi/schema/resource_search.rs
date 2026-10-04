use serde_json::{Map, Value, json};

use super::super::{array, non_empty_string, nullable, object, schema_ref, string_enum};

pub(super) fn insert_resource_search_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "ResourceSearchKind".to_owned(),
    string_enum(&["project", "build", "agent", "agent_pool"]),
  );
  schemas.insert(
    "ResourceSearchResult".to_owned(),
    object(
      [
        ("kind", schema_ref("ResourceSearchKind")),
        ("id", non_empty_string()),
        (
          "label",
          json!({
            "type": "string",
            "minLength": 1,
            "x-max-utf8-bytes": octacity_server_application::MAX_RESOURCE_SEARCH_RESULT_LABEL_BYTES
          }),
        ),
        (
          "context",
          nullable(json!({
            "type": "string",
            "minLength": 1,
            "x-max-utf8-bytes": octacity_server_application::MAX_RESOURCE_SEARCH_RESULT_CONTEXT_BYTES
          })),
        ),
      ],
      &["kind", "id", "label", "context"],
    ),
  );
  schemas.insert(
    "ResourceSearchPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("ResourceSearchResult"))),
        (
          "next_cursor",
          nullable(json!({
            "type": "string",
            "minLength": 1,
            "maxLength": octacity_server_application::MAX_RESOURCE_SEARCH_CURSOR_BYTES,
            "pattern": "^[A-Za-z0-9_-]+$"
          })),
        ),
      ],
      &["items", "next_cursor"],
    ),
  );
}
