//! Selection reuses canonical definitions; consumers need no copied Project enum.
use super::*;

#[test]
fn flow_publication_allows_an_optional_builtin_with_an_empty_project_choice_list() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    let (fixture, mut wire) = snapshot_draft(project);
    // The default catalog has no components; no Issue value or replacement choice is fabricated.
    wire["issue_snapshot"]["definition_version"] = serde_json::json!(0);
    wire["issue_snapshot"]["fields"]["visible"] = serde_json::json!({"source":{"kind":"component"},"required":false});
    snapshot_shape(
      &mut wire,
      [(
        key("visible"),
        FlowFieldSchema::optional(FlowValueSchema::String {
          min_bytes: 1,
          max_bytes: 64,
        }),
      )]
      .into(),
    );
    let entry = entry_index(&wire);
    wire["root"]["nodes"][entry]["input_binding"]["sources"]["issue"]["path"] = serde_json::json!("");
    wire["root"]["nodes"][entry]["input_binding"]["mapping"]["value"] = serde_json::json!({
      "mapping":"if_present","path":"/issue/visible",
      "then":{"mapping":"constant","value":true},"otherwise":{"mapping":"constant","value":false}
    });
    let before = properties.mutation_evidence_counts().await;
    let required_fixture = Fixture {
      choices: fixture.choices.clone(),
      draft: fixture.draft.clone(),
    };
    let published = publish(project, fixture, wire.clone(), Some(properties.clone()))
      .await
      .unwrap();
    assert_eq!(
      serde_json::to_value(published).unwrap()["issue_snapshot"],
      wire["issue_snapshot"]
    );
    assert_eq!(properties.mutation_evidence_counts().await, before);
    wire["issue_snapshot"]["fields"]["visible"]["required"] = serde_json::json!(true);
    assert_invalid(
      publish(project, required_fixture, wire, Some(properties.clone())).await,
      "/issue_snapshot/fields/visible",
    );
  });
}

#[test]
fn flow_publication_reuses_builtin_project_and_dependency_contracts_without_copied_enums() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    let text = FlowValueSchema::String {
      min_bytes: 0,
      max_bytes: 65_536,
    };
    let dependencies = FlowValueSchema::Array {
      min_items: 0,
      max_items: 64,
      item: Box::new(FlowValueSchema::Object {
        fields: [
          (
            key("work_id"),
            FlowFieldSchema::required(FlowValueSchema::String {
              min_bytes: 36,
              max_bytes: 36,
            }),
          ),
          (
            key("run_id"),
            FlowFieldSchema::required(FlowValueSchema::String {
              min_bytes: 36,
              max_bytes: 36,
            }),
          ),
          (
            key("work_digest"),
            FlowFieldSchema::required(FlowValueSchema::String {
              min_bytes: 64,
              max_bytes: 64,
            }),
          ),
          (
            key("terminal"),
            FlowFieldSchema::required(FlowValueSchema::String {
              min_bytes: 1,
              max_bytes: 128,
            }),
          ),
        ]
        .into(),
      }),
    };
    let mut cases = vec![(serde_json::json!({"kind":"dependencies"}), dependencies)];
    for kind in [
      "title",
      "description",
      "acceptance",
      "priority",
      "component",
      "type",
      "size",
    ] {
      cases.push((serde_json::json!({"kind":kind}), text.clone()));
    }
    for id in ["notes_text", "target_date", "environment"] {
      cases.push((serde_json::json!({"kind":"property","id":id}), text.clone()));
    }
    cases.push((
      serde_json::json!({"kind":"property","id":"cost_points"}),
      FlowValueSchema::Integer {
        minimum: -2,
        maximum: 10,
      },
    ));
    cases.push((serde_json::json!({"kind":"classification","slot":"severity"}), text));
    for (source, shape) in cases {
      let (fixture, mut wire) = snapshot_draft(project);
      wire["issue_snapshot"]["fields"]["visible"]["source"] = source;
      snapshot_shape(&mut wire, [(key("visible"), FlowFieldSchema::required(shape))].into());
      let entry = entry_index(&wire);
      // This stage only needs presence; the source schema still has to fit the Project type.
      wire["root"]["nodes"][entry]["input_binding"]["mapping"]["value"] =
        serde_json::json!({"mapping":"constant","value":true});
      let published = publish(project, fixture, wire.clone(), Some(properties.clone()))
        .await
        .unwrap();
      assert_eq!(
        serde_json::to_value(published).unwrap()["issue_snapshot"],
        wire["issue_snapshot"]
      );
    }
  });
}
