//! Template resolution uses the same selected-field form as ordinary agent publication.
use super::*;
use octacity_server_store::*;

#[test]
fn triage_template_resolves_required_task_profiles_without_project_documents_or_an_apply_node() {
  assert_selected_form(FactoryRoleExecutionBinding::Profile);
}

#[test]
fn selected_field_template_keeps_local_save_authority_outside_a_step_bound_agent_profile() {
  assert_selected_form(FactoryRoleExecutionBinding::Step);
}

fn assert_selected_form(execution_binding: FactoryRoleExecutionBinding) {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let properties = issue_delivery::properties(fixture.project).await;
    let mut definition = properties
      .issue_properties(fixture.project, None)
      .await
      .unwrap()
      .definition;
    definition.classification = Default::default();
    properties
      .publish_issue_properties(octacity_server_store::testing::management_mutation(
        PublishIssueProperties {
          intent: PublishIssuePropertiesIntent {
            project_id: fixture.project,
            expected_version: 1,
            definition,
          },
          idempotency_key: "no-business-roles".parse().unwrap(),
          published_at: Timestamp::from_unix_millis(2).unwrap(),
        },
      ))
      .await
      .unwrap();
    let writer = ImmutableReference::new(key("installed.fields"), key("v1"), FactoryDigest::from_bytes([62; 32]));
    let policy_key = key("ordinary-fields");
    let mut installed = FactoryConfigurationChoices::try_new(fixture.installed.entries()).unwrap();
    for contract in fixture.installed.build_pipeline_contracts() {
      let mut contract = contract.clone();
      if execution_binding == FactoryRoleExecutionBinding::Step {
        let selected = fixture.selection.profiles.values().next().unwrap();
        let profile = fixture
          .store
          .factory_role_profile(fixture.project, selected.profile.id, Some(selected.profile.version))
          .await
          .unwrap();
        let permissions = profile.definition().permissions.clone();
        if let Some(compiler) = &mut contract.compilation {
          compiler.permissions = Some(permissions.clone());
        }
      }
      installed = installed.with_build_pipeline_contract(contract).unwrap();
    }
    for (alias, capability) in fixture.installed.role_profile_capabilities() {
      let mut capability = capability.clone();
      capability.ceiling.execution_binding = execution_binding;
      capability.ceiling.context_requirements = [key("task")].into();
      installed = installed
        .with_role_profile_capability(alias.clone(), capability)
        .unwrap();
    }
    let choices = installed
      .with_reference_choice(FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key("fields"),
        reference: writer.clone(),
      })
      .unwrap()
      .with_choice_metadata(
        FactoryChoiceKind::NodeCapability,
        key("fields"),
        serde_json::from_value(serde_json::json!({
          "node_kinds":["trusted_action"],"signal_purposes":[],"enforcement":[],"compatible_choices":[],
          "issue_classification":{"policy":policy_key,"size_labels":{}}
        }))
        .unwrap(),
      )
      .unwrap();
    fixture.store.enable_project(fixture.project, choices).unwrap();
    let (_, preset) = release::TRIAGE.load().unwrap();
    let mut selection = fixture.selection.clone();
    selection.preset = preset;
    selection.flow_only = true;
    selection.configuration_id = None;
    selection.configuration = None;
    selection.repository = None;
    selection.accepted_predecessor = None;
    selection.publication = None;
    selection.commands.clear();
    let mut profile = selection.profiles.values().next().unwrap().clone();
    let existing = fixture
      .store
      .factory_role_profile(fixture.project, profile.profile.id, Some(profile.profile.version))
      .await
      .unwrap();
    let mut definition = existing.definition().clone();
    definition.execution_binding = execution_binding;
    definition.context_requirements = [key("task")].into();
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
        intent: FactoryRoleProfilesIntent {
          project_id: fixture.project,
          role: None,
          profiles: vec![FactoryRoleProfileIntent {
            id: FactoryRoleProfileId::generate(),
            expected_current_version: None,
            role: profile.role.clone(),
            definition,
          }],
        },
        idempotency_key: "form-profile".parse().unwrap(),
        published_at: Timestamp::from_unix_millis(2).unwrap(),
      },
    )
    .await
    .unwrap();
    profile.profile = published.profiles[0].reference().clone();
    if execution_binding == FactoryRoleExecutionBinding::Step {
      profile.build.as_mut().unwrap().execution_permissions =
        Some(Box::new(published.profiles[0].definition().permissions.clone()));
    }
    selection.profiles = [(key("eligibility"), profile.clone()), (key("classification"), profile)].into();
    let mut permissions = serde_json::to_value(selection.execution.as_ref().unwrap().permissions()).unwrap();
    permissions["plugins"]
      .as_array_mut()
      .unwrap()
      .push(serde_json::to_value(&writer).unwrap());
    let permissions = serde_json::from_value(permissions).unwrap();
    selection.execution =
      Some(FlowExecutionPolicy::new(selection.execution.as_ref().unwrap().budget(), permissions, 1).unwrap());
    let form = IssueFieldForm {
      project_id: fixture.project,
      definition_version: 2,
      fields: vec![
        IssueFormField {
          field: IssueFieldTarget::Priority,
          required: true,
        },
        IssueFormField {
          field: IssueFieldTarget::Property { id: "impact".into() },
          required: false,
        },
      ],
    };
    selection.issue_fields = [(key("classification"), form.clone())].into();
    let before = properties.mutation_evidence_counts().await;
    let resolved = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone())
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
    assert!(resolved.diagnostics.is_empty(), "{:?}", resolved.diagnostics);
    let draft = resolved.flow.unwrap();
    assert_eq!(
      draft.root.nodes.len(),
      3,
      "the template has eligibility, its gate and one ordinary field-filling agent"
    );
    assert!(draft.root.nodes.iter().all(|node| node.action().is_none()));
    for node in draft
      .root
      .nodes
      .iter()
      .filter(|node| node.kind() == FlowNodeKind::Reasoning)
    {
      let context = node
        .context_binding()
        .expect("each example agent explicitly receives the admitted task");
      assert_eq!(context.work_task(), Some(&key("task")));
      assert_eq!(
        serde_json::to_value(context).unwrap()["entries"],
        serde_json::json!({}),
        "opening an example selects no Project documents"
      );
    }
    assert!(
      draft
        .root
        .nodes
        .iter()
        .all(|node| matches!(node.code(), Some(FlowCodeBinding::None))),
      "an editable template must explicitly attach code before granting a source"
    );
    let node = draft
      .root
      .nodes
      .iter()
      .find(|node| node.key() == &key("classification"))
      .unwrap();
    let outcome = &node.outcomes()[0];
    assert!(node.permissions().plugins().any(|plugin| plugin == &writer));
    assert!(
      node
        .profile_selection()
        .unwrap()
        .candidates
        .values()
        .all(|candidate| !candidate
          .profile
          .definition()
          .permissions
          .plugins()
          .any(|plugin| plugin == &writer)),
      "local saving is not an Agent capability"
    );
    let saved = outcome.issue_fields().unwrap();
    assert_eq!(
      (&saved.writer, &saved.policy, &saved.form),
      (&writer, &policy_key, &form)
    );
    let catalog = properties.issue_properties(fixture.project, Some(2)).await.unwrap();
    assert_eq!(
      draft
        .data_schemas
        .iter()
        .find(|schema| schema.reference() == outcome.schema())
        .unwrap()
        .shape(),
      &form.form_schema(&catalog).unwrap()
    );
    assert_eq!(
      properties.mutation_evidence_counts().await,
      before,
      "resolution writes no Issue values or definitions"
    );
    assert!(resolved.configuration.is_none() && resolved.input.is_none());
    let (_, preset) = release::TRIAGE.load().unwrap();
    let opened = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone()).with_issue_properties(properties),
      ),
    )
    .handle_query(
      &fixture.context,
      OpenFactoryPresetDraftQuery {
        project_id: fixture.project,
        flow_id: FlowDefinitionId::generate(),
        preset,
      },
    )
    .await
    .unwrap();
    let classification = opened
      .root
      .nodes
      .iter()
      .find(|node| node.key() == &key("classification"))
      .unwrap();
    let result = &classification.outcomes()[0];
    let form = &result
      .issue_fields()
      .expect("the example retains its selected Issue fields before choosing agents")
      .form;
    assert_eq!(form.fields.len(), 8);
    assert_eq!(form.definition_version, 2);
    assert_eq!(
      opened
        .data_schemas
        .iter()
        .find(|schema| schema.reference() == result.schema())
        .unwrap()
        .shape(),
      &form.form_schema(&catalog).unwrap()
    );
  });
}

#[test]
fn issue_example_opens_with_project_snapshot_fields_before_admission_or_executors_are_selected() {
  run_ready(async {
    let fixture = delivery_fixture().await;
    let properties = issue_delivery::properties(fixture.project).await;
    let mut definition = properties
      .issue_properties(fixture.project, None)
      .await
      .unwrap()
      .definition;
    definition.sizes.push(IssuePropertyChoice {
      id: "xl".into(),
      name: "XL".into(),
      retired: false,
    });
    properties
      .publish_issue_properties(octacity_server_store::testing::management_mutation(
        PublishIssueProperties {
          intent: PublishIssuePropertiesIntent {
            project_id: fixture.project,
            expected_version: 1,
            definition,
          },
          idempotency_key: "preview-size-order".parse().unwrap(),
          published_at: Timestamp::from_unix_millis(2).unwrap(),
        },
      ))
      .await
      .unwrap();
    let (_, preset) = release::RELEASES[1].load().unwrap();
    let opened = AuthorizedQueryHandler::new(
      fixture.policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(fixture.store.clone(), fixture.policy.clone()).with_issue_properties(properties),
      ),
    )
    .handle_query(
      &fixture.context,
      OpenFactoryPresetDraftQuery {
        project_id: fixture.project,
        flow_id: FlowDefinitionId::generate(),
        preset,
      },
    )
    .await
    .unwrap();
    let gate = opened
      .root
      .nodes
      .iter()
      .find(|node| node.key() == &key("requirements_needed"))
      .unwrap()
      .gate()
      .unwrap();
    assert_eq!(
      gate.parameters()["rules"][0]["when"],
      serde_json::json!({"operation":"any","predicates":[
        {"operation":"compare","comparison":"equal","left":{"source":"path","path":"/size"},"right":{"source":"literal","value":"large"}},
        {"operation":"compare","comparison":"equal","left":{"source":"path","path":"/size"},"right":{"source":"literal","value":"xl"}}
      ]})
    );
    assert!(
      opened.root.nodes.iter().all(|node| node.key() != &key("reproduction")),
      "the optional reproduction command starts disabled, as in the configured example"
    );
    let snapshot = opened
      .issue_snapshot
      .expect("the Issue-based example retains its input selection");
    assert!(
      snapshot.admission.is_none(),
      "scheduling mappings remain an explicit editor choice"
    );
    assert_eq!(snapshot.fields[&key("priority")].source, FlowIssueFieldSource::Priority);
    assert_eq!(snapshot.fields[&key("size")].source, FlowIssueFieldSource::Size);
    let shape = opened
      .data_schemas
      .iter()
      .find(|schema| schema.reference() == &snapshot.schema)
      .unwrap()
      .shape();
    assert!(
      shape.accepts_value(&serde_json::json!({
        "priority":"urgent_local","component":"compiler","work_kind":"maintenance","size":"small",
        "risk":"low","severity":"low","recommended_route":"research","reproducibility":"unknown","dependencies":[]
      })),
      "snapshot types come from the Project rather than a copied preset vocabulary"
    );
  });
}

#[test]
fn examples_open_in_the_ordinary_editor_without_execution_selections_or_publications() {
  run_ready(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let handler = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryPresetHandlers::new(store.clone(), policy)),
    );
    let before = store.mutation_evidence_counts().await;
    for release in release::RELEASES {
      let (_, preset) = release.load().unwrap();
      let draft = handler
        .handle_query(
          &context,
          OpenFactoryPresetDraftQuery {
            project_id: project,
            flow_id: FlowDefinitionId::generate(),
            preset,
          },
        )
        .await
        .unwrap();
      assert!(!draft.root.nodes.is_empty());
      assert!(draft.root.name.is_some());
      assert!(
        draft
          .root
          .nodes
          .iter()
          .filter(|node| node.kind() == FlowNodeKind::Reasoning)
          .all(|node| node.profile_selection().is_none() && node.build().is_none())
      );
      assert!(
        draft
          .root
          .nodes
          .iter()
          .filter(|node| node.kind() == FlowNodeKind::BuildCommand)
          .all(|node| node.build().is_none())
      );
      assert!(
        draft
          .root
          .nodes
          .iter()
          .filter(|node| node.kind() == FlowNodeKind::Reasoning)
          .all(|node| node.task().is_some())
      );
      assert!(!draft.data_schemas.is_empty());
      let wire = serde_json::to_value(&draft).unwrap();
      for node in draft
        .root
        .nodes
        .iter()
        .filter(|node| matches!(node.kind(), FlowNodeKind::Reasoning | FlowNodeKind::BuildCommand))
      {
        assert!(
          wire["execution_defaults"].get(node.key().as_str()).is_some(),
          "every unconfigured executor remains an explicit draft selection"
        );
      }
      if release.name == Some("Ticket delivery") {
        let wire = serde_json::to_value(&draft).unwrap();
        assert_eq!(
          wire["execution_defaults"]["implementation"]["capture_deliverables"]["policy"]["permitted_paths"],
          serde_json::json!(["docs", "scripts", "src", "tests/unit"])
        );
        assert!(
          wire["schema_slots"].as_array().is_some_and(|slots| !slots.is_empty()),
          "installed report formats remain explicit unconfigured slots"
        );
      }
    }
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
