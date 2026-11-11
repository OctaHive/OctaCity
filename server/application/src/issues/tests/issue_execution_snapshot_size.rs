//! Selected values stay bounded independently of unrelated Project dictionaries.
use super::*;

#[test]
fn snapshot_admits_small_selected_values_after_many_large_catalog_publications() {
  run_ready(async {
    let (store, factory, mut issue, mut command) = snapshot_fixture().await;
    let mut definition = store
      .issue_properties(issue.project_id, Some(0))
      .await
      .unwrap()
      .definition;
    let choice = |i| IssuePropertyChoice {
      id: format!("c{i:02}{}", "x".repeat(61)),
      name: "N".repeat(128),
      retired: false,
    };
    definition.labels = (0..64).map(choice).collect();
    definition.components = (0..64).map(choice).collect();
    definition.types.extend((0..62).map(choice));
    definition.properties = (0..32)
      .map(|i| IssuePropertyDefinition {
        id: format!("p{i}"),
        name: "P".repeat(128),
        retired: false,
        kind: IssuePropertyKind::Choice {
          choices: vec![IssuePropertyChoice {
            id: "low".into(),
            name: "L".repeat(128),
            retired: false,
          }],
        },
      })
      .collect();
    let mut publications = std::collections::BTreeMap::new();
    for i in 0..32 {
      definition.classification.risk = Some(format!("p{i}"));
      let catalog = store
        .publish_issue_properties(management_mutation(PublishIssueProperties {
          intent: PublishIssuePropertiesIntent {
            project_id: issue.project_id,
            expected_version: i,
            definition: definition.clone(),
          },
          idempotency_key: key(&format!("large-catalog-{i}")),
          published_at: at(31 + 2 * i as i64),
        }))
        .await
        .unwrap()
        .resource;
      publications.insert(catalog.version, catalog);
      let mut planning = issue.planning.clone();
      planning.definition_version = i + 1;
      planning
        .values
        .insert(format!("p{i}"), IssuePropertyValue::Choice("low".into()));
      issue = store
        .edit_issue_planning(management_mutation(EditIssuePlanning {
          intent: EditIssuePlanningIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: issue.version,
            planning,
          },
          assignees: Default::default(),
          idempotency_key: key(&format!("assess-property-{i}")),
          edited_at: at(32 + 2 * i as i64),
        }))
        .await
        .unwrap()
        .issue;
    }
    let id = FactoryConfigurationId::generate();
    let fields = (0..32)
      .map(|i| {
        (
          format!("p{i}").parse().unwrap(),
          FlowIssueFieldSelection {
            source: FlowIssueFieldSource::Property { id: format!("p{i}") },
            required: true,
          },
        )
      })
      .collect();
    crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
      &factory,
      PublishedFactoryConfiguration {
        configuration: snapshot_configuration_with_text_fields(issue.project_id, id, 32, fields),
        published_at: at(95),
      },
    )
    .unwrap();
    command.intent.target.configuration_id = id;
    command.intent.expected_version = issue.version;
    command.requested_at = at(100);
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let request = requests
      .handle_command(&context(), command)
      .await
      .expect("small selected input must fit without copying unrelated Project dictionaries")
      .request;
    let published = publish_snapshot_request(store, factory.clone(), &request).await;
    let run = factory
      .factory_run_snapshot(published.association.unwrap().run_id)
      .await
      .unwrap();
    let payload = run.flow.data.incoming[0].payload().value().as_object().unwrap();
    assert_eq!(payload.len(), 32);
    assert!(payload.values().all(|value| value == "low"));
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
    let bytes = serde_json::to_vec(snapshot).unwrap();
    assert!(
      bytes.len() < 65_536,
      "selected field definitions should fit within 64 KiB, got {}",
      bytes.len()
    );
    assert_eq!(snapshot.definitions.len(), 32);
    assert_eq!(snapshot.digest().unwrap(), request.snapshot_digest.unwrap());
    // The first fragment keeps the original selected property's vocabulary/meaning
    // and a pin to the complete publication, without unrelated dictionaries.
    let wire = serde_json::to_value(snapshot).unwrap();
    let first = &wire["definitions"]["1"]["selected_v1"];
    assert_eq!(first["project_id"], issue.project_id.to_string());
    assert_eq!(first["version"], 1);
    assert_eq!(
      first["publication_digest"],
      serde_json::to_value(FactoryDigest::content_sha256(
        &serde_json::to_vec(&publications[&1]).unwrap()
      ))
      .unwrap()
    );
    assert_eq!(first["properties"].as_array().unwrap().len(), 1);
    assert_eq!(first["properties"][0]["definition"]["id"], "p0");
    assert_eq!(first["properties"][0]["definition"]["kind"]["choices"][0]["id"], "low");
    assert_eq!(first["properties"][0]["classification_slot"], "risk");
    assert!(first.get("labels").is_none());
    assert!(first.get("components").is_none());
    assert!(first.get("types").is_none());
    assert!(first.get("priorities").is_none());
  });
}
