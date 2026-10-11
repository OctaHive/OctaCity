use crate::*;
use std::collections::BTreeMap;

fn key(name: &str) -> FactoryKey {
  FactoryKey::new(name).unwrap()
}

#[test]
fn configured_presence_mapping_selects_explicit_branches_without_missing_data_fallback() {
  let value = FlowValueSchema::String {
    min_bytes: 1,
    max_bytes: 50,
  };
  let schema = FlowDataSchema::new(
    key("request"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (key("original"), FlowFieldSchema::required(value.clone())),
        (key("parent"), FlowFieldSchema::optional(value.clone())),
      ]),
    },
  )
  .unwrap();
  let target = FlowDataSchema::new(key("selected"), key("v1"), value).unwrap();
  let wire = serde_json::json!({"value":{"mapping":"if_present","path":"/parent",
    "then":{"mapping":"pointer","path":"/parent"},
    "otherwise":{"mapping":"pointer","path":"/original"}}});
  let mapping: FlowDataMapping = serde_json::from_value(wire.clone()).unwrap();
  for (input, expected) in [
    (serde_json::json!({"original":"base"}), "base"),
    (
      serde_json::json!({"original":"base","parent":"previous-candidate"}),
      "previous-candidate",
    ),
  ] {
    assert_eq!(
      mapping
        .project(&FlowPayload::new(&schema, input).unwrap(), &target)
        .unwrap()
        .value(),
      expected
    );
  }
  let mut missing = wire.clone();
  missing["value"]["then"]["path"] = serde_json::json!("/undeclared");
  let mapping: FlowDataMapping = serde_json::from_value(missing).unwrap();
  assert!(
    mapping
      .project(
        &FlowPayload::new(&schema, serde_json::json!({"original":"base","parent":"previous"})).unwrap(),
        &target
      )
      .is_err()
  );
  let mut invalid = wire;
  invalid["value"]["path"] = serde_json::json!("/bad~2pointer");
  assert!(serde_json::from_value::<FlowDataMapping>(invalid).is_err());
}

#[test]
fn configured_array_mapping_projects_only_declared_items() {
  let text = FlowValueSchema::String {
    min_bytes: 1,
    max_bytes: 30,
  };
  let source_schema = FlowDataSchema::new(
    key("input"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (key("public"), FlowFieldSchema::required(text.clone())),
        (key("private"), FlowFieldSchema::required(text.clone())),
      ]),
    },
  )
  .unwrap();
  let target = FlowDataSchema::new(
    key("references"),
    key("v1"),
    FlowValueSchema::Array {
      item: Box::new(text),
      min_items: 1,
      max_items: 2,
    },
  )
  .unwrap();
  let mapping: FlowDataMapping = serde_json::from_value(serde_json::json!({"value":{"mapping":"array",
    "items":[{"mapping":"pointer","path":"/public"}]}}))
  .unwrap();
  let input = FlowPayload::new(
    &source_schema,
    serde_json::json!({"public":"safe-report","private":"hidden-test"}),
  )
  .unwrap();
  assert_eq!(
    mapping.project(&input, &target).unwrap().value(),
    &serde_json::json!(["safe-report"])
  );
}

#[test]
fn configured_mapping_exposes_only_selected_fields_under_the_target_schema() {
  let source_schema = FlowDataSchema::new(
    key("team.raw"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("score"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 100,
          }),
        ),
        (
          key("private_note"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 50,
          }),
        ),
      ]),
    },
  )
  .unwrap();
  let target = FlowDataSchema::new(
    key("team.accepted"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("quality"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 100,
          }),
        ),
        (
          key("policy"),
          FlowFieldSchema::required(FlowValueSchema::Enum {
            values: vec![serde_json::json!("operator_v2")],
          }),
        ),
      ]),
    },
  )
  .unwrap();
  let source = FlowPayload::new(
    &source_schema,
    serde_json::json!({"score": 81, "private_note": "not projected"}),
  )
  .unwrap();
  let mapping = FlowDataMapping::new(FlowValueMapping::Object {
    fields: BTreeMap::from([
      (key("quality"), FlowValueMapping::Pointer { path: "/score".into() }),
      (
        key("policy"),
        FlowValueMapping::Constant {
          value: serde_json::json!("operator_v2"),
        },
      ),
    ]),
  })
  .unwrap();
  let projected = mapping.project(&source, &target).unwrap();
  assert_eq!(
    projected.value(),
    &serde_json::json!({"quality": 81, "policy": "operator_v2"})
  );
  assert_eq!(projected.schema(), target.reference());
  let restored: FlowDataMapping = serde_json::from_slice(&serde_json::to_vec(&mapping).unwrap()).unwrap();
  assert_eq!(restored.project(&source, &target).unwrap(), projected);
  let missing = FlowDataMapping::new(FlowValueMapping::Pointer { path: "/absent".into() }).unwrap();
  assert!(missing.project(&source, &target).is_err());
  let all = FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap();
  assert!(all.project(&source, &target).is_err());
  assert!(FlowDataMapping::new(FlowValueMapping::Pointer { path: "score".into() }).is_err());
}

#[test]
fn configured_mapping_bounds_configuration_and_expanded_result_bytes() {
  let mut deep = FlowValueMapping::Pointer { path: String::new() };
  for _ in 0..17 {
    deep = FlowValueMapping::Object {
      fields: BTreeMap::from([(key("next"), deep)]),
    };
  }
  assert!(FlowDataMapping::new(deep).is_err());
  assert!(
    FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!("x".repeat(16 * 1024))
    })
    .is_err()
  );
  let invalid_wire = serde_json::json!({"value": {"mapping": "pointer", "path": "/bad~2path"}});
  assert!(serde_json::from_value::<FlowDataMapping>(invalid_wire).is_err());
  let source_schema = FlowDataSchema::new(
    key("team.large_source"),
    key("v1"),
    FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: MAX_FLOW_DATA_BYTES,
    },
  )
  .unwrap();
  let source = FlowPayload::new(&source_schema, serde_json::json!("x".repeat(MAX_FLOW_DATA_BYTES / 2))).unwrap();
  let target = FlowDataSchema::new(
    key("team.large_target"),
    key("v1"),
    FlowValueSchema::Object {
      fields: ["first", "second", "third"]
        .into_iter()
        .map(|name| {
          (
            key(name),
            FlowFieldSchema::required(FlowValueSchema::String {
              min_bytes: 1,
              max_bytes: MAX_FLOW_DATA_BYTES,
            }),
          )
        })
        .collect(),
    },
  )
  .unwrap();
  let mapping = FlowDataMapping::new(FlowValueMapping::Object {
    fields: ["first", "second", "third"]
      .into_iter()
      .map(|name| (key(name), FlowValueMapping::Pointer { path: String::new() }))
      .collect(),
  })
  .unwrap();
  assert!(mapping.project(&source, &target).is_err());
}
