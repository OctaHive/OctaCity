use crate::*;
use std::collections::BTreeMap;

fn key(name: &str) -> FactoryKey {
  FactoryKey::new(name).unwrap()
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
