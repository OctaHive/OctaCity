//! Whole input mappings retain independently declared intra-Flow sources.
use crate::*;

#[test]
fn issue_snapshot_projection_preserves_a_whole_alias_map_with_an_ordinary_record() {
  let key = |value: &str| FactoryKey::new(value).unwrap();
  let input = FlowDataSchema::new(
    key("issue.input"),
    key("v1"),
    FlowValueSchema::Object {
      fields: [(key("visible"), FlowFieldSchema::required(FlowValueSchema::Boolean))].into(),
    },
  )
  .unwrap();
  let binding = FlowInputBinding::new(
    [
      (
        key("issue"),
        FlowInputSource::Incoming {
          schema: input.reference().clone(),
          path: "/visible".into(),
        },
      ),
      (
        key("earlier"),
        FlowInputSource::Record {
          node: key("producer"),
          outcome: key("observed"),
          view: FlowRecordView::Payload,
          path: String::new(),
        },
      ),
    ]
    .into(),
    FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap(),
  )
  .unwrap();
  let target = |issue| {
    FlowDataSchema::new(
      key("step.input"),
      key("v1"),
      FlowValueSchema::Object {
        fields: [
          (key("issue"), FlowFieldSchema::required(issue)),
          (key("earlier"), FlowFieldSchema::required(FlowValueSchema::Boolean)),
        ]
        .into(),
      },
    )
    .unwrap()
  };
  assert!(
    binding.fits_incoming(input.reference(), input.shape(), &target(FlowValueSchema::Boolean)),
    "whole-map projection must retain the existing record input alongside the snapshot"
  );
  assert!(
    !binding.fits_incoming(
      input.reference(),
      input.shape(),
      &target(FlowValueSchema::String {
        min_bytes: 0,
        max_bytes: 80
      })
    ),
    "the known snapshot projection still has to fit its target type"
  );
}
