//! Issue-based release resolution and ordinary publication, without producer reads.
use super::*;

#[path = "issue_delivery_route_tests.rs"]
mod routes;

async fn issue_selection(fixture: &DeliveryFixture) -> FactoryPresetResolutionRequest {
  let page = AuthorizedQueryHandler::new(
    fixture.policy.clone(),
    Arc::new(FactoryPresetHandlers::new(
      fixture.store.clone(),
      fixture.policy.clone(),
    )),
  )
  .handle_query(
    &fixture.context,
    ListFactoryPresetsQuery {
      page: CurrentDefinitionPageInput::try_new(fixture.project, None, 100).unwrap(),
    },
  )
  .await
  .unwrap();
  FactoryPresetResolutionRequest {
    preset: page
      .items
      .into_iter()
      .find(|item| item.reference.identity() == &key("ticket-delivery") && item.reference.version() == &key("v2"))
      .unwrap()
      .reference,
    accepted_predecessor: None,
    ..fixture.selection.clone()
  }
}

#[test]
fn issue_delivery_requires_project_input_contract_instead_of_a_producer() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let selection = issue_selection(&fixture).await;
    // No producing-Flow reader is installed at this public boundary.
    let resolver = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone()),
      ),
    );
    let result = resolver
      .handle_query(
        &fixture.context,
        ResolveFactoryPresetQuery {
          project_id: fixture.project,
          selection,
        },
      )
      .await
      .unwrap();
    assert!(result.flow.is_none());
    assert_eq!(
      result.diagnostics.iter().map(|d| d.path.as_str()).collect::<Vec<_>>(),
      ["/issue_snapshot"]
    );
  });
}

async fn properties(project: ProjectId) -> Arc<octacity_server_store::testing::InMemoryProjectStore> {
  use octacity_server_store::testing::{InMemoryProjectStore, management_mutation};
  use octacity_server_store::*;
  let store = Arc::new(InMemoryProjectStore::new());
  store
    .create_project(management_mutation(CreateProject {
      id: project,
      parent_id: None,
      name: "Issue delivery".parse().unwrap(),
      idempotency_key: "project".parse().unwrap(),
      created_at: Timestamp::from_unix_millis(0).unwrap(),
    }))
    .await
    .unwrap();
  let property = |id: &str, values: &[&str]| serde_json::json!({"id":id,"name":id,"retired":false,"kind":{"type":"choice","choices":values.iter().map(|value|serde_json::json!({"id":value,"name":value,"retired":false})).collect::<Vec<_>>()}});
  let mut definition:IssuePropertyDefinitionSet=serde_json::from_value(serde_json::json!({
    "labels":[],"components":[{"id":"compiler","name":"Compiler","retired":false},{"id":"api","name":"API","retired":false}],
    "properties":[property("impact",&["advisory","low","high"]),property("risk",&["low","high"]),property("route",&["requirements","research"]),property("repro",&["unknown","needs_input","not_applicable"])],
    "classification":{"severity":"impact","risk":"risk","recommended_route":"route","reproducibility":"repro"}
  })).unwrap();
  definition.types.push(IssuePropertyChoice {
    id: "maintenance".into(),
    name: "Maintenance".into(),
    retired: false,
  });
  definition.priorities.push(IssuePropertyChoice {
    id: "urgent_local".into(),
    name: "Urgent locally".into(),
    retired: false,
  });
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: project,
        expected_version: 0,
        definition,
      },
      idempotency_key: "definitions".parse().unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    }))
    .await
    .unwrap();
  store
}

fn issue_input() -> FactoryPresetIssueSnapshotSelection {
  FactoryPresetIssueSnapshotSelection {
    definition_version: 1,
    admission: Some(
      serde_json::from_value(serde_json::json!({
        "priority":{"field":"priority","values":{"blocker":100,"high":80,"medium":50,"low":20,"urgent_local":100}},
        "risk":{"field":"risk","values":{"low":"low","high":"high"}},
        "allowed":{"work_kind":["defect","feature_request"]}
      }))
      .unwrap(),
    ),
  }
}

#[test]
fn issue_delivery_resolves_and_publishes_project_contract_without_any_producer() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let properties = properties(fixture.project).await;
    let mut selection = issue_selection(&fixture).await;
    selection.issue_snapshot = Some(issue_input());
    let resolver = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone())
          .with_issue_properties(properties.clone()),
      ),
    );
    let before = fixture.store.mutation_evidence_counts().await;
    let result = resolver
      .handle_query(
        &fixture.context,
        ResolveFactoryPresetQuery {
          project_id: fixture.project,
          selection: selection.clone(),
        },
      )
      .await
      .unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
      fixture.store.mutation_evidence_counts().await,
      before,
      "resolution is read-only"
    );
    let draft = result
      .flow
      .expect("an Issue-based draft is executable without producer metadata");
    assert!(draft.accepted_predecessor.is_none());
    let flow_only = resolver
      .handle_query(
        &fixture.context,
        ResolveFactoryPresetQuery {
          project_id: fixture.project,
          selection: FactoryPresetResolutionRequest {
            flow_only: true,
            configuration_id: None,
            configuration: None,
            repository: None,
            ..selection.clone()
          },
        },
      )
      .await
      .unwrap();
    assert!(flow_only.diagnostics.is_empty());
    assert_eq!(flow_only.flow.as_ref(), Some(&draft));
    assert!(flow_only.configuration.is_none() && flow_only.input.is_none());
    let input = draft
      .issue_snapshot
      .as_ref()
      .expect("delivery must retain its canonical Issue input contract");
    assert_eq!(input.definition_version, 1);
    assert!(input.materials.is_empty());
    assert_eq!(input.fields[&key("priority")].source, FlowIssueFieldSource::Priority);
    assert_eq!(input.fields[&key("component")].source, FlowIssueFieldSource::Component);
    assert_eq!(input.fields[&key("work_kind")].source, FlowIssueFieldSource::Type);
    assert_eq!(input.fields[&key("size")].source, FlowIssueFieldSource::Size);
    let schema = draft
      .data_schemas
      .iter()
      .find(|schema| schema.reference() == &input.schema)
      .unwrap();
    let shape = serde_json::to_value(schema.shape()).unwrap();
    assert_eq!(
      shape["fields"]["priority"]["value"]["values"],
      serde_json::json!(["blocker", "high", "medium", "low", "urgent_local"])
    );
    assert_eq!(
      shape["fields"]["component"]["value"]["values"],
      serde_json::json!(["compiler", "api"])
    );
    assert!(
      draft
        .root
        .nodes
        .iter()
        .flat_map(|node| node.input_binding().into_iter().flat_map(|b| b.incoming_schemas()))
        .all(|schema| schema == &input.schema)
    );
    let policy = fixture.policy.clone();
    let published = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        FactoryFlowDefinitionHandlers::new(
          Arc::new(PublicationChoices::for_publication(fixture.installed.clone())),
          policy,
        )
        .with_issue_properties(properties.clone()),
      ),
    )
    .handle_command(
      &fixture.context,
      CreateFactoryFlowDefinitionCommand {
        id: selection.flow_id.unwrap(),
        project_id: fixture.project,
        draft: draft.clone(),
        idempotency_key: "issue-delivery-flow".parse().unwrap(),
        published_at: Timestamp::from_unix_millis(2).unwrap(),
      },
    )
    .await
    .unwrap();
    assert!(published.publication.flow.accepted_predecessor.is_none());
    assert_eq!(published.publication.flow.issue_snapshot, draft.issue_snapshot);
    let configuration = result.configuration.unwrap();
    assert!(!configuration.enabled);
    assert!(result.input.is_none());
    let saved = AuthorizedCommandHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryConfigurationHandlers::with_policy(fixture.store.clone(), fixture.policy.clone())
          .with_issue_properties(properties),
      ),
    )
    .handle_command(
      &fixture.context,
      CreateFactoryConfigurationCommand {
        id: selection.configuration_id.unwrap(),
        project_id: fixture.project,
        definition_digest: FactoryDigest::content_sha256(&serde_json::to_vec(&configuration).unwrap()),
        draft: configuration,
        idempotency_key: "issue-delivery-configuration".parse().unwrap(),
        published_at: Timestamp::from_unix_millis(3).unwrap(),
      },
    )
    .await
    .unwrap();
    assert!(!saved.configuration.configuration.is_enabled());
  });
}

#[test]
fn issue_delivery_requires_reviewed_admission_and_supported_kind_allowlist() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let properties = properties(fixture.project).await;
    let resolver = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone())
          .with_issue_properties(properties),
      ),
    );
    for case in ["mapping", "kinds", "unsupported_kind"] {
      let mut selection = issue_selection(&fixture).await;
      let mut input = issue_input();
      match case {
        "mapping" => input.admission = None,
        "kinds" => input.admission.as_mut().unwrap().allowed.clear(),
        "unsupported_kind" => {
          input
            .admission
            .as_mut()
            .unwrap()
            .allowed
            .insert(key("work_kind"), ["maintenance".to_owned()].into());
        }
        _ => unreachable!(),
      }
      selection.issue_snapshot = Some(input);
      let result = resolver
        .handle_query(
          &fixture.context,
          ResolveFactoryPresetQuery {
            project_id: fixture.project,
            selection,
          },
        )
        .await
        .unwrap();
      assert!(
        result.flow.is_none(),
        "{case}: an Issue choice must not infer admission or a delivery route"
      );
      assert_eq!(result.diagnostics[0].path, "/issue_snapshot/admission");
    }
  });
}

#[test]
fn issue_delivery_preserves_installed_executor_publication_and_access_checks() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let properties = properties(fixture.project).await;
    let resolver = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone())
          .with_issue_properties(properties),
      ),
    );
    let mut selected = issue_selection(&fixture).await;
    selected.issue_snapshot = Some(issue_input());
    let profile = selected.profiles.keys().next().unwrap().clone();
    for (case, path) in [
      ("profile", format!("/profiles/{profile}")),
      ("command", "/commands/protected.validation".into()),
      ("role_only_command", "/commands/reproduction".into()),
      ("publication", "/publication".into()),
      ("target", "/publication/target".into()),
      ("definitions", "/issue_snapshot/definition_version".into()),
    ] {
      let mut selection = selected.clone();
      match case {
        "profile" => {
          selection.profiles.remove(&profile);
        }
        "command" => {
          selection
            .commands
            .remove(&key("protected.validation"))
            .expect("installed protected-validation command");
        }
        "role_only_command" => {
          selection
            .commands
            .get_mut(&key("reproduction"))
            .unwrap()
            .build
            .model_or_tool = fixture.build.model_or_tool.clone()
        }
        "publication" => selection.publication = None,
        "target" => {
          selection.publication.as_mut().unwrap().target =
            ImmutableReference::new(key("uninstalled"), key("v1"), FactoryDigest::from_bytes([92; 32]))
        }
        "definitions" => selection.issue_snapshot.as_mut().unwrap().definition_version = 999,
        _ => unreachable!(),
      }
      let result = resolver
        .handle_query(
          &fixture.context,
          ResolveFactoryPresetQuery {
            project_id: fixture.project,
            selection,
          },
        )
        .await
        .unwrap();
      assert!(result.flow.is_none(), "{case}");
      assert_eq!(result.diagnostics[0].path, path, "{case}");
    }
    let mut conflicting = selected.clone();
    conflicting.accepted_predecessor = fixture.selection.accepted_predecessor.clone();
    assert!(
      resolver
        .handle_query(
          &fixture.context,
          ResolveFactoryPresetQuery {
            project_id: fixture.project,
            selection: conflicting
          }
        )
        .await
        .is_err(),
      "competing Issue and producer inputs are forbidden"
    );
    let mut no_access = selected;
    let execution = no_access.execution.as_ref().unwrap();
    no_access.execution = Some(
      FlowExecutionPolicy::new(
        execution.budget(),
        FactoryPermissionSet::deny_all(),
        execution.max_active_nodes(),
      )
      .unwrap(),
    );
    assert!(
      resolver
        .handle_query(
          &fixture.context,
          ResolveFactoryPresetQuery {
            project_id: fixture.project,
            selection: no_access
          }
        )
        .await
        .is_err(),
      "the new input cannot increase the Flow access ceiling"
    );
  });
}

#[test]
fn issue_delivery_reports_the_project_semantic_binding_that_is_unavailable() {
  run_ready(async {
    use octacity_server_store::testing::management_mutation;
    use octacity_server_store::{IssuePropertyStore, PublishIssueProperties, PublishIssuePropertiesIntent};
    let fixture = delivery_fixture().await;
    let properties = properties(fixture.project).await;
    let mut definition = properties
      .issue_properties(fixture.project, None)
      .await
      .unwrap()
      .definition;
    definition.classification.reproducibility = None;
    properties
      .publish_issue_properties(management_mutation(PublishIssueProperties {
        intent: PublishIssuePropertiesIntent {
          project_id: fixture.project,
          expected_version: 1,
          definition,
        },
        idempotency_key: "unbind-reproduction".parse().unwrap(),
        published_at: Timestamp::from_unix_millis(2).unwrap(),
      }))
      .await
      .unwrap();
    let mut selection = issue_selection(&fixture).await;
    let mut input = issue_input();
    input.definition_version = 2;
    selection.issue_snapshot = Some(input);
    let result = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone())
          .with_issue_properties(properties),
      ),
    )
    .handle_query(
      &fixture.context,
      ResolveFactoryPresetQuery {
        project_id: fixture.project,
        selection,
      },
    )
    .await
    .unwrap();
    assert!(result.flow.is_none());
    assert_eq!(result.diagnostics[0].path, "/issue_snapshot/fields/reproducibility");
  });
}
