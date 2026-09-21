use serde_json::{Map, Value, json};

use super::super::{array, integer, non_empty_string, object, positive_integer, schema_ref};

pub(super) fn insert_trigger_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "CreateManualTriggerDefinitionRequest".to_owned(),
    object(
      [
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("enabled", json!({"type": "boolean"})),
        ("definition", json!({"type": "object"})),
      ],
      &["configuration_id", "configuration_version", "enabled", "definition"],
    ),
  );
  schemas.insert(
    "TriggerDefinitionResource".to_owned(),
    object(
      [("id", non_empty_string()), ("version", positive_integer())],
      &["id", "version"],
    ),
  );
  schemas.insert(
    "TriggerDefinitionMutationResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("resource", schema_ref("TriggerDefinitionResource")),
      ],
      &["disposition", "resource"],
    ),
  );
  schemas.insert(
    "ManualSource".to_owned(),
    json!({
      "oneOf": [
        object([("kind", json!({"const": "default_reference"}))], &["kind"]),
        object(
          [("kind", json!({"const": "reference"})), ("value", non_empty_string())],
          &["kind", "value"]
        ),
        object(
          [("kind", json!({"const": "exact_revision"})), ("value", non_empty_string())],
          &["kind", "value"]
        )
      ],
      "discriminator": {"propertyName": "kind"}
    }),
  );
  schemas.insert(
    "AcceptManualTriggerRequest".to_owned(),
    object(
      [
        ("trigger_id", non_empty_string()),
        ("trigger_version", positive_integer()),
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("deduplication_identity", non_empty_string()),
        ("source", schema_ref("ManualSource")),
        (
          "parameters",
          json!({
            "type": "object",
            "additionalProperties": {"oneOf": [{"type": "string"}, {"type": "integer"}, {"type": "boolean"}]}
          }),
        ),
        ("priority", integer()),
      ],
      &[
        "trigger_id",
        "trigger_version",
        "configuration_id",
        "configuration_version",
        "deduplication_identity",
        "source",
        "parameters",
        "priority",
      ],
    ),
  );
  schemas.insert(
    "TriggerEvaluationResponse".to_owned(),
    json!({
      "oneOf": [
        object(
          [
            ("outcome", json!({"const": "accepted"})),
            ("disposition", schema_ref("MutationDisposition")),
            ("trigger_occurrence_id", non_empty_string()),
            ("build_id", non_empty_string()),
            ("attempt_id", non_empty_string()),
            ("ready_job_ids", array(non_empty_string()))
          ],
          &["outcome", "disposition", "trigger_occurrence_id", "build_id", "attempt_id", "ready_job_ids"]
        ),
        object(
          [
            ("outcome", json!({"const": "suppressed"})),
            ("disposition", schema_ref("MutationDisposition")),
            ("trigger_occurrence_id", non_empty_string())
          ],
          &["outcome", "disposition", "trigger_occurrence_id"]
        )
      ],
      "discriminator": {"propertyName": "outcome"}
    }),
  );
}
