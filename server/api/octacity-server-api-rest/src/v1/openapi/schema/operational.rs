use serde_json::{Map, Value, json};

use super::super::{
  array, boolean, integer, non_empty_string, non_negative_integer, object, positive_integer, schema_ref, string_enum,
};

pub(super) fn insert_job_event_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "JobEventResource".to_owned(),
    object(
      [
        ("sequence", positive_integer()),
        ("kind", json!({"type": "string", "minLength": 1, "maxLength": 64})),
        ("occurred_at_unix_ms", integer()),
        ("payload", json!({})),
      ],
      &["sequence", "kind", "occurred_at_unix_ms", "payload"],
    ),
  );
  schemas.insert(
    "JobEventPage".to_owned(),
    object(
      [
        ("items", array(schema_ref("JobEventResource"))),
        ("cursor", non_negative_integer()),
      ],
      &["items", "cursor"],
    ),
  );
}

pub(super) fn insert_operational_schemas(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "ManagementSecurityMode".to_owned(),
    string_enum(&["trusted_network_unauthenticated"]),
  );
  schemas.insert(
    "AuthenticatedIngressMode".to_owned(),
    string_enum(&["registration_credentials", "provider_verification"]),
  );
  schemas.insert(
    "CapabilityStatus".to_owned(),
    string_enum(&["available", "unavailable"]),
  );
  schemas.insert(
    "ManagementSecurity".to_owned(),
    object(
      [
        ("mode", schema_ref("ManagementSecurityMode")),
        ("operator_authentication", boolean()),
        ("externally_reachable", boolean()),
        ("external_access_acknowledged", boolean()),
      ],
      &[
        "mode",
        "operator_authentication",
        "externally_reachable",
        "external_access_acknowledged",
      ],
    ),
  );
  schemas.insert(
    "AuthenticatedIngressSecurity".to_owned(),
    object(
      [
        ("mode", schema_ref("AuthenticatedIngressMode")),
        ("authentication_required", boolean()),
      ],
      &["mode", "authentication_required"],
    ),
  );
  schemas.insert(
    "DeploymentSecurity".to_owned(),
    object(
      [
        ("management", schema_ref("ManagementSecurity")),
        ("agent", schema_ref("AuthenticatedIngressSecurity")),
        ("webhook", schema_ref("AuthenticatedIngressSecurity")),
      ],
      &["management", "agent", "webhook"],
    ),
  );
  schemas.insert(
    "IngressMetadata".to_owned(),
    object(
      [
        ("management_enabled", boolean()),
        ("agent_enabled", boolean()),
        ("webhook_enabled", boolean()),
        ("listeners_separate", boolean()),
      ],
      &[
        "management_enabled",
        "agent_enabled",
        "webhook_enabled",
        "listeners_separate",
      ],
    ),
  );
  schemas.insert(
    "CapabilityMetadata".to_owned(),
    object(
      [("name", non_empty_string()), ("status", schema_ref("CapabilityStatus"))],
      &["name", "status"],
    ),
  );
  schemas.insert(
    "OperationalMetadata".to_owned(),
    object(
      [
        ("api_version", json!({"const": "v1"})),
        ("security", schema_ref("DeploymentSecurity")),
        ("ingress", schema_ref("IngressMetadata")),
        ("capabilities", array(schema_ref("CapabilityMetadata"))),
      ],
      &["api_version", "security", "ingress", "capabilities"],
    ),
  );
}
