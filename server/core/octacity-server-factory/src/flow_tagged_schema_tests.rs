use crate::*;
use std::collections::BTreeMap;

fn schema(shape: FlowValueSchema) -> FlowDataSchema {
  FlowDataSchema::new(
    FactoryKey::new("typed-alternatives").unwrap(),
    FactoryKey::new("v1").unwrap(),
    shape,
  )
  .unwrap()
}
fn alternatives(maximum: i64) -> FlowValueSchema {
  FlowValueSchema::ExternallyTagged {
    variants: BTreeMap::from([
      ("Ready".to_owned(), None),
      (
        "RetryAfter".to_owned(),
        Some(Box::new(FlowValueSchema::Integer { minimum: 1, maximum })),
      ),
    ]),
  }
}

#[test]
fn external_tags_accept_only_the_declared_unit_or_single_payload_shape() {
  let schema = schema(alternatives(10));
  for value in [serde_json::json!("Ready"), serde_json::json!({"RetryAfter":3})] {
    let payload = FlowPayload::new(&schema, value.clone()).unwrap();
    assert_eq!(payload.value(), &value);
    assert_eq!(
      FlowPayload::restore(&serde_json::to_vec(&payload).unwrap(), &schema).unwrap(),
      payload
    );
  }
  for value in [
    serde_json::json!("ready"),
    serde_json::json!("RetryAfter"),
    serde_json::json!({"Ready":null}),
    serde_json::json!({"RetryAfter":0}),
    serde_json::json!({"RetryAfter":11}),
    serde_json::json!({"RetryAfter":"3"}),
    serde_json::json!({"RetryAfter":3,"Ready":null}),
    serde_json::json!({"Unknown":3}),
    serde_json::json!({}),
  ] {
    assert!(FlowPayload::new(&schema, value.clone()).is_err(), "must reject {value}");
  }
  assert!(
    FactoryKey::new("RetryAfter").is_err(),
    "data tags do not widen graph identifiers"
  );
}

#[test]
fn external_tag_compatibility_covers_every_variant_and_payload_range() {
  let consumer = schema(alternatives(10));
  assert!(consumer.accepts_all(&alternatives(3)));
  assert!(!consumer.accepts_all(&alternatives(11)));
  assert!(consumer.accepts_all(&FlowValueSchema::Enum {
    values: vec![serde_json::json!("Ready")]
  }));
  for producer in [
    FlowValueSchema::ExternallyTagged {
      variants: BTreeMap::from([("Unknown".into(), None)]),
    },
    FlowValueSchema::ExternallyTagged {
      variants: BTreeMap::from([("RetryAfter".into(), None)]),
    },
    FlowValueSchema::ExternallyTagged {
      variants: BTreeMap::from([("Ready".into(), Some(Box::new(FlowValueSchema::Boolean)))]),
    },
  ] {
    assert!(!consumer.accepts_all(&producer));
  }
  let restored: FlowDataSchema = serde_json::from_slice(&serde_json::to_vec(&consumer).unwrap()).unwrap();
  assert_eq!(restored, consumer);
  assert_ne!(schema(alternatives(3)).reference(), consumer.reference());
}

#[test]
fn external_tag_catalogs_are_nonempty_bounded_and_recursively_validated() {
  for shape in [
    FlowValueSchema::ExternallyTagged {
      variants: BTreeMap::new(),
    },
    FlowValueSchema::ExternallyTagged {
      variants: BTreeMap::from([("".into(), None)]),
    },
    FlowValueSchema::ExternallyTagged {
      variants: BTreeMap::from([("bad\nname".into(), None)]),
    },
    FlowValueSchema::ExternallyTagged {
      variants: BTreeMap::from([("a".repeat(MAX_FLOW_VARIANT_NAME_BYTES + 1), None)]),
    },
    FlowValueSchema::ExternallyTagged {
      variants: (0..=MAX_FLOW_SCHEMA_ENTRIES).map(|n| (format!("V{n}"), None)).collect(),
    },
    FlowValueSchema::ExternallyTagged {
      variants: BTreeMap::from([(
        "RetryAfter".into(),
        Some(Box::new(FlowValueSchema::Integer { minimum: 3, maximum: 1 })),
      )]),
    },
  ] {
    assert!(
      FlowDataSchema::new(
        FactoryKey::new("invalid").unwrap(),
        FactoryKey::new("v1").unwrap(),
        shape
      )
      .is_err()
    );
  }
  let mut deep = FlowValueSchema::Boolean;
  for _ in 0..=MAX_FLOW_SCHEMA_DEPTH {
    deep = FlowValueSchema::ExternallyTagged {
      variants: BTreeMap::from([("Next".into(), Some(Box::new(deep)))]),
    };
  }
  assert!(
    FlowDataSchema::new(
      FactoryKey::new("too-deep").unwrap(),
      FactoryKey::new("v1").unwrap(),
      deep
    )
    .is_err()
  );
}

#[test]
fn template_publication_resolves_only_declared_tag_payload_aliases_and_rejects_cycles() {
  let key = |name: &str| FactoryKey::new(name).unwrap();
  let mut template = crate::flow_test_support::supplied_base_flow_template();
  template.schemas.insert(
    key("tagged-payload"),
    FlowTemplateSchema {
      identity: key("tagged-payload"),
      version: key("v1"),
      shape: FlowValueSchema::Boolean,
    },
  );
  template.schemas.insert(
    key("tagged-choice"),
    FlowTemplateSchema {
      identity: key("tagged-choice"),
      version: key("v1"),
      shape: FlowValueSchema::ExternallyTagged {
        variants: BTreeMap::from([(
          "Selected".into(),
          Some(Box::new(FlowValueSchema::Reference {
            schema: key("tagged-payload"),
          })),
        )]),
      },
    },
  );
  let published = template.data_schemas().unwrap();
  assert!(FlowPayload::new(&published[&key("tagged-choice")], serde_json::json!({"Selected":true})).is_ok());
  assert!(FlowPayload::new(&published[&key("tagged-choice")], serde_json::json!({"Selected":1})).is_err());
  template.schemas.get_mut(&key("tagged-payload")).unwrap().shape = FlowValueSchema::Reference {
    schema: key("tagged-choice"),
  };
  assert!(template.data_schemas().is_err());
}
