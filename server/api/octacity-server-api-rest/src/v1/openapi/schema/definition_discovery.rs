use serde_json::{Map, Value, json};

use super::super::{
  MAX_CURSOR_BYTES, array, boolean, integer, non_empty_string, nullable, object, positive_integer, schema_ref,
  string_enum,
};

pub(super) fn insert_definition_discovery_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert("PipelineSummaryResource".to_owned(), named_definition_summary(false));
  schemas.insert(
    "PipelineSummaryPage".to_owned(),
    definition_summary_page("PipelineSummaryResource"),
  );
  schemas.insert("RepositorySummaryResource".to_owned(), named_definition_summary(false));
  schemas.insert(
    "RepositorySummaryPage".to_owned(),
    definition_summary_page("RepositorySummaryResource"),
  );
  schemas.insert(
    "BuildConfigurationSummaryResource".to_owned(),
    named_definition_summary(true),
  );
  schemas.insert(
    "BuildConfigurationSummaryPage".to_owned(),
    definition_summary_page("BuildConfigurationSummaryResource"),
  );
  schemas.insert(
    "TriggerDefinitionKind".to_owned(),
    string_enum(&["manual", "scheduled", "internal"]),
  );
  schemas.insert(
    "TriggerDefinitionSummaryResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("project_id", non_empty_string()),
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("version", positive_integer()),
        ("kind", schema_ref("TriggerDefinitionKind")),
        ("enabled", boolean()),
        ("published_at_unix_ms", integer()),
      ],
      &[
        "id",
        "project_id",
        "configuration_id",
        "configuration_version",
        "version",
        "kind",
        "enabled",
        "published_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "TriggerDefinitionSummaryPage".to_owned(),
    definition_summary_page("TriggerDefinitionSummaryResource"),
  );
}

fn named_definition_summary(include_enabled: bool) -> Value {
  let mut properties = Map::from_iter([
    ("id".to_owned(), non_empty_string()),
    ("project_id".to_owned(), non_empty_string()),
    ("name".to_owned(), non_empty_string()),
    ("version".to_owned(), positive_integer()),
    ("published_at_unix_ms".to_owned(), integer()),
  ]);
  let mut required = vec!["id", "project_id", "name", "version", "published_at_unix_ms"];
  if include_enabled {
    properties.insert("enabled".to_owned(), boolean());
    required.push("enabled");
  }
  json!({
    "type": "object",
    "additionalProperties": false,
    "properties": properties,
    "required": required,
  })
}

fn definition_summary_page(item_schema: &str) -> Value {
  object(
    [
      ("items", array(schema_ref(item_schema))),
      (
        "next_cursor",
        nullable(json!({"type": "string", "minLength": 1, "maxLength": MAX_CURSOR_BYTES})),
      ),
    ],
    &["items", "next_cursor"],
  )
}
