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

#[test]
fn direct_choice_thresholds_freeze_size_and_unrelated_project_field_order() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    let mut catalog = properties.issue_properties(project, None).await.unwrap();
    catalog.definition.classification = Default::default();
    catalog
      .definition
      .properties
      .push(octacity_server_store::IssuePropertyDefinition {
        id: "queue_lane".into(),
        name: "Queue lane".into(),
        retired: false,
        kind: octacity_server_store::IssuePropertyKind::Choice {
          choices: ["routine", "review", "deep_review"]
            .into_iter()
            .map(|id| octacity_server_store::IssuePropertyChoice {
              id: id.into(),
              name: id.into(),
              retired: false,
            })
            .collect(),
        },
      });
    for (source, threshold, values) in [
      (FlowIssueFieldSource::Size, "medium", ["small", "medium", "large"]),
      (
        FlowIssueFieldSource::Property {
          id: "queue_lane".into(),
        },
        "review",
        ["routine", "review", "deep_review"],
      ),
    ] {
      let program = FlowGateProgram::new(
        vec![FlowGateRule {
          when: crate::issue_choice_threshold(&catalog, &source, "/arbitrary_alias", threshold).unwrap(),
          outcome: key("review_needed"),
        }],
        key("ready"),
      )
      .unwrap();
      let schema = FlowDataSchema::new(
        key("ordinary_choice"),
        key("1"),
        FlowValueSchema::Object {
          fields: [(
            key("arbitrary_alias"),
            FlowFieldSchema::required(FlowValueSchema::Enum {
              values: values.into_iter().map(serde_json::Value::from).collect(),
            }),
          )]
          .into(),
        },
      )
      .unwrap();
      // Later labels/order cannot alter an already compiled, serializable ordinary gate.
      let restored: FlowGateProgram = serde_json::from_value(serde_json::to_value(&program).unwrap()).unwrap();
      let mut edited = catalog.clone();
      edited.definition.sizes.reverse();
      if let octacity_server_store::IssuePropertyKind::Choice { choices } =
        &mut edited.definition.properties.last_mut().unwrap().kind
      {
        choices.reverse();
        choices[0].name = "Renamed".into();
      }
      let newer = FlowGateProgram::new(
        vec![FlowGateRule {
          when: crate::issue_choice_threshold(&edited, &source, "/arbitrary_alias", threshold).unwrap(),
          outcome: key("review_needed"),
        }],
        key("ready"),
      )
      .unwrap();
      for (index, value) in values.into_iter().enumerate() {
        let payload = FlowPayload::new(&schema, serde_json::json!({"arbitrary_alias":value})).unwrap();
        assert_eq!(
          restored.select(&payload).unwrap(),
          key(if index == 0 { "ready" } else { "review_needed" })
        );
        assert_eq!(
          newer.select(&payload).unwrap(),
          key(if index == 2 { "ready" } else { "review_needed" })
        );
      }
      assert!(crate::issue_choice_threshold(&catalog, &source, "/arbitrary_alias", "missing").is_err());
    }
    catalog.definition.sizes[1].retired = true;
    assert!(crate::issue_choice_threshold(&catalog, &FlowIssueFieldSource::Size, "/x", "medium").is_err());
    assert!(crate::issue_choice_threshold(&catalog, &FlowIssueFieldSource::Title, "/x", "medium").is_err());
  });
}
