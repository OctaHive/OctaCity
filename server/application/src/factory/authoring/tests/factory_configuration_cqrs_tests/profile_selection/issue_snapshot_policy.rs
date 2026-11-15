//! Policy declarations are qualified against canonical selected Project fields.
use super::*;

fn policy_draft(project: ProjectId) -> (Fixture, serde_json::Value) {
  let (fixture, mut wire) = snapshot_draft(project);
  wire["issue_snapshot"]["fields"]["visible"]["source"] = serde_json::json!({"kind":"priority"});
  snapshot_shape(
    &mut wire,
    [(
      key("visible"),
      FlowFieldSchema::required(FlowValueSchema::String {
        min_bytes: 0,
        max_bytes: MAX_ISSUE_DOCUMENT_BYTES,
      }),
    )]
    .into(),
  );
  let entry = entry_index(&wire);
  wire["root"]["nodes"][entry]["input_binding"]["mapping"]["value"] =
    serde_json::json!({"mapping":"constant","value":true});
  wire["issue_snapshot"]["admission"] = serde_json::json!({
    "priority":{"field":"visible","values":{"high":80}},
    "risk":{"field":"visible","values":{"high":"low"}}, "allowed":{}
  });
  (fixture, wire)
}

#[test]
fn issue_snapshot_policy_publication_rejects_missing_mapping_fields() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    let (fixture, wire) = policy_draft(project);
    let published = publish(project, fixture, wire.clone(), Some(properties.clone()))
      .await
      .unwrap();
    assert_eq!(
      serde_json::to_value(published).unwrap()["issue_snapshot"]["admission"],
      wire["issue_snapshot"]["admission"]
    );
    for case in [
      "missing",
      "empty",
      "unknown_choice",
      "empty_allowlist",
      "unknown_allowed",
      "optional",
      "prose",
      "ordinary",
    ] {
      let (fixture, mut wire) = policy_draft(project);
      match case {
        "missing" => wire["issue_snapshot"]["admission"]["priority"]["field"] = "missing".into(),
        "empty" => wire["issue_snapshot"]["admission"]["risk"]["values"] = serde_json::json!({}),
        "unknown_choice" => {
          wire["issue_snapshot"]["admission"]["priority"]["values"] = serde_json::json!({"critical":80})
        }
        "empty_allowlist" => wire["issue_snapshot"]["admission"]["allowed"] = serde_json::json!({"visible":[]}),
        "unknown_allowed" => {
          wire["issue_snapshot"]["admission"]["allowed"] = serde_json::json!({"visible":["critical"]})
        }
        "optional" => {
          wire["issue_snapshot"]["fields"]["visible"]["required"] = false.into();
          snapshot_shape(
            &mut wire,
            [(
              key("visible"),
              FlowFieldSchema::optional(FlowValueSchema::String {
                min_bytes: 0,
                max_bytes: MAX_ISSUE_DOCUMENT_BYTES,
              }),
            )]
            .into(),
          );
        }
        "prose" => wire["issue_snapshot"]["fields"]["visible"]["source"] = serde_json::json!({"kind":"title"}),
        "ordinary" => {
          wire["issue_snapshot"]["fields"]["visible"]["source"] =
            serde_json::json!({"kind":"property","id":"notes_text"})
        }
        _ => unreachable!(),
      }
      assert_invalid(
        publish(project, fixture, wire, Some(properties.clone())).await,
        "/issue_snapshot/admission",
      );
    }
  });
}

#[test]
fn issue_snapshot_admission_uses_direct_project_choice_fields_without_fixed_roles() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    let mut catalog = properties.issue_properties(project, None).await.unwrap();
    catalog.definition.classification = Default::default();
    catalog.definition.properties.push(IssuePropertyDefinition {
      id: "queue_bucket".into(),
      name: "Queue bucket".into(),
      retired: false,
      kind: IssuePropertyKind::Choice {
        choices: vec![IssuePropertyChoice {
          id: "expedite".into(),
          name: "Expedite".into(),
          retired: false,
        }],
      },
    });
    let catalog = properties
      .publish_issue_properties(octacity_server_store::testing::management_mutation(
        PublishIssueProperties {
          intent: PublishIssuePropertiesIntent {
            project_id: project,
            expected_version: catalog.version,
            definition: catalog.definition,
          },
          idempotency_key: "direct-choice-catalog".parse().unwrap(),
          published_at: time(3),
        },
      ))
      .await
      .unwrap()
      .resource;
    let (fixture, mut wire) = policy_draft(project);
    wire["issue_snapshot"]["definition_version"] = catalog.version.into();
    wire["issue_snapshot"]["fields"]["visible"]["source"] = serde_json::json!({"kind":"property","id":"queue_bucket"});
    wire["issue_snapshot"]["admission"] = serde_json::json!({
      "priority":{"field":"visible","values":{"expedite":72}},
      "risk":{"field":"visible","values":{"expedite":"high"}},
      "allowed":{"visible":["expedite"]}
    });
    let published = publish(project, fixture, wire.clone(), Some(properties))
      .await
      .expect("any selected Project choice can supply an explicit typed admission conversion");
    assert_eq!(
      serde_json::to_value(published).unwrap()["issue_snapshot"]["admission"],
      wire["issue_snapshot"]["admission"]
    );
  });
}
