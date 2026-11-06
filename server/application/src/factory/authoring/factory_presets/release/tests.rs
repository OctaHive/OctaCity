use super::*;

#[test]
fn ticket_delivery_manifest_contains_the_same_graph_as_the_resolved_example() {
  let direct = FlowDefinitionTemplate::from_manifest(TICKET_DELIVERY.manifest, |path| {
    Ok(bundled_part(path.as_str()).unwrap().to_vec())
  })
  .unwrap();
  let (released, _) = TICKET_DELIVERY.load().unwrap();
  assert!(
    serde_json::to_value(direct).unwrap() == serde_json::to_value(released).unwrap(),
    "the released manifest must describe the complete graph, including routes, queues and tasks"
  );
}

#[test]
fn ticket_delivery_release_has_one_route_from_each_successful_stage() {
  let (template, _) = TICKET_DELIVERY.load().unwrap();
  for (stage, outcome, next) in [
    ("reproduction-needed", "bypass", "requirements_needed"),
    ("reproduction-result", "reproduced", "requirements_needed"),
    ("requirements_review_policy", "automatic", "protected_tests"),
    ("requirements_approved", "accepted", "protected_tests"),
    ("protected_validation_gate", "passed", "prepare_pull_request"),
  ] {
    let routes: Vec<_> = template
      .transitions
      .iter()
      .filter(|edge| edge.predecessor().as_str() == stage && edge.outcome().as_str() == outcome)
      .map(FlowTransition::target)
      .collect();
    assert_eq!(routes, [&FlowTransitionTarget::Node(next.parse().unwrap())], "{stage}");
  }
  assert!(
    template
      .terminals
      .keys()
      .any(|key| key.as_str() == "delivered_for_review")
  );
}

#[test]
fn ticket_delivery_consumes_only_selected_fields_of_the_exact_published_triage_schema() {
  use crate::factory_node_test_support::fixtures::key;
  use serde_json::json;
  let (mut template, _) = TICKET_DELIVERY.load().unwrap();
  let (triage, _) = TRIAGE.load().unwrap();
  let triage = triage.data_schemas().unwrap().remove(&key("summary")).unwrap();
  let incompatible = FlowDataSchema::new(
    key("text-only"),
    key("v1"),
    FlowValueSchema::String {
      min_bytes: 0,
      max_bytes: 4096,
    },
  )
  .unwrap();
  assert!(super::super::delivery::bind_predecessor(&mut template, &incompatible).is_err());
  super::super::delivery::bind_predecessor(&mut template, &triage).unwrap();
  let entry = template.nodes.iter().find(|node| node.key == template.entry).unwrap();
  let requirements = template
    .nodes
    .iter()
    .find(|node| node.key == key("requirements_needed"))
    .unwrap();
  let shape = template.data_schemas().unwrap();
  let projected = |node: &FlowTemplateNode| {
    let encoded = serde_json::to_value(&node.input_binding).unwrap();
    let mapping: FlowDataMapping = serde_json::from_value(encoded["mapping"].clone()).unwrap();
    let triage = json!({"accepted":true,"admission_risk":"low","assessment":{
      "work_kind":"defect","reproducibility":"unknown","size":"small","risk":"low","component":"server",
      "recommended_route":"requirements","severity":"low","dependencies":[],"priority_assessment":30,
      "private":"must not propagate"
    }});
    assert!(
      node
        .input_binding
        .incoming_schemas()
        .all(|schema| schema == shape.get(&key("predecessor")).unwrap().reference())
    );
    mapping
      .project_value(&json!({"triage":triage}), &shape[&node.input_schema])
      .unwrap()
  };
  assert_eq!(
    projected(entry).value(),
    &json!({"work_kind":"defect","reproducibility":"unknown","triage_accepted":true})
  );
  assert_eq!(
    projected(requirements).value(),
    &json!({"summary":{
    "work_kind":"defect","size":"small","risk":"low","admission_risk":"low","recommended_route":"requirements",
    "component":"server","severity":"low","dependencies":[],"research_required":false,"research_accepted":false
  },"priority_assessment":30})
  );
  assert_eq!(shape[&key("predecessor")], triage);
}

#[test]
fn ticket_delivery_publishes_exact_installed_commands_and_review_target_as_ordinary_nodes() {
  use crate::factory_node_test_support::{fixtures, probe_observation_contract};
  let (mut template, _) = TICKET_DELIVERY.load().unwrap();
  let project = octacity_server_domain::ProjectId::generate();
  let mut profiles = std::collections::BTreeMap::new();
  let mut commands = std::collections::BTreeMap::new();
  let schemas = template.data_schemas().unwrap();
  for node in &template.nodes {
    let Some(alias) = &node.build_profile else { continue };
    let mut build = fixtures::binding(project);
    build.result_outcome = node.outcomes[0].key.clone();
    if node.kind == FlowNodeKind::BuildCommand {
      let result = schemas
        .get(&node.outcomes[0].schema)
        .cloned()
        .unwrap_or_else(|| probe_observation_contract().report_schema().unwrap());
      commands.insert(
        alias.clone(),
        super::super::FactoryPresetCommandSelection {
          build,
          result,
          permissions: node.permissions.clone(),
        },
      );
    } else {
      profiles.insert(alias.clone(), build);
    }
  }
  let publication = super::super::FactoryPresetPublicationSelection {
    plugin: fixtures::reference("installed-publisher"),
    target: fixtures::reference("installed-review-target"),
    permissions: FactoryPermissionSet::try_new(FactoryPermissionDraft {
      plugins: vec![fixtures::reference("installed-publisher")],
      ..Default::default()
    })
    .unwrap(),
  };
  let receipt = schemas[&fixtures::key("pull_request_receipt")].clone();
  let choice = FactoryCandidatePublicationChoice {
    plugin: publication.plugin.clone(),
    target: publication.target.clone(),
    repository_id: octacity_server_domain::RepositoryId::generate(),
    outcome: FlowOutcomeDefinition::new(
      fixtures::key("published"),
      FlowOutcomeKind::Success,
      receipt.reference().clone(),
    ),
    receipt_schema: Some(receipt),
    review_url: true,
    permissions: Default::default(),
  };
  let command_bindings =
    super::super::delivery::bind_installed_steps(&mut template, &commands, &publication, &choice).unwrap();
  let implementation = template
    .nodes
    .iter_mut()
    .find(|node| node.key.as_str() == "implementation")
    .unwrap();
  let build = profiles.remove(&implementation.build_profile.take().unwrap()).unwrap();
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    FactoryRoleDefinition {
      name: FactoryText::new("Coder").unwrap(),
      instructions: "Implement the accepted requirements.".into(),
      template: None,
      skills: Default::default(),
      icon: None,
      avatar: None,
      description: String::new(),
      concurrency_limit: None,
    },
  )
  .unwrap();
  let mut profile = octacity_server_store::testing::role_profile_contract_definition(project);
  profile.toolchain = FactoryTaskToolchain {
    executable: build.tool.clone(),
    plugin: build.plugin.clone(),
    model: build.model_or_tool.clone(),
  };
  profile.build_configuration = build.build_configuration.clone();
  profile.budget = implementation.budget;
  profile.permissions = implementation.permissions.clone();
  profile.execution = Default::default();
  profile.context_requirements.clear();
  let profile = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, profile).unwrap();
  let candidate = fixtures::key("selected");
  implementation.profile_selection = Some(FlowProfileSelectionBinding {
    role: role.reference().clone(),
    candidates: [(candidate.clone(), FlowProfileCandidate { profile, build })].into(),
    policy: FlowProfileSelectionPolicy::Manual {
      candidate: Some(candidate.clone()),
    },
    failure: FlowProfileSelectionFailure::ManualSelection,
    selection_budget: BudgetUsage::default(),
  });
  template.execution = FlowExecutionPolicy::new(
    template.execution.budget(),
    publication.permissions.clone(),
    template.execution.max_active_nodes(),
  )
  .unwrap();
  profiles.extend(command_bindings);
  let published = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  let limits = FlowAdmissionLimits::product_defaults(
    template.execution.max_active_nodes(),
    template.execution.budget(),
    publication.permissions.clone(),
  )
  .unwrap();
  let published = FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(published.definition.reference(), vec![published.definition]).unwrap(),
    limits,
    published.schemas,
    Default::default(),
  )
  .unwrap();
  let published = published.closure.definition(published.closure.root()).unwrap();
  let implementation = published.node(&fixtures::key("implementation")).unwrap();
  let build = &implementation.profile_selection().unwrap().candidates[&candidate].build;
  assert!(build.capture_deliverables.is_some());
  for (node, alias) in [
    ("reproduction", "reproduction"),
    ("tests_on_base", "tests_on_base"),
    ("protected_validation", "protected.validation"),
  ] {
    let node = published.node(&fixtures::key(node)).unwrap();
    let command = &commands[&fixtures::key(alias)];
    assert_eq!(
      node.build().unwrap().build_configuration,
      command.build.build_configuration
    );
    assert_eq!(node.outcomes()[0].schema(), command.result.reference());
  }
  let action = published
    .node(&fixtures::key("publish_pull_request"))
    .unwrap()
    .action()
    .unwrap();
  assert_eq!(action.plugin(), &publication.plugin);
  assert_eq!(
    action.parameters()["target"],
    serde_json::to_value(publication.target).unwrap()
  );
}
