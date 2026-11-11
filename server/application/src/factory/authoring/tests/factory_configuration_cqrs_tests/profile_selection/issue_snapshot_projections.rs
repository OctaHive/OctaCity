//! Snapshot projections must be well typed and explicit about optional presence.
use super::*;

#[test]
fn flow_publication_checks_snapshot_step_paths_types_and_optional_presence() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    for case in ["unselected", "wrong_type", "optional_source", "unguarded_optional"] {
      let (fixture, mut wire) = snapshot_draft(project);
      if case.contains("optional") {
        wire["issue_snapshot"]["fields"]["visible"]["required"] = serde_json::json!(false);
        snapshot_shape(
          &mut wire,
          [(key("visible"), FlowFieldSchema::optional(FlowValueSchema::Boolean))].into(),
        );
      }
      let entry = entry_index(&wire);
      let binding = &mut wire["root"]["nodes"][entry]["input_binding"];
      if case == "unselected" {
        binding["sources"]["issue"]["path"] = serde_json::json!("/not_selected");
      } else if case != "optional_source" {
        binding["sources"]["issue"]["path"] = serde_json::json!("");
        binding["mapping"]["value"] = if case == "unguarded_optional" {
          serde_json::json!({"mapping":"pointer","path":"/issue/visible"})
        } else {
          serde_json::json!({"mapping":"pointer","path":"/issue"})
        };
      }
      let result = publish(project, fixture, wire, Some(properties.clone())).await;
      let Err(crate::ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report))) = result
      else {
        panic!("invalid snapshot step projection must block publication: {case}");
      };
      assert!(
        report
          .errors
          .iter()
          .any(|error| error.field == "/input_binding" && error.node.is_some() && error.definition.is_some())
      );
    }
    let (fixture, mut wire) = snapshot_draft(project);
    wire["issue_snapshot"]["fields"]["visible"]["required"] = serde_json::json!(false);
    snapshot_shape(
      &mut wire,
      [(key("visible"), FlowFieldSchema::optional(FlowValueSchema::Boolean))].into(),
    );
    let entry = entry_index(&wire);
    wire["root"]["nodes"][entry]["input_binding"]["sources"]["issue"]["path"] = serde_json::json!("");
    wire["root"]["nodes"][entry]["input_binding"]["mapping"]["value"] = serde_json::json!({
      "mapping":"if_present","path":"/issue/visible",
      "then":{"mapping":"pointer","path":"/issue/visible"},
      "otherwise":{"mapping":"constant","value":false}
    });
    let schema = wire["issue_snapshot"]["schema"].clone();
    let flow = fixture.draft.flow.as_ref().unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let next_key = root
      .transitions()
      .iter()
      .find_map(|edge| {
        if edge.predecessor() != root.entry() {
          return None;
        }
        if let FlowTransitionTarget::Node(key) = edge.target() {
          Some(key)
        } else {
          None
        }
      })
      .unwrap();
    let next = wire["root"]["nodes"]
      .as_array()
      .unwrap()
      .iter()
      .position(|node| node["key"] == next_key.as_str())
      .unwrap();
    wire["root"]["nodes"][next]["input_binding"] = serde_json::json!({
      "sources":{"issue":{"source":"incoming","schema":schema,"path":""},
        "earlier":{"source":"record","node":wire["root"]["entry"],"outcome":"succeeded","view":"payload","path":""}},
      "mapping":{"value":{"mapping":"if_present","path":"/issue/visible",
        "then":{"mapping":"pointer","path":"/earlier"},"otherwise":{"mapping":"constant","value":false}}}
    });
    let published = publish(project, fixture, wire.clone(), Some(properties.clone()))
      .await
      .unwrap();
    let nodes = published.closure.definition(published.closure.root()).unwrap().nodes();
    assert_eq!(
      serde_json::to_value(nodes[next].input_binding()).unwrap(),
      wire["root"]["nodes"][next]["input_binding"]
    );
  });
}
