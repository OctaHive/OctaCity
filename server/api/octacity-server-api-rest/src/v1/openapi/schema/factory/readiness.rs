//! Exact readiness metadata; no command or provider credential fields.
use super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let uuid = || json!({"type":"string","format":"uuid"});
  let state = || json!({"type":"string","enum":["ready","blocked","unknown"]});
  schemas.insert(
    "FactoryReadinessRepository".into(),
    object(
      [
        ("id", uuid()),
        ("project_id", uuid()),
        ("version", positive_integer()),
        ("definition_digest", schema_ref("FactoryDigest")),
      ],
      &["id", "project_id", "version", "definition_digest"],
    ),
  );
  schemas.insert(
    "FactoryReadinessInput".into(),
    object([("id", uuid()), ("version", positive_integer())], &["id", "version"]),
  );
  schemas.insert(
    "FactoryReadinessSelection".into(),
    object(
      [
        ("repository", schema_ref("FactoryReadinessRepository")),
        ("input", nullable(schema_ref("FactoryReadinessInput"))),
      ],
      &["repository"],
    ),
  );
  schemas.insert(
    "FactoryReadinessCheck".into(),
    object(
      [
        ("subject", nullable(schema_ref("FactoryReadinessSubject"))),
        ("remediation", json!({"type":"string","minLength":1,"maxLength":64})),
        ("aspect", json!({"type":"string","minLength":1,"maxLength":64})),
        ("reason", json!({"type":"string","minLength":1,"maxLength":128})),
        ("required", boolean()),
        ("state", state()),
        ("observed_at", integer()),
        ("expires_at", integer()),
      ],
      &[
        "remediation",
        "aspect",
        "reason",
        "required",
        "state",
        "observed_at",
        "expires_at",
      ],
    ),
  );
  schemas.insert(
    "FactoryReadinessSubject".into(),
    object(
      [
        ("definition", schema_ref("FactoryFlowDefinitionReference")),
        ("node", key()),
        ("dependency", key()),
      ],
      &["definition", "node"],
    ),
  );
  schemas.insert(
    "FactoryReadinessReport".into(),
    object(
      [
        ("configuration", schema_ref("FactoryConfigurationReference")),
        ("repository", schema_ref("FactoryReadinessRepository")),
        ("flow", schema_ref("FactoryFlowDefinitionReference")),
        ("execution", state()),
        ("native_write", state()),
        ("observed_at", integer()),
        ("expires_at", integer()),
        (
          "checks",
          bounded_array(
            schema_ref("FactoryReadinessCheck"),
            octacity_server_application::factory_configuration_document_bounds().choice_entries,
          ),
        ),
      ],
      &[
        "configuration",
        "repository",
        "flow",
        "execution",
        "native_write",
        "observed_at",
        "expires_at",
        "checks",
      ],
    ),
  );
}
