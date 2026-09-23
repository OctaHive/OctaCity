use serde_json::{Map, Value, json};

use super::super::{
  MAX_SCHEDULE_CATCH_UP, MAX_SCHEDULE_EXPRESSION_BYTES, MAX_SCHEDULE_TIMEZONE_BYTES, MAX_WEBHOOK_VERIFICATION_HEADERS,
  array, integer, non_empty_string, object, positive_integer, schema_ref,
};

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
    "MissedRunPolicy".to_owned(),
    json!({
      "oneOf": [
        object([("kind", json!({"const": "run_once"}))], &["kind"]),
        object(
          [
            ("kind", json!({"const": "catch_up"})),
            ("maximum_occurrences", json!({"type": "integer", "minimum": 1, "maximum": MAX_SCHEDULE_CATCH_UP}))
          ],
          &["kind", "maximum_occurrences"]
        )
      ],
      "discriminator": {"propertyName": "kind"}
    }),
  );
  schemas.insert(
    "ScheduleDefinition".to_owned(),
    object(
      [
        (
          "expression",
          json!({"type": "string", "minLength": 1, "maxLength": MAX_SCHEDULE_EXPRESSION_BYTES}),
        ),
        (
          "timezone",
          json!({"type": "string", "minLength": 1, "maxLength": MAX_SCHEDULE_TIMEZONE_BYTES}),
        ),
        ("missed_run_policy", schema_ref("MissedRunPolicy")),
      ],
      &["expression", "timezone", "missed_run_policy"],
    ),
  );
  schemas.insert(
    "ScheduledBuildDefinition".to_owned(),
    object(
      [
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
      &["source", "parameters", "priority"],
    ),
  );
  schemas.insert(
    "CreateScheduledTriggerDefinitionRequest".to_owned(),
    object(
      [
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("enabled", json!({"type": "boolean"})),
        ("schedule", schema_ref("ScheduleDefinition")),
        ("build", schema_ref("ScheduledBuildDefinition")),
      ],
      &[
        "configuration_id",
        "configuration_version",
        "enabled",
        "schedule",
        "build",
      ],
    ),
  );
  schemas.insert(
    "ScheduleResource".to_owned(),
    object(
      [
        ("trigger_id", non_empty_string()),
        ("trigger_version", positive_integer()),
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("enabled", json!({"type": "boolean"})),
        ("schedule", schema_ref("ScheduleDefinition")),
        ("next_occurrence_at_unix_ms", integer()),
        ("build", schema_ref("ScheduledBuildDefinition")),
      ],
      &[
        "trigger_id",
        "trigger_version",
        "configuration_id",
        "configuration_version",
        "enabled",
        "schedule",
        "next_occurrence_at_unix_ms",
        "build",
      ],
    ),
  );
  schemas.insert(
    "CreateUnmanagedWebhookRequest".to_owned(),
    object(
      [
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("enabled", json!({"type": "boolean"})),
        ("adapter_id", non_empty_string()),
        ("adapter_sha256", json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})),
        ("verification_material_handle", non_empty_string()),
        (
          "verification_headers",
          json!({
            "type": "array",
            "minItems": 1,
            "maxItems": MAX_WEBHOOK_VERIFICATION_HEADERS,
            "uniqueItems": true,
            "items": {"type": "string", "pattern": "^[a-z0-9-]+$"}
          }),
        ),
        ("repository_id", non_empty_string()),
        ("event_kind", non_empty_string()),
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
        "configuration_id",
        "configuration_version",
        "enabled",
        "adapter_id",
        "adapter_sha256",
        "verification_material_handle",
        "verification_headers",
        "repository_id",
        "event_kind",
        "parameters",
        "priority",
      ],
    ),
  );
  schemas.insert(
    "WebhookVerificationRequirements".to_owned(),
    object(
      [
        ("adapter_id", non_empty_string()),
        ("adapter_sha256", json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})),
        ("required_headers", array(non_empty_string())),
        ("protected_material_required", json!({"type": "boolean"})),
      ],
      &[
        "adapter_id",
        "adapter_sha256",
        "required_headers",
        "protected_material_required",
      ],
    ),
  );
  schemas.insert(
    "UnmanagedWebhookResource".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("integration_id", non_empty_string()),
        ("trigger", schema_ref("TriggerDefinitionResource")),
        ("callback_url", non_empty_string()),
        ("verification", schema_ref("WebhookVerificationRequirements")),
      ],
      &[
        "disposition",
        "integration_id",
        "trigger",
        "callback_url",
        "verification",
      ],
    ),
  );
  schemas.insert(
    "CreateManagedWebhookRequest".to_owned(),
    object(
      [
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("enabled", json!({"type": "boolean"})),
        ("adapter_id", non_empty_string()),
        ("adapter_sha256", json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})),
        ("verification_material_handle", non_empty_string()),
        (
          "verification_headers",
          json!({
            "type": "array",
            "minItems": 1,
            "maxItems": MAX_WEBHOOK_VERIFICATION_HEADERS,
            "uniqueItems": true,
            "items": {"type": "string", "pattern": "^[a-z0-9-]+$"}
          }),
        ),
        ("administration_credential_handle", non_empty_string()),
        ("repository_id", non_empty_string()),
        ("event_kind", non_empty_string()),
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
        "configuration_id",
        "configuration_version",
        "enabled",
        "adapter_id",
        "adapter_sha256",
        "verification_material_handle",
        "verification_headers",
        "administration_credential_handle",
        "repository_id",
        "event_kind",
        "parameters",
        "priority",
      ],
    ),
  );
  schemas.insert(
    "ManagedWebhookRegistrationStatus".to_owned(),
    json!({"type": "string", "enum": ["active", "disabled", "missing"]}),
  );
  schemas.insert(
    "ManagedWebhookRegistrationResource".to_owned(),
    object(
      [
        ("registration_id", non_empty_string()),
        ("status", schema_ref("ManagedWebhookRegistrationStatus")),
        ("callback_url", non_empty_string()),
      ],
      &["registration_id", "status", "callback_url"],
    ),
  );
  schemas.insert(
    "ManagedWebhookResource".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("integration_id", non_empty_string()),
        ("trigger", schema_ref("TriggerDefinitionResource")),
        ("callback_url", non_empty_string()),
        ("registration", schema_ref("ManagedWebhookRegistrationResource")),
      ],
      &[
        "disposition",
        "integration_id",
        "trigger",
        "callback_url",
        "registration",
      ],
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
