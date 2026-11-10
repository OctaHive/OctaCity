//! Retained classification is exercised through the public Issue revision/store contracts.
use crate::*;
use octacity_server_domain::{IssueId, IssueVersion, ProjectId, ProjectName, Timestamp};
use serde_json::{Value, json};

#[path = "issue_classification_applicability_tests.rs"]
mod applicability;

fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}
fn run_ready(future: impl std::future::Future<Output = ()>) {
  let mut future = std::pin::pin!(future);
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(future.as_mut().poll(&mut context).is_ready());
}
async fn ordinary_issue(store: &testing::InMemoryProjectStore) -> IssueRevision {
  let project_id = ProjectId::generate();
  store
    .create_project(testing::management_mutation(CreateProject {
      id: project_id,
      parent_id: None,
      name: ProjectName::new("Classification").unwrap(),
      idempotency_key: key("project"),
      created_at: at(1),
    }))
    .await
    .unwrap();
  store
    .create_issue(testing::management_mutation(CreateIssue {
      assignees: Default::default(),
      id: IssueId::generate(),
      intent: CreateIssueIntent {
        project_id,
        planning: IssuePlanning::default(),
        content: IssueContent {
          title: "A defect".into(),
          description: "Initial requirements".into(),
          acceptance: None,
          priority: Some(20.into()),
        },
      },
      idempotency_key: key("issue"),
      created_at: at(2),
    }))
    .await
    .unwrap()
    .issue
}
fn manual_priority() -> Value {
  json!({"fields":[{
    "field":{"kind":"priority"},
    "origin":{"kind":"manual","actor":{"kind":"unauthenticated_management","identity":null}},
    "assessed_version":1,"requirements_version":1,"assessed_at":at(2)
  }]})
}

#[test]
fn retained_classification_rejects_duplicate_locations_across_catalog_versions() {
  let metadata = |first: Value, second: Value| {
    let mut one = manual_priority()["fields"][0].clone();
    let mut two = one.clone();
    one["field"] = first;
    two["field"] = second;
    json!({"fields":[one,two]})
  };
  for (first, second) in [
    (
      json!({"kind":"priority"}),
      json!({"kind":"priority","definition_version":2}),
    ),
    (
      json!({"kind":"component","definition_version":1}),
      json!({"kind":"component","definition_version":2}),
    ),
    (
      json!({"kind":"type","definition_version":1}),
      json!({"kind":"type","definition_version":2}),
    ),
    (json!({"kind":"size"}), json!({"kind":"size"})),
    (json!({"kind":"dependencies"}), json!({"kind":"dependencies"})),
    (
      json!({"kind":"property","id":"effort","definition_version":1}),
      json!({"kind":"property","id":"effort","definition_version":2}),
    ),
  ] {
    assert!(serde_json::from_value::<IssueClassificationMetadata>(metadata(first, second)).is_err());
  }
  let distinct = metadata(
    json!({"kind":"property","id":"effort","definition_version":1}),
    json!({"kind":"property","id":"reproduction","definition_version":2}),
  );
  assert_eq!(
    serde_json::from_value::<IssueClassificationMetadata>(distinct)
      .unwrap()
      .fields
      .len(),
    2
  );
}

#[test]
fn builtin_component_uses_project_choices_independently_of_custom_properties() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = store
      .publish_issue_properties(testing::management_mutation(PublishIssueProperties {
        intent: PublishIssuePropertiesIntent {
          project_id: original.project_id,
          expected_version: 0,
          definition: serde_json::from_value(json!({
            "labels":[],"components":[{"id":"engine","name":"Engine","retired":false}],
            "properties":[{"id":"customer_visible","name":"Customer visible","retired":false,
              "kind":{"type":"boolean"}}]
          }))
          .expect("the Project catalog supports built-in component choices"),
        },
        idempotency_key: key("component-catalog"),
        published_at: at(3),
      }))
      .await
      .unwrap()
      .resource;
    let intent = EditIssuePlanningIntent {
      project_id: original.project_id,
      id: original.id,
      expected_version: original.version,
      planning: serde_json::from_value(json!({
        "definition_version":catalog.version,"component":"engine","type":"feature_request","size":"medium","assignee":null,
        "labels":[],"due_date":null,"parent_id":null,
        "values":{"customer_visible":{"type":"boolean","value":true}}
      }))
      .expect("component is a built-in planning field, outside custom values"),
    };
    let edit = EditIssuePlanning {
      intent,
      assignees: Default::default(),
      idempotency_key: key("component-select"),
      edited_at: at(4),
    };
    let selected = store
      .edit_issue_planning(testing::management_mutation(edit.clone()))
      .await
      .unwrap()
      .issue;
    assert_eq!(
      serde_json::to_value(&selected).unwrap()["planning"]["component"],
      "engine"
    );
    assert_eq!(
      serde_json::to_value(&selected).unwrap()["planning"]["type"],
      "feature_request"
    );
    assert_eq!(serde_json::to_value(&selected).unwrap()["planning"]["size"], "medium");
    assert_eq!(selected.planning.values.len(), 1);
    assert_eq!(
      selected.planning.values["customer_visible"],
      IssuePropertyValue::Boolean(true)
    );
    assert_eq!(selected.applicability_version, original.applicability_version);
    assert_eq!(selected.classification.fields.len(), 4);
    assert!(
      selected
        .classification
        .fields
        .iter()
        .all(|field| matches!(field.origin, IssueClassificationOrigin::Manual { .. })),
      "ordinary planning retains its human origin without inventing a producer"
    );
    assert_eq!(
      store
        .issue(original.project_id, original.id, Some(original.version))
        .await
        .unwrap(),
      original
    );
    assert!(serde_json::to_value(&original).unwrap().get("planning").is_none());
    let mut invalid = edit.clone();
    let mut wire = serde_json::to_value(&invalid.intent.planning).unwrap();
    wire["component"] = json!("another-project-component");
    invalid.intent.planning = serde_json::from_value(wire).unwrap();
    invalid.intent.expected_version = selected.version;
    invalid.idempotency_key = key("foreign-component");
    invalid.edited_at = at(5);
    assert!(
      store
        .edit_issue_planning(testing::management_mutation(invalid))
        .await
        .is_err()
    );
    assert_eq!(
      store.issue(original.project_id, original.id, None).await.unwrap(),
      selected
    );
    let replay = store
      .edit_issue_planning(testing::management_mutation(edit))
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, selected);
  });
}

#[test]
fn issue_priority_uses_project_choices_and_preserves_legacy_numeric_history() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let mut content = serde_json::to_value(&original.content).unwrap();
    content["priority"] = json!("high");
    let content: IssueContent =
      serde_json::from_value(content).expect("new Issue priorities are Project choice IDs, not scheduling numbers");
    let request = EditIssue {
      intent: EditIssueIntent {
        project_id: original.project_id,
        id: original.id,
        expected_version: original.version,
        content,
      },
      idempotency_key: key("priority-choice"),
      edited_at: at(3),
    };
    let selected = store
      .edit_issue(testing::management_mutation(request.clone()))
      .await
      .unwrap()
      .issue;
    assert_eq!(serde_json::to_value(&selected.content).unwrap()["priority"], "high");
    assert_eq!(selected.applicability_version, original.applicability_version);
    assert_eq!(serde_json::to_value(&original.content).unwrap()["priority"], 20);
    assert_eq!(
      store
        .issue(original.project_id, original.id, Some(original.version))
        .await
        .unwrap(),
      original
    );
    let mut invalid = request.clone();
    let mut wire = serde_json::to_value(&invalid.intent.content).unwrap();
    wire["priority"] = json!("urgent");
    invalid.intent.content = serde_json::from_value(wire).unwrap();
    invalid.intent.expected_version = selected.version;
    invalid.idempotency_key = key("unconfigured-priority");
    invalid.edited_at = at(4);
    assert!(
      store
        .edit_issue(testing::management_mutation(invalid.clone()))
        .await
        .is_err()
    );
    store
      .publish_issue_properties(testing::management_mutation(PublishIssueProperties {
        intent: PublishIssuePropertiesIntent {
          project_id: original.project_id,
          expected_version: 0,
          definition: serde_json::from_value(json!({"labels":[],"properties":[],"priorities":[
            {"id":"blocker","name":"Blocker","retired":false},{"id":"high","name":"High","retired":false},
            {"id":"medium","name":"Medium","retired":false},{"id":"low","name":"Low","retired":false},
            {"id":"urgent","name":"Urgent","retired":false}
          ]}))
          .unwrap(),
        },
        idempotency_key: key("project-priorities"),
        published_at: at(4),
      }))
      .await
      .unwrap();
    let custom = store
      .edit_issue(testing::management_mutation(invalid))
      .await
      .unwrap()
      .issue;
    assert_eq!(serde_json::to_value(&custom.content).unwrap()["priority"], "urgent");
    let replay = store.edit_issue(testing::management_mutation(request)).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.issue, selected);
  });
}

#[test]
fn issue_classification_metadata_rejects_reserved_property_ids() {
  let mut retained = manual_priority();
  retained["fields"][0]["field"] = json!({"kind":"property","id":"effort","definition_version":1});
  let valid: IssueClassificationMetadata = serde_json::from_value(retained.clone()).unwrap();
  assert!(valid.validate().is_ok());
  for id in [
    "id",
    "status",
    "priority",
    "accepted",
    "approved",
    "permissions",
    "source_owned",
  ] {
    retained["fields"][0]["field"]["id"] = json!(id);
    assert!(
      serde_json::from_value::<IssueClassificationMetadata>(retained.clone()).is_err(),
      "reserved property ID {id} must not be retained"
    );
  }
}

#[test]
fn issue_classification_metadata_survives_requirements_edits_without_copying_values() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let mut retained = serde_json::to_value(&original).unwrap();
    // Historical documents predate manual attribution; new creations now record it.
    retained.as_object_mut().unwrap().remove("classification");
    assert!(retained.get("classification").is_none());
    let legacy: IssueRevision = serde_json::from_value(retained.clone()).unwrap();
    assert!(legacy.classification.is_empty());
    retained["classification"] = manual_priority();
    let classified: IssueRevision = serde_json::from_value(retained.clone()).unwrap();
    assert_eq!(classified.content.priority, Some(20.into()));
    assert_eq!(serde_json::to_value(&classified).unwrap(), retained);
    let mut content = classified.content.clone();
    content.description = "Changed requirements".into();
    let edited = classified
      .edited(
        &EditIssue {
          intent: EditIssueIntent {
            project_id: classified.project_id,
            id: classified.id,
            expected_version: classified.version,
            content,
          },
          idempotency_key: key("requirements"),
          edited_at: at(3),
        },
        &IssueFieldPolicy::default(),
      )
      .unwrap();
    assert_eq!(edited.version, IssueVersion::new(2).unwrap());
    assert_eq!(edited.applicability_version, edited.version);
    assert_eq!(edited.content.priority, Some(20.into()));
    assert_eq!(
      serde_json::to_value(&edited).unwrap()["classification"],
      manual_priority()
    );
    assert_eq!(serde_json::to_value(&classified).unwrap(), retained);
    // Ordinary management intents cannot supply attribution or evidence.
    let mut forged = json!({"project_id":original.project_id,"content":original.content});
    forged["classification"] = manual_priority();
    assert!(serde_json::from_value::<CreateIssueIntent>(forged).is_err());
    assert_eq!(
      store
        .issue(original.project_id, original.id, Some(original.version))
        .await
        .unwrap(),
      original
    );
  });
}

fn pin() -> Value {
  json!({"identity":"reproduction.check","version":"v1","digest":"a".repeat(64)})
}
fn assessment(project_id: ProjectId) -> Value {
  let work_id = uuid::Uuid::new_v4().to_string();
  let run_id = uuid::Uuid::new_v4().to_string();
  let mut field = manual_priority()["fields"][0].clone();
  field["field"] = json!({"kind":"property","id":"reproduction","definition_version":1});
  field["origin"] = json!({"kind":"pipeline", "actor":{"kind":"unauthenticated_management","identity":null},
    "work_id":work_id,"run_id":run_id,"record":"b".repeat(64),"policy":"triage.classify","operation_id":"c".repeat(64)});
  field["assessed_version"] = json!(2);
  field["assessed_at"] = json!(at(3));
  field["evidence"] = json!(["d".repeat(64)]);
  let mut dependencies = field.clone();
  dependencies["field"] = json!({"kind":"dependencies"});
  dependencies.as_object_mut().unwrap().remove("evidence");
  dependencies["origin"] = json!({"kind":"source","actor":{"kind":"authenticated_management","identity":"operator"},
    "mapping":pin(),"observation":"e".repeat(64),"native_correlation":"f".repeat(64)});
  json!({"fields":[manual_priority()["fields"][0].clone(),field,dependencies],
    "evidence":[{"work_id":work_id,"run_id":run_id,"record":"d".repeat(64),
      "subject":{"kind":"exact","value":{"project_id":project_id,"repository_id":uuid::Uuid::new_v4().to_string(),"base_revision":"abc123"}},
      "environment":pin(),"criterion":pin(),"observed_at":at(3),"fresh_until":at(10)}],
    "dependencies":[{"work_id":uuid::Uuid::new_v4().to_string(),"run_id":uuid::Uuid::new_v4().to_string(),"work_digest":"a".repeat(64),"terminal":"finished"}]})
}

#[test]
fn issue_classification_metadata_retains_project_fields_and_exact_references_across_planning() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = store.publish_issue_properties(testing::management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent { project_id: original.project_id, expected_version: 0,
        definition: serde_json::from_value(json!({"labels":[],"properties":[
          {"id":"reproduction","name":"Reproduction","retired":false,"kind":{"type":"choice","choices":[{"id":"unknown","name":"Unresolved","retired":false}]}}
        ],"classification":{"reproducibility":"reproduction"}})).unwrap() },
      idempotency_key: key("catalog"), published_at: at(3),
    })).await.unwrap().resource;
    let planned = store
      .edit_issue_planning(testing::management_mutation(EditIssuePlanning {
        assignees: Default::default(),
        intent: EditIssuePlanningIntent {
          project_id: original.project_id,
          id: original.id,
          expected_version: original.version,
          planning: serde_json::from_value(
            json!({"definition_version":1,"assignee":null,"labels":[],"due_date":null,"parent_id":null,
          "values":{"reproduction":{"type":"choice","value":"unknown"}}}),
          )
          .unwrap(),
        },
        idempotency_key: key("planning"),
        edited_at: at(3),
      }))
      .await
      .unwrap()
      .issue;
    let metadata = assessment(original.project_id);
    let mut retained = serde_json::to_value(&planned).unwrap();
    retained["classification"] = metadata.clone();
    let classified: IssueRevision = serde_json::from_value(retained).unwrap();
    let mut plan = classified.planning.clone();
    plan.due_date = Some("2026-12-01".into());
    let next = classified
      .planned(
        &EditIssuePlanning {
          assignees: Default::default(),
          intent: EditIssuePlanningIntent {
            project_id: classified.project_id,
            id: classified.id,
            expected_version: classified.version,
            planning: plan,
          },
          idempotency_key: key("due-date"),
          edited_at: at(4),
        },
        &IssueFieldPolicy::default(),
        &catalog,
      )
      .unwrap();
    assert_eq!(next.applicability_version, classified.applicability_version);
    assert_eq!(next.planning.values, classified.planning.values);
    assert_eq!(serde_json::to_value(&next).unwrap()["classification"], metadata);
    assert_eq!(serde_json::to_value(&classified).unwrap()["classification"], metadata);
    assert_eq!(
      store
        .issue(original.project_id, original.id, Some(planned.version))
        .await
        .unwrap(),
      planned
    );
  });
}

#[test]
fn issue_classification_metadata_rejects_unbounded_and_ambiguous_retained_data() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let mut valid = assessment(original.project_id);
    for field in valid["fields"].as_array_mut().unwrap() {
      field["assessed_version"] = json!(1);
    }
    let mut cases = vec![];
    let mut duplicate = manual_priority();
    duplicate["fields"]
      .as_array_mut()
      .unwrap()
      .push(manual_priority()["fields"][0].clone());
    cases.push(duplicate);
    for id in ["", "x y", "Uppercase", &"x".repeat(65)] {
      let mut bad = valid.clone();
      bad["fields"][1]["field"]["id"] = json!(id);
      cases.push(bad);
    }
    for (path, value) in [
      ("/fields/1/field/definition_version", json!(0)),
      ("/fields/0/requirements_version", json!(2)),
      ("/fields/0/origin/actor/kind", json!("worker")),
      ("/fields/0/origin/actor/identity", json!("invented-person")),
      ("/fields/1/evidence/0", json!("0".repeat(64))),
      ("/evidence/0/fresh_until", json!(at(3))),
    ] {
      let mut bad = valid.clone();
      *bad.pointer_mut(path).unwrap() = value;
      cases.push(bad);
    }
    for field in ["evidence", "dependencies"] {
      let mut bad = valid.clone();
      let entry = bad[field][0].clone();
      bad[field].as_array_mut().unwrap().push(entry.clone());
      cases.push(bad);
      let mut oversized = valid.clone();
      oversized[field] = json!(
        (0..65)
          .map(|i| {
            let mut item = entry.clone();
            if field == "evidence" && i > 0 {
              item["record"] = json!(format!("{i:064x}"));
            } else if field == "dependencies" {
              item["run_id"] = json!(uuid::Uuid::new_v4().to_string());
            }
            item
          })
          .collect::<Vec<_>>()
      );
      cases.push(oversized);
    }
    let mut ambiguous_property = valid.clone();
    let mut same_property = ambiguous_property["fields"][1].clone();
    same_property["field"]["definition_version"] = json!(2);
    ambiguous_property["fields"].as_array_mut().unwrap().push(same_property);
    cases.push(ambiguous_property);
    let mut repeated_citation = valid.clone();
    repeated_citation["fields"][1]["evidence"] = json!(["d".repeat(64), "d".repeat(64)]);
    cases.push(repeated_citation);
    let mut correlation = valid.clone();
    correlation["evidence"][0]["subject"] =
      json!({"kind":"work_input_v1","value":valid["evidence"][0]["subject"]["value"]});
    cases.push(correlation);
    let mut invented_actor = manual_priority();
    invented_actor["fields"][0]["origin"]["actor"] = json!({"kind":"authenticated_management","identity":null});
    cases.push(invented_actor);
    let mut oversized_actor = manual_priority();
    oversized_actor["fields"][0]["origin"]["actor"] =
      json!({"kind":"authenticated_management","identity":"x".repeat(MAX_AUDIT_ACTOR_IDENTITY_BYTES+1)});
    cases.push(oversized_actor);
    let mut oversized = manual_priority();
    oversized["fields"] = json!(
      (0..35)
        .map(|i| {
          let mut field = manual_priority()["fields"][0].clone();
          field["field"] = json!({"kind":"property","id":format!("field_{i}"),"definition_version":1});
          field
        })
        .collect::<Vec<_>>()
    );
    cases.push(oversized);
    let mut copied_values = valid.clone();
    copied_values["fields"][0]["value"] = json!(20);
    cases.push(copied_values);
    let mut false_authority = valid.clone();
    false_authority["verified"] = json!(true);
    cases.push(false_authority);
    for (index, metadata) in cases.into_iter().enumerate() {
      let mut retained = serde_json::to_value(&original).unwrap();
      retained["classification"] = metadata;
      assert!(
        serde_json::from_value::<IssueRevision>(retained).is_err(),
        "accepted invalid metadata case {index}"
      );
    }
  });
}
