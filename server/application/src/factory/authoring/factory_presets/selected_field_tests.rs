//! Template resolution uses the same selected-field form as ordinary agent publication.
use super::*;
use octacity_server_store::*;

#[test]
fn triage_template_resolves_a_custom_project_form_without_roles_or_an_apply_node() {
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
    let mut installed = fixture.installed.clone();
    if execution_binding == FactoryRoleExecutionBinding::Step {
      let selected = fixture.selection.profiles.values().next().unwrap();
      let profile = fixture
        .store
        .factory_role_profile(fixture.project, selected.profile.id, Some(selected.profile.version))
        .await
        .unwrap();
      let permissions = profile.definition().permissions.clone();
      installed = FactoryConfigurationChoices::try_new(fixture.installed.entries()).unwrap();
      for contract in fixture.installed.build_pipeline_contracts() {
        let mut contract = contract.clone();
        if let Some(compiler) = &mut contract.compilation {
          compiler.permissions = Some(permissions.clone());
        }
        installed = installed.with_build_pipeline_contract(contract).unwrap();
      }
      for (alias, capability) in fixture.installed.role_profile_capabilities() {
        let mut capability = capability.clone();
        capability.ceiling.execution_binding = execution_binding;
        installed = installed
          .with_role_profile_capability(alias.clone(), capability)
          .unwrap();
      }
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
  });
}
