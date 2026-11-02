//! Separate Project and legacy reference schemas; names and digests confer no authority.
use super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  let uuid = || json!({"type":"string","format":"uuid"});
  let version = || bounded_integer(1, i64::MAX as u64);
  let name =
    || json!({"type":"string","minLength":1,"maxLength":bounds.text_bytes,"x-max-utf8-bytes":bounds.text_bytes});
  schemas.insert(
    "FactoryLaunchPolicyDefinition".into(),
    object(
      [
        ("name", name()),
        ("allow_manual", boolean()),
        ("allow_automatic_source", boolean()),
      ],
      &["name", "allow_manual", "allow_automatic_source"],
    ),
  );
  schemas.insert(
    "FactoryAccessProfileDefinition".into(),
    object(
      [("name", name()), ("permissions", schema_ref("FactoryPermissions"))],
      &["name", "permissions"],
    ),
  );
  for (reference, policy, definition) in [
    (
      "FactoryLaunchPolicyRef",
      "FactoryLaunchPolicy",
      "FactoryLaunchPolicyDefinition",
    ),
    (
      "FactoryAccessProfileRef",
      "FactoryAccessProfile",
      "FactoryAccessProfileDefinition",
    ),
  ] {
    schemas.insert(
      reference.into(),
      object(
        [
          ("project_id", uuid()),
          ("id", uuid()),
          ("version", version()),
          ("digest", schema_ref("FactoryDigest")),
        ],
        &["project_id", "id", "version", "digest"],
      ),
    );
    schemas.insert(
      policy.into(),
      object(
        [
          ("reference", schema_ref(reference)),
          ("definition", schema_ref(definition)),
        ],
        &["reference", "definition"],
      ),
    );
  }
  schemas.insert(
    "FactoryProjectPolicyRef".into(),
    json!({"oneOf":[
      tagged("kind","launch",[("reference",schema_ref("FactoryLaunchPolicyRef"))], &["reference"]),
      tagged("kind","access",[("reference",schema_ref("FactoryAccessProfileRef"))], &["reference"]),
    ]}),
  );
  schemas.insert(
    "FactoryProjectPolicyRecord".into(),
    json!({"oneOf":[
      tagged("kind","launch",[("policy",schema_ref("FactoryLaunchPolicy"))], &["policy"]),
      tagged("kind","access",[("policy",schema_ref("FactoryAccessProfile"))], &["policy"]),
    ]}),
  );
  schemas.insert("FactoryProjectPolicyDraft".into(), json!({"oneOf":[
    tagged("kind","launch",[("id",uuid()),("expected_current",nullable(schema_ref("FactoryLaunchPolicyRef"))),("definition",schema_ref("FactoryLaunchPolicyDefinition"))], &["id","definition"]),
    tagged("kind","access",[("id",uuid()),("expected_current",nullable(schema_ref("FactoryAccessProfileRef"))),("definition",schema_ref("FactoryAccessProfileDefinition"))], &["id","definition"]),
  ]}));
  schemas.insert(
    "FactoryProjectPolicyDetails".into(),
    object([("policy", schema_ref("FactoryProjectPolicyRecord"))], &["policy"]),
  );
  schemas.insert(
    "FactoryPolicyInstalledCeiling".into(),
    object(
      [
        ("alias", json!({"type":"string"})),
        ("reference", schema_ref("FactoryImmutableReferenceResource")),
        ("permissions", schema_ref("FactoryPermissions")),
      ],
      &["alias", "reference"],
    ),
  );
  schemas.insert(
    "FactoryProjectPolicyValidation".into(),
    object(
      [
        ("policy", schema_ref("FactoryProjectPolicyRecord")),
        ("installed_ceiling", schema_ref("FactoryPolicyInstalledCeiling")),
      ],
      &["policy"],
    ),
  );
  schemas.insert(
    "FactoryProjectPolicyPublication".into(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("policy", schema_ref("FactoryProjectPolicyRecord")),
        ("published_at", integer()),
      ],
      &["disposition", "policy", "published_at"],
    ),
  );
  schemas.insert(
    "FactoryProjectPolicySummary".into(),
    object(
      [("reference", schema_ref("FactoryProjectPolicyRef")), ("name", name())],
      &["reference", "name"],
    ),
  );
  schemas.insert(
    "FactoryProjectPolicyPage".into(),
    object(
      [
        (
          "items",
          bounded_array(
            schema_ref("FactoryProjectPolicySummary"),
            usize::from(octacity_server_application::MAX_CURRENT_DEFINITION_PAGE_SIZE),
          ),
        ),
        ("next_cursor", nullable(schema_ref("FactoryProjectPolicyRef"))),
      ],
      &["items", "next_cursor"],
    ),
  );
  let pins = || {
    [
      ("launch_policy", nullable(schema_ref("FactoryLaunchPolicyRef"))),
      ("access_profile", nullable(schema_ref("FactoryAccessProfileRef"))),
    ]
  };
  schemas.insert(
    "FactoryProjectPolicyDefaultsDraft".into(),
    object(
      [
        ("expected_version", nullable(version())),
        pins()[0].clone(),
        pins()[1].clone(),
      ],
      &[],
    ),
  );
  schemas.insert(
    "FactoryProjectPolicyDefaults".into(),
    object(
      [
        ("project_id", uuid()),
        ("version", version()),
        pins()[0].clone(),
        pins()[1].clone(),
      ],
      &["project_id", "version", "launch_policy", "access_profile"],
    ),
  );
  schemas.insert(
    "FactoryProjectPolicyDefaultsRead".into(),
    object(
      [("defaults", nullable(schema_ref("FactoryProjectPolicyDefaults")))],
      &["defaults"],
    ),
  );
  schemas.insert(
    "FactoryProjectPolicyDefaultsPublication".into(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("defaults", schema_ref("FactoryProjectPolicyDefaults")),
        ("published_at", integer()),
      ],
      &["disposition", "defaults", "published_at"],
    ),
  );
  schemas.insert(
    "FactoryLegacyPolicyReference".into(),
    json!({"oneOf":[
      tagged("kind","launch",[("reference",schema_ref("FactoryImmutableReferenceResource"))], &["reference"]),
      tagged("kind","access",[("reference",schema_ref("FactoryImmutableReferenceResource"))], &["reference"]),
    ]}),
  );
  schemas.insert(
    "FactoryLegacyPolicyMapping".into(),
    object(
      [
        ("project_id", uuid()),
        ("legacy", schema_ref("FactoryLegacyPolicyReference")),
        ("target", schema_ref("FactoryProjectPolicyRef")),
      ],
      &["project_id", "legacy", "target"],
    ),
  );
  schemas.insert(
    "FactoryLegacyPolicyMappingRead".into(),
    object(
      [("mapping", nullable(schema_ref("FactoryLegacyPolicyMapping")))],
      &["mapping"],
    ),
  );
}
