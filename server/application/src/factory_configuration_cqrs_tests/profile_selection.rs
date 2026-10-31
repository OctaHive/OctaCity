use super::*;
use octacity_server_factory::*;
use std::collections::BTreeMap;

fn selected_fixture(project: ProjectId) -> Fixture {
  selected_fixture_with_profiles(project, |_, _| {})
}
fn selected_fixture_with_profiles(
  project: ProjectId,
  mut edit: impl FnMut(&str, &mut FactoryRoleProfileDefinition),
) -> Fixture {
  let mut fixture = fixture(project);
  let prior = fixture.draft.flow.as_ref().unwrap();
  let root = prior.closure.definition(prior.closure.root()).unwrap();
  let original = root.node(root.entry()).unwrap();
  let base_build = FlowBuildBinding {
    tool: original.action().unwrap().plugin().clone(),
    result_tool: None,
    plugin: original.action().unwrap().plugin().clone(),
    model_or_tool: original.action().unwrap().plugin().clone(),
    task_digest: digest(9),
    build_configuration: fixture.choices.entries().build_configurations[0].1.clone(),
    capture_write_paths: None,
    capture_deliverables: None,
    result_output: key("observations"),
    result_output_kind: EvidenceOutputKind::Report,
    result_outcome: key("succeeded"),
    evidence: vec![],
  };
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    FactoryRoleDefinition {
      avatar: None,
      skills: Default::default(),
      name: FactoryText::new("Stage owner").unwrap(),
      instructions: "Private stage instructions".into(),
      template: None,
      icon: None,
      description: String::new(),
      concurrency_limit: None,
    },
  )
  .unwrap();
  let verifier = exact("selection.verifier", "v1", 72);
  let mut entries = fixture.choices.entries();
  entries.references.push(FactoryReferenceChoice {
    kind: FactoryChoiceKind::NodeCapability,
    alias: key("selection.verifier"),
    reference: verifier.clone(),
  });
  let mut candidates = BTreeMap::new();
  let mut caps = Vec::new();
  for (name, byte) in [("default", 73), ("pro", 74), ("specialized", 75)] {
    let mut build = base_build.clone();
    build.result_tool = Some(verifier.clone());
    build.model_or_tool = exact(name, "v1", byte);
    let mut definition = FactoryRoleProfileDefinition {
      model_identifier: None,
      thinking_parameter: None,
      name: FactoryText::new(name).unwrap(),
      instructions: "Private profile instructions".into(),
      template: None,
      toolchain: FactoryTaskToolchain {
        plugin: build.plugin.clone(),
        executable: build.tool.clone(),
        model: build.model_or_tool.clone(),
      },
      execution: Default::default(),
      model_parameters: BTreeMap::new(),
      skills: BTreeMap::new(),
      context_requirements: Default::default(),
      build_configuration: build.build_configuration.clone(),
      credential_purpose: FactoryCredentialPurpose::Model,
      permissions: FactoryPermissionSet::deny_all(),
      budget: original.budget(),
    };
    edit(name, &mut definition);
    entries.references.extend([
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key(name),
        reference: build.tool.clone(),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key(&format!("model.{name}")),
        reference: build.model_or_tool.clone(),
      },
    ]);
    caps.push((
      key(name),
      FactoryRoleProfileCapability {
        mcp_required_environment: Default::default(),
        model_catalog: None,
        ceiling: definition.clone(),
        parameters: FlowDataSchema::new(
          key("parameters"),
          key("v1"),
          FlowValueSchema::Object {
            fields: BTreeMap::new(),
          },
        )
        .unwrap(),
        public_environment: BTreeMap::new(),
        local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
      },
    ));
    candidates.insert(
      key(name),
      FlowProfileCandidate {
        profile: FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).unwrap(),
        build,
      },
    );
  }
  let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
  for (alias, cap) in caps {
    choices = choices.with_role_profile_capability(alias, cap).unwrap();
  }
  let mut wire = serde_json::to_value(original).unwrap();
  wire.as_object_mut().unwrap().remove("action");
  wire["kind"] = serde_json::json!("reasoning");
  let selected = serde_json::from_value::<FlowNodeDefinition>(wire)
    .unwrap()
    .with_profile_selection(FlowProfileSelectionBinding {
      role: role.reference().clone(),
      candidates,
      policy: FlowProfileSelectionPolicy::Manual { candidate: None },
      failure: FlowProfileSelectionFailure::ManualSelection,
      selection_budget: BudgetUsage::default(),
    })
    .unwrap();
  let nodes = root
    .nodes()
    .iter()
    .map(|node| {
      if node.key() == selected.key() {
        selected.clone()
      } else {
        node.clone()
      }
    })
    .collect();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes,
    transitions: root.transitions().to_vec(),
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  fixture.draft.flow = Some(
    FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      prior.limits.clone(),
      prior.data_schemas.clone(),
      prior.pools.clone(),
    )
    .unwrap(),
  );
  fixture.choices = choices;
  fixture
}

#[test]
fn selection_validation_and_diagnostics_include_every_candidate_build_contract() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices.clone()).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store.clone());
    let query_input = ValidateFactoryConfigurationQuery {
      id: FactoryConfigurationId::generate(),
      project_id: project,
      definition_digest: digest(20),
      draft: fixture.draft,
    };
    let validation = query(&handlers, query_input.clone()).await.unwrap();
    let models: Vec<_> = validation
      .diagnostics
      .required_agent_capabilities
      .iter()
      .map(|reference| reference.identity().as_str())
      .collect();
    assert!(
      models.contains(&"default") && models.contains(&"pro") && models.contains(&"specialized"),
      "candidate models absent from diagnostics: {models:?}"
    );
    let mut deleted_choices = fixture.choices.clone();
    let role = query_input
      .draft
      .flow
      .as_ref()
      .unwrap()
      .closure
      .definitions()
      .iter()
      .flat_map(|definition| definition.nodes())
      .find_map(|node| node.profile_selection())
      .unwrap()
      .role
      .id;
    deleted_choices.exclude_role(role);
    store.enable_project(project, deleted_choices).unwrap();
    assert!(
      query(&handlers, query_input.clone()).await.is_err(),
      "authorization filtering must preserve deleted-role exclusions"
    );
    let mut entries = fixture.choices.entries();
    entries
      .references
      .retain(|entry| entry.alias != key("selection.verifier"));
    let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
    for (alias, cap) in fixture.choices.role_profile_capabilities() {
      choices = choices
        .with_role_profile_capability(alias.clone(), cap.clone())
        .unwrap();
    }
    store.enable_project(project, choices).unwrap();
    let error = query(&handlers, query_input)
      .await
      .expect_err("candidate verifiers must be installed and visible");
    assert_eq!(error.classification(), ApplicationFailure::Invalid);
  });
}

#[test]
fn constraints_explain_each_exact_profile_allowance_instead_of_claiming_the_node_reservation() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture_with_profiles(project, |name, definition| {
      if name == "default" {
        definition.budget = BudgetLimit::new(1, 10_000, 17, 10_000, 10_000).unwrap();
      }
    });
    let flow = fixture.draft.flow.as_ref().unwrap();
    let selected_node = flow
      .closure
      .definitions()
      .iter()
      .flat_map(|d| d.nodes())
      .find(|n| n.profile_selection().is_some())
      .unwrap();
    let profile = selected_node.profile_selection().unwrap().candidates[&key("default")]
      .profile
      .reference()
      .clone();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let validation = query(
      &FactoryConfigurationHandlers::new(store),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(20),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap();
    let diagnostics = serde_json::to_value(validation.diagnostics).unwrap();
    let budgets = diagnostics["constraints"]["node_budgets"].as_array().unwrap();
    let selected = budgets
      .iter()
      .find(|budget| budget["profile"] == serde_json::json!(profile))
      .expect("each exact candidate must retain its own effective allowance");
    assert_eq!(selected["requested"]["max_tokens"], 1000);
    assert_eq!(selected["effective"]["max_tokens"], 17);
    assert_eq!(selected["sources"][0]["source"], "role_profile");
    assert_eq!(selected["sources"][0]["scope"], "node_execution");
  });
}

#[test]
fn standalone_flow_publication_uses_the_same_profile_compatibility_validation() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let flow = fixture.draft.flow.unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let draft = crate::FactoryFlowDefinitionDraft {
      root: crate::FactoryFlowRootDraft {
        code_sources: None,
        process: root.process().cloned(),
        input_schema: root.input_schema().cloned(),
        entry: root.entry().clone(),
        nodes: root.nodes().to_vec(),
        transitions: root.transitions().to_vec(),
        terminals: root.terminals().to_vec(),
        context_projections: root.context_projections().to_vec(),
        data_projections: root.data_projections().to_vec(),
        execution: root.execution().clone(),
      },
      limits: flow.limits.clone(),
      data_schemas: flow.data_schemas.clone(),
      pools: flow.pools.clone(),
    };
    let mut choices = FactoryConfigurationChoices::try_new(fixture.choices.entries()).unwrap();
    for (alias, cap) in fixture.choices.role_profile_capabilities() {
      if alias != &key("specialized") {
        choices = choices
          .with_role_profile_capability(alias.clone(), cap.clone())
          .unwrap();
      }
    }
    let store = Arc::new(PublicationChoices(std::sync::Mutex::new(choices)));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryFlowDefinitionHandlers::new(store.clone(), policy)),
    );
    let input = crate::ValidateFactoryFlowDefinitionQuery {
      id: FlowDefinitionId::generate(),
      project_id: project,
      draft,
    };
    assert!(
      handlers.handle_query(&context(), input.clone()).await.is_err(),
      "standalone Flow publication must reject an unavailable candidate too"
    );
    *store.0.lock().unwrap() = fixture.choices;
    assert!(handlers.handle_query(&context(), input).await.is_ok());
  });
}

// Root validation may read installed choices, but cannot read a subflow or write a publication.
// Fail immediately if the public handler crosses either boundary in these contracts.
pub(super) struct PublicationChoices(pub(super) std::sync::Mutex<FactoryConfigurationChoices>);

#[test]
fn access_constraints_explain_profile_narrowing_and_defer_actual_agent_enforcement() {
  run_ready(async {
    let project = ProjectId::generate();
    let mut fixture = selected_fixture_with_profiles(project, |name, definition| {
      if name == "default" {
        definition.budget = BudgetLimit::new(1, 1000, 1000, 10_000, 10_000).unwrap();
      }
    });
    let old = fixture.draft.flow.take().unwrap();
    let root = old.closure.definition(old.closure.root()).unwrap();
    let selected = root
      .nodes()
      .iter()
      .find(|node| node.profile_selection().is_some())
      .unwrap();
    let profile = selected.profile_selection().unwrap().candidates[&key("default")]
      .profile
      .reference()
      .clone();
    let selected_key = selected.key().clone();
    let mut permissions = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
    permissions["network_hosts"] = serde_json::json!(["api.allowed.example"]);
    permissions["resources"]["elapsed_millis"] = serde_json::json!(10_000);
    let mut root_wire = serde_json::to_value(root).unwrap();
    root_wire.as_object_mut().unwrap().remove("reference");
    root_wire["execution"]["permissions"] = permissions.clone();
    for node in root_wire["nodes"].as_array_mut().unwrap() {
      node["permissions"] = permissions.clone();
    }
    let root_draft: crate::FactoryFlowRootDraft = serde_json::from_value(root_wire).unwrap();
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: root.reference().id(),
      version: root.reference().version(),
      input_schema: root_draft.input_schema,
      entry: root_draft.entry,
      nodes: root_draft.nodes,
      transitions: root_draft.transitions,
      terminals: root_draft.terminals,
      context_projections: root_draft.context_projections,
      data_projections: root_draft.data_projections,
      execution: root_draft.execution,
    })
    .unwrap();
    let mut limits = serde_json::to_value(old.limits).unwrap();
    limits["permissions"] = permissions;
    fixture.draft.flow = Some(
      FactoryFlowConfiguration::new(
        PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
        serde_json::from_value(limits).unwrap(),
        old.data_schemas,
        old.pools,
      )
      .unwrap(),
    );
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let validation = query(
      &FactoryConfigurationHandlers::new(store),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(20),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap();
    let diagnostics = serde_json::to_value(validation.diagnostics).unwrap();
    let node = diagnostics["effective_permissions"]
      .as_array()
      .unwrap()
      .iter()
      .find(|row| row["node"] == serde_json::json!(selected_key))
      .unwrap();
    assert_eq!(
      node["requested"]["network_hosts"],
      serde_json::json!(["api.allowed.example"])
    );
    assert!(
      node["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "flow_admission")
    );
    let selected = node["profiles"]
      .as_array()
      .expect("profile ceilings remain explicit")
      .iter()
      .find(|row| row["profile"] == serde_json::json!(profile))
      .unwrap();
    assert_eq!(selected["permissions"]["network_hosts"], serde_json::json!([]));
    assert_eq!(selected["permissions"]["resources"]["elapsed_millis"], 0);
    assert!(
      selected["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "profile_budget")
    );
    assert!(
      node["unresolved_sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "agent_local")
    );
    assert!(
      node["unresolved_sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "backend_placement")
    );
    assert!(!diagnostics.to_string().contains("Private profile instructions"));
    for source in ["project_policy", "installed_runtime_policy"] {
      assert!(
        node["unresolved_sources"]
          .as_array()
          .unwrap()
          .iter()
          .any(|value| value == source),
        "dispatch policy {source} cannot be claimed resolved by definition validation"
      );
    }
  });
}
use octacity_server_store::*;
#[async_trait]
impl FactoryConfigurationStore for PublicationChoices {
  async fn factory_configuration_availability(
    &self,
    _: ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    Ok(FactoryConfigurationAvailability::Available(
      self.0.lock().unwrap().clone(),
    ))
  }
  async fn replay_factory_configuration_mutation(
    &self,
    _: ManagementMutation<ReplayFactoryConfigurationMutation>,
  ) -> Result<Option<FactoryConfigurationMutationOutcome>, StoreError> {
    unreachable!("validation cannot replay a mutation")
  }
  async fn create_factory_configuration(
    &self,
    _: ManagementMutation<CreateFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, StoreError> {
    unreachable!("validation cannot create a configuration")
  }
  async fn replace_factory_configuration(
    &self,
    _: ManagementMutation<ReplaceFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, ReplaceFactoryConfigurationError> {
    unreachable!("validation cannot replace a configuration")
  }
  async fn factory_configuration_version(
    &self,
    _: FactoryConfigurationId,
    _: FactoryConfigurationVersion,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    unreachable!("root validation needs no configuration read")
  }
  async fn current_factory_configuration(
    &self,
    _: FactoryConfigurationId,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    unreachable!("root validation needs no configuration read")
  }
}
#[async_trait]
impl FactoryFlowDefinitionStore for PublicationChoices {
  async fn replay_factory_flow_definition(
    &self,
    _: ManagementMutation<ReplayFactoryFlowDefinition>,
  ) -> Result<Option<FactoryFlowDefinitionMutationOutcome>, StoreError> {
    unreachable!("validation cannot replay a mutation")
  }
  async fn publish_factory_flow_definition(
    &self,
    _: ManagementMutation<PublishFactoryFlowDefinition>,
  ) -> Result<FactoryFlowDefinitionMutationOutcome, ReplaceFactoryConfigurationError> {
    unreachable!("validation cannot publish")
  }
  async fn factory_flow_definition(
    &self,
    _: FlowDefinitionId,
    _: Option<FlowDefinitionVersion>,
  ) -> Result<PublishedFactoryFlowDefinition, StoreError> {
    unreachable!("a standalone root has no subflow")
  }
  async fn list_factory_flow_definitions(
    &self,
    _: ProjectId,
    _: Option<FlowDefinitionPagePosition>,
    _: u16,
    _: FlowDefinitionListVisibility,
  ) -> Result<FactoryFlowDefinitionPage, StoreError> {
    unreachable!("validation cannot list definitions")
  }
}

#[test]
fn decision_profile_answers_are_checked_before_flow_publication() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let flow = fixture.draft.flow.unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let selector = exact("installed-selector", "v1", 91);
    let mut entries = fixture.choices.entries();
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::DecisionSignalPolicy,
      alias: key("selector"),
      reference: selector.clone(),
    });
    let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
    for (alias, capability) in fixture.choices.role_profile_capabilities() {
      choices = choices
        .with_role_profile_capability(alias.clone(), capability.clone())
        .unwrap();
    }
    choices = choices
      .with_choice_metadata(
        FactoryChoiceKind::DecisionSignalPolicy,
        key("selector"),
        FactoryChoiceMetadata {
          profile_selection: Some(FactoryProfileSelectorMetadata {
            provider: exact("provider", "v1", 92),
            adapter: exact("adapter", "v1", 93),
            model: exact("model", "v1", 94),
            question_set: exact("criteria", "v1", 95),
            calibration: exact("calibration", "v1", 96),
            probability_semantics: exact("semantics", "v1", 97),
            mode: DecisionSignalMode::Shadow,
            baseline: key("default"),
            candidates: DecisionSignalChoices::try_new(vec![key("default"), key("pro"), key("specialized")]).unwrap(),
            minimum_probability: DecisionSignalProbability::new(800_000).unwrap(),
            minimum_margin: DecisionSignalProbability::new(100_000).unwrap(),
          }),
          ..Default::default()
        },
      )
      .unwrap();
    let store = Arc::new(PublicationChoices(std::sync::Mutex::new(choices)));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryFlowDefinitionHandlers::new(store, policy)),
    );
    let draft = crate::FactoryFlowDefinitionDraft {
      root: crate::FactoryFlowRootDraft {
        code_sources: None,
        process: root.process().cloned(),
        input_schema: root.input_schema().cloned(),
        entry: root.entry().clone(),
        nodes: root.nodes().to_vec(),
        transitions: root.transitions().to_vec(),
        terminals: root.terminals().to_vec(),
        context_projections: root.context_projections().to_vec(),
        data_projections: root.data_projections().to_vec(),
        execution: root.execution().clone(),
      },
      limits: flow.limits.clone(),
      data_schemas: flow.data_schemas.clone(),
      pools: flow.pools.clone(),
    };
    let mut input = crate::ValidateFactoryFlowDefinitionQuery {
      id: FlowDefinitionId::generate(),
      project_id: project,
      draft,
    };
    let at = input
      .draft
      .root
      .nodes
      .iter()
      .position(|node| node.profile_selection().is_some())
      .unwrap();
    let node = input.draft.root.nodes[at].clone();
    let mut selection = node.profile_selection().unwrap().clone();
    selection.policy = FlowProfileSelectionPolicy::DecisionApi {
      policy: selector,
      input: FlowDataMapping::new(FlowValueMapping::Object {
        fields: BTreeMap::new(),
      })
      .unwrap(),
    };
    input.draft.root.nodes[at] = node.clone().with_profile_selection(selection.clone()).unwrap();
    assert!(handlers.handle_query(&context(), input.clone()).await.is_ok());
    let candidate = selection.candidates.remove(&key("default")).unwrap();
    selection
      .candidates
      .insert(key("62000000-0000-4000-8000-000000000001"), candidate);
    input.draft.root.nodes[at] = node.clone().with_profile_selection(selection.clone()).unwrap();
    assert!(
      handlers.handle_query(&context(), input.clone()).await.is_err(),
      "a profile UUID is not the installed selector answer default"
    );
    selection.policy = FlowProfileSelectionPolicy::Manual { candidate: None };
    input.draft.root.nodes[at] = node.with_profile_selection(selection).unwrap();
    assert!(
      handlers.handle_query(&context(), input).await.is_ok(),
      "manual selection keeps arbitrary candidate keys and needs no provider"
    );
  });
}
