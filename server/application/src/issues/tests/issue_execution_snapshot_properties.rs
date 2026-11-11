//! Ordinary Project fields are frozen data without classification or producer requirements.
use super::*;

#[path = "issue_execution_snapshot_properties_fixture.rs"]
mod ordinary;
use ordinary::*;
#[path = "issue_execution_snapshot_properties_bounds.rs"]
mod bounds;
#[path = "issue_execution_snapshot_properties_presence.rs"]
mod presence;

#[test]
fn ordinary_property_snapshot_freezes_native_scalar_values_and_exact_definitions() {
  run_ready(async {
    let (store, factory, issue, command) = ordinary_fixture().await;
    assert!(issue.classification.fields.is_empty());
    assert!(issue.content.priority.is_none());
    assert!(issue.planning.size.is_none());
    assert!(
      store
        .issue_properties(issue.project_id, Some(1))
        .await
        .unwrap()
        .definition
        .classification
        .fields()
        .next()
        .is_none()
    );
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let request = requests.handle_command(&context(), command).await.unwrap().request;
    assert!(request.intent.target.predecessor_run.is_none());
    let published = publish_snapshot_request(store.clone(), factory.clone(), &request).await;
    let run = factory
      .factory_run_snapshot(published.association.unwrap().run_id)
      .await
      .unwrap();
    let incoming = &run.flow.data.incoming[0];
    assert!(incoming.predecessor().is_none());
    assert_eq!(
      incoming.payload().value(),
      &serde_json::json!({
        "notes":"ёж","visible":false,"points":-2,"date":"2028-02-29","environment":"test"
      })
    );
    let documents = factory
      .project_text_documents(
        issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let snapshot = documents.issue_snapshot().unwrap();
    assert_eq!(snapshot.digest().unwrap(), request.snapshot_digest.unwrap());
    assert!(snapshot.attribution.is_empty());
    assert_eq!(
      snapshot.omissions.get(&"optional_context".parse().unwrap()),
      Some(&IssueSnapshotOmission::Missing)
    );
    let definition = store.issue_properties(issue.project_id, Some(1)).await.unwrap();
    let wire = serde_json::to_value(snapshot).unwrap();
    let selected = &wire["definitions"]["1"]["selected_v1"];
    assert_eq!(
      selected["publication_digest"],
      serde_json::to_value(FactoryDigest::content_sha256(&serde_json::to_vec(&definition).unwrap())).unwrap()
    );
    let properties = selected["properties"].as_array().unwrap();
    assert_eq!(properties.len(), 6);
    for (id, kind) in [
      ("release_notes", serde_json::json!({"type":"text","max_bytes":4})),
      ("customer_visible", serde_json::json!({"type":"boolean"})),
      (
        "cost_points",
        serde_json::json!({"type":"number","minimum":-2,"maximum":10}),
      ),
      ("target_date", serde_json::json!({"type":"date"})),
      (
        "environment",
        serde_json::json!({"type":"choice","choices":[
        {"id":"test","name":"Test","retired":false},{"id":"prod","name":"Production","retired":false}]}),
      ),
      ("optional_context", serde_json::json!({"type":"text","max_bytes":12})),
    ] {
      let property = properties.iter().find(|p| p["definition"]["id"] == id).unwrap();
      assert_eq!(property["definition"]["kind"], kind);
      assert!(property.get("classification_slot").is_none());
    }
    assert!(properties.iter().all(|p| p["definition"]["id"] != "unselected_note"));
  });
}

#[test]
fn ordinary_property_snapshot_keeps_values_and_definitions_after_later_publications() {
  run_ready(async {
    let (store, factory, issue, command) = ordinary_fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let request = requests
      .handle_command(&context(), command.clone())
      .await
      .unwrap()
      .request;
    let original = publish_snapshot_request(store.clone(), factory.clone(), &request).await;
    let association = original.association.as_ref().unwrap();
    let run = factory.factory_run_snapshot(association.run_id).await.unwrap();
    let documents = factory
      .project_text_documents(
        issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let mut definition = store
      .issue_properties(issue.project_id, Some(1))
      .await
      .unwrap()
      .definition;
    for property in &mut definition.properties {
      match property.id.as_str() {
        "release_notes" => {
          property.name = "Renamed notes".into();
          property.kind = IssuePropertyKind::Text { max_bytes: 8 };
        }
        "cost_points" => {
          property.kind = IssuePropertyKind::Number {
            minimum: -3,
            maximum: 20,
          }
        }
        "environment" => {
          if let IssuePropertyKind::Choice { choices } = &mut property.kind {
            choices.iter_mut().find(|c| c.id == "test").unwrap().retired = true;
            choices.push(IssuePropertyChoice {
              id: "canary".into(),
              name: "Canary".into(),
              retired: false,
            });
          }
        }
        _ => {}
      }
    }
    store
      .publish_issue_properties(management_mutation(PublishIssueProperties {
        intent: PublishIssuePropertiesIntent {
          project_id: issue.project_id,
          expected_version: 1,
          definition,
        },
        idempotency_key: key("later-ordinary-definitions"),
        published_at: at(50),
      }))
      .await
      .unwrap();
    let later = store
      .edit_issue_planning(management_mutation(EditIssuePlanning {
        intent: EditIssuePlanningIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          planning: IssuePlanning {
            definition_version: 2,
            values: [
              ("release_notes".into(), IssuePropertyValue::Text("later".into())),
              ("customer_visible".into(), IssuePropertyValue::Boolean(true)),
              ("cost_points".into(), IssuePropertyValue::Number(20)),
              ("target_date".into(), IssuePropertyValue::Date("2029-12-31".into())),
              ("environment".into(), IssuePropertyValue::Choice("canary".into())),
              (
                "optional_context".into(),
                IssuePropertyValue::Text("ADDED_LATER".into()),
              ),
            ]
            .into(),
            ..issue.planning
          },
        },
        assignees: Default::default(),
        idempotency_key: key("later-ordinary-values"),
        edited_at: at(51),
      }))
      .await
      .unwrap()
      .issue;
    let before = factory.mutation_evidence_counts().await;
    let replay = requests.handle_command(&context(), command).await.unwrap().request;
    assert_eq!(replay, request);
    assert_eq!(
      publish_snapshot_request(store.clone(), factory.clone(), &request).await,
      original
    );
    assert_eq!(factory.factory_run_snapshot(association.run_id).await.unwrap(), run);
    assert_eq!(
      factory
        .project_text_documents(
          issue.project_id,
          &context().security_scope().to_store(),
          &request.admission_key()
        )
        .await
        .unwrap()
        .unwrap(),
      documents
    );
    assert_eq!(factory.mutation_evidence_counts().await, before);
    assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), later);
  });
}
