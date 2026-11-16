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

pub(super) async fn properties(project: ProjectId) -> Arc<octacity_server_store::testing::InMemoryProjectStore> {
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
    fields: Default::default(),
    choice_thresholds: [(key("requirements_needed"), "large".into())].into(),
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
fn template_discovery_and_resolution_require_the_configured_choice_threshold() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let resolver = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone())
          .with_issue_properties(properties(fixture.project).await),
      ),
    );
    let page = resolver
      .handle_query(
        &fixture.context,
        ListFactoryPresetsQuery {
          page: CurrentDefinitionPageInput::try_new(fixture.project, None, 100).unwrap(),
        },
      )
      .await
      .unwrap();
    let metadata = serde_json::to_value(
      page
        .items
        .iter()
        .find(|item| item.reference.identity() == &key("ticket-delivery") && item.reference.version() == &key("v2"))
        .unwrap(),
    )
    .unwrap();
    assert!(
      metadata["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["kind"] == "choice_threshold"
          && p["path"] == "/issue_snapshot/choice_thresholds/requirements_needed"
          && p["required"] == true)
    );
    assert_eq!(
      metadata["editor"]["choice_thresholds"]["requirements_needed"],
      serde_json::json!("size")
    );
    let mut selection = issue_selection(&fixture).await;
    selection.issue_snapshot = Some(
      serde_json::from_value(serde_json::json!({
        "definition_version":1,"admission":issue_input().admission,"choice_thresholds":{}
      }))
      .unwrap(),
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
      result.diagnostics[0].path,
      "/issue_snapshot/choice_thresholds/requirements_needed"
    );
  });
}

#[test]
fn delivery_template_reads_configured_property_ids_without_classification_roles() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let properties = properties(fixture.project).await;
    let mut definition = properties
      .issue_properties(fixture.project, None)
      .await
      .unwrap()
      .definition;
    definition.classification = Default::default();
    properties
      .publish_issue_properties(octacity_server_store::testing::management_mutation(
        octacity_server_store::PublishIssueProperties {
          intent: octacity_server_store::PublishIssuePropertiesIntent {
            project_id: fixture.project,
            expected_version: 1,
            definition,
          },
          idempotency_key: "direct-delivery-fields".parse().unwrap(),
          published_at: Timestamp::from_unix_millis(2).unwrap(),
        },
      ))
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
          .with_issue_properties(properties.clone()),
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
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let input = result.flow.unwrap().issue_snapshot.unwrap();
    assert_eq!(
      input.fields[&key("severity")].source,
      FlowIssueFieldSource::Property { id: "impact".into() }
    );
    assert_eq!(
      input.fields[&key("risk")].source,
      FlowIssueFieldSource::Property { id: "risk".into() }
    );
    assert!(!result.configuration.unwrap().enabled);
    assert!(result.input.is_none());
  });
}

#[test]
fn issue_delivery_custom_sizes_require_an_active_explicit_threshold() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let issues = properties(fixture.project).await;
    let mut definition = issues.issue_properties(fixture.project, None).await.unwrap().definition;
    definition.sizes[0].retired = true;
    definition.sizes.push(octacity_server_store::IssuePropertyChoice {
      id: "xl".into(),
      name: "Extra large".into(),
      retired: false,
    });
    issues
      .publish_issue_properties(octacity_server_store::testing::management_mutation(
        octacity_server_store::PublishIssueProperties {
          intent: octacity_server_store::PublishIssuePropertiesIntent {
            project_id: fixture.project,
            expected_version: 1,
            definition,
          },
          idempotency_key: "size-threshold-errors".parse().unwrap(),
          published_at: Timestamp::from_unix_millis(2).unwrap(),
        },
      ))
      .await
      .unwrap();
    let before = issues.mutation_evidence_counts().await;
    let resolver = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone())
          .with_issue_properties(issues.clone()),
      ),
    );
    for threshold in [None, Some("foreign_size"), Some("small")] {
      let mut selection = issue_selection(&fixture).await;
      let mut input = issue_input();
      input.definition_version = 2;
      input.choice_thresholds = threshold
        .map(|threshold| [(key("requirements_needed"), threshold.to_owned())].into())
        .unwrap_or_default();
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
      assert!(result.flow.is_none());
      assert_eq!(
        result.diagnostics.iter().map(|d| d.path.as_str()).collect::<Vec<_>>(),
        ["/issue_snapshot/choice_thresholds/requirements_needed"]
      );
    }
    assert_eq!(issues.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_delivery_command_budget_covers_its_selected_execution_request() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let mut selection = issue_selection(&fixture).await;
    selection.issue_snapshot = Some(issue_input());
    let execution = selection.execution.as_ref().unwrap();
    let mut permissions = serde_json::to_value(execution.permissions()).unwrap();
    permissions["resources"]["elapsed_millis"] = serde_json::json!(180_000);
    permissions["outputs"] = serde_json::json!({
      "kinds":["report"], "max_artifact_count":1,"max_artifact_bytes":4_194_304,
      "max_report_count":1,"max_report_bytes":1_048_576
    });
    let permissions: FactoryPermissionSet = serde_json::from_value(permissions).unwrap();
    selection.execution =
      Some(FlowExecutionPolicy::new(execution.budget(), permissions.clone(), execution.max_active_nodes()).unwrap());
    selection.commands.get_mut(&key("tests_on_base")).unwrap().permissions = permissions;
    let result = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone())
          .with_issue_properties(properties(fixture.project).await),
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
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let draft = result.flow.unwrap();
    let command = draft
      .root
      .nodes
      .iter()
      .find(|node| node.key() == &key("tests_on_base"))
      .unwrap();
    assert_eq!(command.budget().max_elapsed_millis(), 180_000);
    assert_eq!(
      command.budget().max_output_bytes(),
      5_242_880,
      "ordinary Build admission reserves artifact and report ceilings together"
    );
    assert_eq!(command.budget().max_attempts(), 1);
    assert_eq!(command.budget().max_tokens(), 0);
  });
}

#[test]
fn issue_delivery_uses_selected_step_permissions_with_a_reusable_profile() {
  run_ready(async {
    use octacity_server_store::FactoryRoleProfileStore;
    let fixture = delivery_fixture().await;
    let mut selection = issue_selection(&fixture).await;
    selection.issue_snapshot = Some(issue_input());
    let selected = selection.profiles.get_mut(&key("protected_tests")).unwrap();
    let old = fixture
      .store
      .factory_role_profile(fixture.project, selected.profile.id, None)
      .await
      .unwrap();
    let mut intent = role_profile_contract_intent(fixture.project);
    intent.profiles.truncate(1);
    let mut definition = old.definition().clone();
    definition.execution_binding = FactoryRoleExecutionBinding::Step;
    let mode_permissions = definition.permissions.clone();
    let mut ceiling = serde_json::to_value(&definition.permissions).unwrap();
    ceiling["network_hosts"] = serde_json::json!(["other-mode.example"]);
    definition.permissions = serde_json::from_value(ceiling).unwrap();
    intent.profiles[0].definition = definition.clone();
    let published = AuthorizedCommandHandler::new(
      fixture.policy.clone(),
      Arc::new(FactoryRoleProfileHandlers::with_policy(
        fixture.store.clone(),
        fixture.policy.clone(),
      )),
    )
    .handle_command(
      &fixture.context,
      PublishFactoryRoleProfilesCommand {
        intent,
        published_at: Timestamp::from_unix_millis(2).unwrap(),
        idempotency_key: "reusable-delivery-profile".parse().unwrap(),
      },
    )
    .await
    .unwrap();
    selected.role = published.role.unwrap().reference().clone();
    selected.profile = published.profiles[0].reference().clone();
    selected.build.as_mut().unwrap().execution_permissions = Some(Box::new(mode_permissions.clone()));
    let mut choices = FactoryConfigurationChoices::try_new(fixture.installed.entries()).unwrap();
    for contract in fixture.installed.build_pipeline_contracts() {
      let mut contract = contract.clone();
      if let Some(compiler) = &mut contract.compilation {
        compiler.permissions = Some(mode_permissions.clone());
      }
      choices = choices.with_build_pipeline_contract(contract).unwrap();
    }
    for target in fixture.installed.candidate_publications() {
      choices = choices.with_candidate_publication(target.clone()).unwrap();
    }
    for (alias, capability) in fixture.installed.role_profile_capabilities() {
      choices = choices
        .with_role_profile_capability(alias.clone(), capability.clone())
        .unwrap();
    }
    let mut capability = fixture
      .installed
      .role_profile_capabilities()
      .values()
      .next()
      .unwrap()
      .clone();
    capability.local_permissions = LocalPermissionCeiling::fully_enforced(definition.permissions.clone());
    capability.ceiling = definition;
    choices = choices
      .with_reference_choice(FactoryReferenceChoice {
        alias: key("reusable-delivery"),
        kind: FactoryChoiceKind::NodeCapability,
        reference: capability.ceiling.toolchain.executable.clone(),
      })
      .unwrap()
      .with_role_profile_capability(key("reusable-delivery"), capability)
      .unwrap();
    fixture.store.enable_project(fixture.project, choices).unwrap();
    let result = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone())
          .with_issue_properties(properties(fixture.project).await),
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
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let draft = result
      .flow
      .expect("a qualified execution mode must open an editable draft");
    let node = draft
      .root
      .nodes
      .iter()
      .find(|node| node.key() == &key("protected_tests"))
      .unwrap();
    assert_eq!(
      node.permissions(),
      &mode_permissions,
      "other execution modes grant no step authority"
    );
  });
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
fn issue_delivery_reports_the_configured_project_property_that_is_unavailable() {
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
    definition.classification = Default::default();
    definition
      .properties
      .iter_mut()
      .find(|property| property.id == "repro")
      .unwrap()
      .retired = true;
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

#[test]
fn delivery_template_uses_selected_direct_fields_and_their_choice_order_for_conditions() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let store = properties(fixture.project).await;
    let mut definition = store.issue_properties(fixture.project, None).await.unwrap().definition;
    definition.classification = Default::default();
    for size in &mut definition.sizes {
      size.retired = true;
    }
    definition
      .sizes
      .push(serde_json::from_value(serde_json::json!({"id":"size_xl","name":"Extra large","retired":false})).unwrap());
    for (id, choices) in [
      ("effort_band", vec!["small", "medium", "large"]),
      ("risk_band", vec!["low", "high"]),
    ] {
      definition.properties.push(serde_json::from_value(serde_json::json!({"id":id,"name":id,"retired":false,"kind":{"type":"choice","choices":choices.into_iter().map(|id|serde_json::json!({"id":id,"name":id,"retired":false})).collect::<Vec<_>>()}})).unwrap());
    }
    store
      .publish_issue_properties(octacity_server_store::testing::management_mutation(
        octacity_server_store::PublishIssueProperties {
          intent: octacity_server_store::PublishIssuePropertiesIntent {
            project_id: fixture.project,
            expected_version: 1,
            definition,
          },
          idempotency_key: "chosen-consumer-fields".parse().unwrap(),
          published_at: Timestamp::from_unix_millis(2).unwrap(),
        },
      ))
      .await
      .unwrap();
    let input:FactoryPresetIssueSnapshotSelection=serde_json::from_value(serde_json::json!({
      "definition_version":2,"admission":issue_input().admission,
      "fields":{"size":{"source":{"kind":"property","id":"effort_band"},"required":true},"risk":{"source":{"kind":"property","id":"risk_band"},"required":true}},
      "choice_thresholds":{"requirements_needed":"medium"}
    })).expect("ordinary consumer field selections are part of the request contract");
    let mut selection = issue_selection(&fixture).await;
    selection.issue_snapshot = Some(input);
    let result = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
          .with_repositories(fixture.reader.clone())
          .with_issue_properties(store),
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
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let draft = result.flow.unwrap();
    let snapshot = draft.issue_snapshot.unwrap();
    assert_eq!(
      snapshot.fields[&key("size")].source,
      FlowIssueFieldSource::Property {
        id: "effort_band".into()
      }
    );
    assert_eq!(
      snapshot.fields[&key("risk")].source,
      FlowIssueFieldSource::Property { id: "risk_band".into() }
    );
    let condition = draft
      .root
      .nodes
      .iter()
      .find(|node| node.key() == &key("requirements_needed"))
      .unwrap();
    let program: FlowGateProgram = serde_json::from_value(condition.gate().unwrap().parameters().clone()).unwrap();
    let json = serde_json::to_value(program).unwrap();
    assert_eq!(
      json["rules"],
      serde_json::json!([{"when":{"operation":"any","predicates":[
      {"operation":"compare","left":{"source":"path","path":"/size"},"comparison":"equal","right":{"source":"literal","value":"medium"}},
      {"operation":"compare","left":{"source":"path","path":"/size"},"comparison":"equal","right":{"source":"literal","value":"large"}}
    ]},"outcome":"specification"}])
    );
  });
}
