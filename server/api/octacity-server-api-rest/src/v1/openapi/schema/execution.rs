use serde_json::{Map, Value, json};

use super::super::{
  array, boolean, integer, non_empty_string, non_negative_integer, nullable, object, positive_integer, schema_ref,
  string_enum, string_map, unique_array,
};

pub(super) fn insert_execution_detail_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "BuildParameters".to_owned(),
    json!({
      "type": "object",
      "additionalProperties": {"oneOf": [{"type": "string"}, {"type": "integer"}, {"type": "boolean"}]}
    }),
  );
  schemas.insert(
    "PolicySource".to_owned(),
    object(
      [("project_id", non_empty_string()), ("version", positive_integer())],
      &["project_id", "version"],
    ),
  );
  schemas.insert(
    "EffectiveCachePolicy".to_owned(),
    object(
      [
        ("namespaces", unique_array(non_empty_string())),
        ("read", boolean()),
        ("write", boolean()),
        ("max_bytes", non_negative_integer()),
      ],
      &["namespaces", "read", "write", "max_bytes"],
    ),
  );
  schemas.insert(
    "EffectiveConcurrencyPolicy".to_owned(),
    object(
      [
        ("active_builds", non_negative_integer()),
        ("active_jobs", non_negative_integer()),
      ],
      &["active_builds", "active_jobs"],
    ),
  );
  schemas.insert(
    "EffectiveRetentionPolicy".to_owned(),
    object(
      [
        ("build_seconds", non_negative_integer()),
        ("log_seconds", non_negative_integer()),
        ("artifact_seconds", non_negative_integer()),
        ("cache_seconds", non_negative_integer()),
      ],
      &["build_seconds", "log_seconds", "artifact_seconds", "cache_seconds"],
    ),
  );
  schemas.insert(
    "EffectiveProjectPolicyValue".to_owned(),
    object(
      [
        ("pools", unique_array(non_empty_string())),
        ("repositories", unique_array(non_empty_string())),
        ("secret_profiles", unique_array(non_empty_string())),
        ("identity_profiles", unique_array(non_empty_string())),
        (
          "runtimes",
          unique_array(string_enum(&["native", "oci_process", "oci_hypervisor"])),
        ),
        ("cache", schema_ref("EffectiveCachePolicy")),
        ("artifacts", schema_ref("ArtifactPolicy")),
        ("concurrency", schema_ref("EffectiveConcurrencyPolicy")),
        ("retention", schema_ref("EffectiveRetentionPolicy")),
      ],
      &[
        "pools",
        "repositories",
        "secret_profiles",
        "identity_profiles",
        "runtimes",
        "cache",
        "artifacts",
        "concurrency",
        "retention",
      ],
    ),
  );
  schemas.insert(
    "EffectiveProjectPolicy".to_owned(),
    object(
      [
        ("sources", array(schema_ref("PolicySource"))),
        ("policy", schema_ref("EffectiveProjectPolicyValue")),
      ],
      &["sources", "policy"],
    ),
  );
  schemas.insert(
    "TriggerCause".to_owned(),
    json!({
      "oneOf": [
        object([("kind", json!({"const": "manual"}))], &["kind"]),
        object([("kind", json!({"const": "scheduled"}))], &["kind"]),
        object(
          [
            ("kind", json!({"const": "external"})),
            ("integration_id", non_empty_string()),
            ("repository_id", non_empty_string()),
            ("event_kind", non_empty_string()),
            ("reference", nullable(non_empty_string())),
            ("revision", nullable(non_empty_string())),
          ],
          &["kind", "integration_id", "repository_id", "event_kind", "reference", "revision"],
        ),
        object(
          [
            ("kind", json!({"const": "internal"})),
            ("source_build_id", non_empty_string()),
            ("event_kind", non_empty_string()),
          ],
          &["kind", "source_build_id", "event_kind"],
        ),
      ],
      "discriminator": {"propertyName": "kind"},
    }),
  );
  schemas.insert(
    "TriggerHistory".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        (
          "trigger",
          object(
            [("id", non_empty_string()), ("version", positive_integer())],
            &["id", "version"],
          ),
        ),
        (
          "target",
          object(
            [
              ("configuration_id", non_empty_string()),
              ("configuration_version", positive_integer()),
            ],
            &["configuration_id", "configuration_version"],
          ),
        ),
        ("deduplication_identity", non_empty_string()),
        ("kind", string_enum(&["manual", "scheduled", "external", "internal"])),
        ("cause", schema_ref("TriggerCause")),
        (
          "causality",
          object(
            [
              ("root_occurrence_id", non_empty_string()),
              ("parent_occurrence_id", nullable(non_empty_string())),
              ("depth", non_negative_integer()),
            ],
            &["root_occurrence_id", "parent_occurrence_id", "depth"],
          ),
        ),
        ("source_time", integer()),
        (
          "state",
          string_enum(&[
            "pending",
            "evaluating",
            "deferred",
            "accepted",
            "suppressed",
            "rejected",
          ]),
        ),
        ("build_id", nullable(non_empty_string())),
        ("created_at", integer()),
        ("updated_at", integer()),
      ],
      &[
        "id",
        "trigger",
        "target",
        "deduplication_identity",
        "kind",
        "cause",
        "causality",
        "source_time",
        "state",
        "build_id",
        "created_at",
        "updated_at",
      ],
    ),
  );
  schemas.insert(
    "JobPlacement".to_owned(),
    object(
      [
        ("capabilities", unique_array(non_empty_string())),
        ("labels", string_map()),
        ("minimum_cpu_millis", positive_integer()),
        ("minimum_memory_bytes", positive_integer()),
        ("minimum_disk_bytes", positive_integer()),
        ("runtime_class", schema_ref("RuntimeClass")),
        ("operating_system", schema_ref("PlatformOs")),
        ("architecture", schema_ref("PlatformArchitecture")),
      ],
      &[
        "capabilities",
        "labels",
        "minimum_cpu_millis",
        "minimum_memory_bytes",
        "minimum_disk_bytes",
        "runtime_class",
        "operating_system",
        "architecture",
      ],
    ),
  );
  schemas.insert(
    "JobOutput".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("name", non_empty_string()),
        ("kind", string_enum(&["artifact", "report"])),
        ("sha256", json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})),
        ("size_bytes", non_negative_integer()),
        ("published_at", integer()),
      ],
      &["id", "name", "kind", "sha256", "size_bytes", "published_at"],
    ),
  );
}
