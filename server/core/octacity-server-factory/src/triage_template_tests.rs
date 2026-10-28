//! Release-template contracts through ordinary instantiation, publication and admission.
use crate::flow_test_support::{binding, key};
use crate::*;
use std::collections::BTreeMap;

fn triage_template() -> Result<FlowDefinitionTemplate, FactoryError> {
  crate::flow_test_support::supplied_triage_template()
}

#[path = "triage_classification_tests.rs"]
mod classification;

#[test]
fn triage_preset_publishes_two_reasoning_stages_and_only_terminal_recommendations() {
  let template = triage_template().expect("the versioned triage manifest and parts must be bundled");
  let project = octacity_server_domain::ProjectId::generate();
  let profiles = BTreeMap::from([
    (key("eligibility"), binding(project)),
    (key("classification"), binding(project)),
  ]);
  let mut profiles = profiles;
  for profile in profiles.values_mut() {
    profile.result_outcome = key("observed");
  }
  let published = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  assert_eq!(published.definition.nodes().len(), 4);
  assert_eq!(
    published
      .definition
      .nodes()
      .iter()
      .filter(|n| n.kind() == FlowNodeKind::Reasoning)
      .count(),
    2
  );
  assert_eq!(
    published
      .definition
      .nodes()
      .iter()
      .filter(|n| n.kind() == FlowNodeKind::DeterministicGate)
      .count(),
    2
  );
  assert_eq!(published.definition.entry().as_str(), "eligibility");
  assert!(
    published
      .definition
      .transitions()
      .iter()
      .filter(|transition| transition.predecessor().as_str() == "classification_gate")
      .all(|transition| matches!(transition.target(), FlowTransitionTarget::Terminal(_)))
  );
  assert_eq!(
    published.definition.node(&key("classification")).unwrap().build(),
    profiles.get(&key("classification"))
  );
  assert!(
    published
      .definition
      .terminals()
      .iter()
      .any(|t| t.key().as_str() == "triaged")
  );
  assert!(
    published
      .definition
      .terminals()
      .iter()
      .any(|t| t.key().as_str() == "human_review")
  );
  assert!(
    template
      .instantiate(
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL,
        &BTreeMap::new()
      )
      .is_err()
  );
}

fn stage_selection(
  project: octacity_server_domain::ProjectId,
  node: &FlowTemplateNode,
  name: &str,
  required_context: std::collections::BTreeSet<FactoryKey>,
) -> FlowProfileSelectionBinding {
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    FactoryRoleDefinition {
      name: FactoryText::new(name).unwrap(),
      icon: None,
      avatar: None,
      description: String::new(),
      instructions: "Reviewed stage responsibility".into(),
      template: None,
      skills: BTreeMap::new(),
      concurrency_limit: None,
    },
  )
  .unwrap();
  let mut build = binding(project);
  build.result_outcome = key("observed");
  let profile = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    project,
    &role,
    FactoryRoleProfileDefinition {
      name: FactoryText::new("Reviewed profile").unwrap(),
      instructions: "Reviewed stage instructions".into(),
      template: None,
      toolchain: FactoryTaskToolchain {
        plugin: build.plugin.clone(),
        executable: build.tool.clone(),
        model: build.model_or_tool.clone(),
      },
      execution: Default::default(),
      model_parameters: BTreeMap::new(),
      model_identifier: None,
      thinking_parameter: None,
      skills: BTreeMap::new(),
      context_requirements: required_context,
      build_configuration: build.build_configuration.clone(),
      credential_purpose: FactoryCredentialPurpose::Model,
      permissions: FactoryPermissionSet::deny_all(),
      budget: node.budget,
    },
  )
  .unwrap();
  FlowProfileSelectionBinding {
    role: role.reference().clone(),
    candidates: BTreeMap::from([(key("reviewed"), FlowProfileCandidate { profile, build })]),
    policy: FlowProfileSelectionPolicy::Manual {
      candidate: Some(key("reviewed")),
    },
    failure: FlowProfileSelectionFailure::ManualSelection,
    selection_budget: BudgetUsage::default(),
  }
}

#[test]
fn triage_template_retains_each_exact_role_and_profile_selection() {
  let template = triage_template().unwrap();
  let project = octacity_server_domain::ProjectId::generate();
  let expected: BTreeMap<_, _> = template
    .nodes
    .iter()
    .filter(|node| node.kind == FlowNodeKind::Reasoning)
    .map(|node| {
      (
        node.key.clone(),
        stage_selection(project, node, &format!("Reviewed {}", node.key), Default::default()),
      )
    })
    .collect();
  let mut wire = serde_json::to_value(&template).unwrap();
  for node in wire["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .filter(|node| node["kind"] == "reasoning")
  {
    node["build_profile"] = serde_json::Value::Null;
    node["profile_selection"] = serde_json::to_value(&expected[&key(node["key"].as_str().unwrap())]).unwrap();
  }
  let selected: FlowDefinitionTemplate =
    serde_json::from_value(wire).expect("ordinary editable templates must retain explicit role/profile selections");
  let published = selected
    .instantiate(
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
      &BTreeMap::new(),
    )
    .unwrap();
  for (name, binding) in expected {
    assert_eq!(
      published.definition.node(&name).unwrap().profile_selection(),
      Some(&binding)
    );
  }
}

#[test]
fn triage_instruction_seeds_copy_into_independent_project_roles_without_execution_defaults() {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .ancestors()
    .find(|path| path.join("config/factory").is_dir())
    .unwrap()
    .join("config/factory/presets/triage");
  let template = triage_template().unwrap();
  for (file, expected, node) in [
    ("project-role-v1.json", "Project", "eligibility"),
    ("analyst-role-v1.json", "Systems analyst", "classification"),
  ] {
    let bytes = std::fs::read(root.join(file)).expect("the editable versioned triage responsibility must be bundled");
    let mut seed: FactoryRoleTemplate = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(seed.role.name.as_str(), expected);
    assert_eq!(seed.reference().unwrap().version().as_str(), "v1");
    let project = octacity_server_domain::ProjectId::generate();
    let selected = stage_selection(
      project,
      template.nodes.iter().find(|n| n.key.as_str() == node).unwrap(),
      "Operator role",
      Default::default(),
    );
    let mut definition = selected.candidates[&key("reviewed")].profile.definition().clone();
    definition.instructions = seed.profiles[&key("default")].instructions.clone();
    let copied = seed
      .instantiate(
        project,
        FactoryRoleId::generate(),
        seed.role.clone(),
        BTreeMap::from([(
          key("default"),
          FactoryRoleTemplateBinding {
            id: FactoryRoleProfileId::generate(),
            definition,
          },
        )]),
      )
      .unwrap();
    assert_eq!(copied.role.definition().instructions, seed.role.instructions);
    assert_eq!(
      copied.profiles[&key("default")].definition().instructions,
      seed.profiles[&key("default")].instructions
    );
    let original = copied.role.reference().clone();
    seed.role.instructions = "New release draft instructions".into();
    assert_ne!(
      seed.reference().unwrap(),
      *copied.role.definition().template.as_ref().unwrap()
    );
    assert_eq!(copied.role.reference(), &original);
    let profile_seed = serde_json::to_value(&seed.profiles[&key("default")]).unwrap();
    assert_eq!(
      profile_seed
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>(),
      ["instructions", "name"]
    );
  }
}

#[test]
fn triage_stage_input_contracts_keep_original_ticket_completion_and_only_the_accepted_handoff() {
  let template = triage_template().unwrap();
  let schemas = template.data_schemas().unwrap();
  let work = crate::flow_test_support::work();
  for name in ["eligibility", "classification"] {
    let node = template.nodes.iter().find(|node| node.key.as_str() == name).unwrap();
    let mut value = serde_json::json!({"task":work.artifacts().task(),"completion":work.artifacts().acceptance(),
      "work":{"priority":1,"admission_risk":"low"}});
    if name == "classification" {
      value["eligibility"] = serde_json::json!({"accepted":true,"decision":"eligible","reason":"Reviewed bounded assessment.","evidence":[]});
    }
    assert!(
      FlowPayload::new(&schemas[&node.input_schema], value).is_ok(),
      "the {name} input keeps the original Work artifacts and its explicit handoff"
    );
  }
}

// These installed-capability fixtures qualify publication only, not executor readiness.
fn reviewed_template(
  project: octacity_server_domain::ProjectId,
) -> (FlowDefinitionTemplate, FactoryConfigurationChoices) {
  use crate::flow_test_support::reference;
  let mut template = triage_template().unwrap();
  let schemas = template.data_schemas().unwrap();
  let mut entries = FactoryConfigurationChoiceEntries {
    references: vec![],
    build_configurations: vec![],
  };
  for (kind, name) in [
    (FactoryChoiceKind::AdmissionPolicy, "admission"),
    (FactoryChoiceKind::PermissionCeiling, "ceiling"),
  ] {
    entries.references.push(FactoryReferenceChoice {
      kind,
      alias: key(name),
      reference: reference(name),
    });
  }
  let mut capabilities = vec![];
  for node in template
    .nodes
    .iter_mut()
    .filter(|node| node.kind == FlowNodeKind::Reasoning)
  {
    let names: &[(&str, ContextSourceKind)] = if node.key.as_str() == "eligibility" {
      &[
        ("project-goals", ContextSourceKind::Specification),
        ("project-rules", ContextSourceKind::Policy),
      ]
    } else {
      &[("analysis-guide", ContextSourceKind::Specification)]
    };
    let context = names
      .iter()
      .map(|(name, kind)| {
        let artifact = FactoryArtifactReference::new(
          octacity_server_domain::ArtifactId::generate(),
          FactoryDigest::content_sha256(name.as_bytes()),
          name.len() as u64,
        )
        .unwrap();
        (
          key(name),
          FlowContextSelection {
            when_present: None,
            source_kind: *kind,
            source: FlowDataMapping::new(FlowValueMapping::Constant {
              value: serde_json::json!({"kind":"artifact","value":artifact}),
            })
            .unwrap(),
          },
        )
      })
      .collect();
    node.context_binding = Some(FlowContextBinding::new(None, context).unwrap());
    let mut selection = stage_selection(
      project,
      node,
      node.key.as_str(),
      names.iter().map(|(name, _)| key(name)).collect(),
    );
    let candidate = selection.candidates.get_mut(&key("reviewed")).unwrap();
    let verifier = if node.key.as_str() == "eligibility" {
      let verifier = ImmutableReference::new(
        key("native-mcp-evidence"),
        key("v1"),
        FactoryDigest::content_sha256(b"octacity.factory.native-mcp-evidence.v1"),
      );
      candidate.build.result_tool = Some(verifier.clone());
      candidate.build.result_output = key("codex-run-result");
      verifier
    } else {
      let verifier = reference("facts.verifier");
      candidate.build.evidence = vec![EvidenceRequirement::new(
        key("facts"),
        EvidenceOutputKind::Report,
        schemas[&key("classification_facts")].reference().clone(),
        verifier.clone(),
        candidate.build.plugin.clone(),
      )];
      verifier
    };
    for (suffix, reference) in [
      ("runtime", candidate.build.tool.clone()),
      ("plugin", candidate.build.plugin.clone()),
      ("model", candidate.build.model_or_tool.clone()),
      ("verifier", verifier),
    ] {
      entries.references.push(FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: if suffix == "runtime" {
          node.key.clone()
        } else {
          key(&format!("{}.{}", node.key, suffix))
        },
        reference,
      });
    }
    entries
      .build_configurations
      .push((node.key.clone(), candidate.build.build_configuration.clone()));
    capabilities.push((
      node.key.clone(),
      FactoryRoleProfileCapability {
        model_catalog: None,
        ceiling: candidate.profile.definition().clone(),
        parameters: FlowDataSchema::new(
          key("parameters"),
          key("v1"),
          FlowValueSchema::Object {
            fields: BTreeMap::new(),
          },
        )
        .unwrap(),
        public_environment: BTreeMap::new(),
        mcp_required_environment: BTreeMap::new(),
        local_permissions: LocalPermissionCeiling::fully_enforced(FactoryPermissionSet::deny_all()),
      },
    ));
    node.build_profile = None;
    node.profile_selection = Some(selection);
  }
  let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
  for (name, capability) in capabilities {
    choices = choices.with_role_profile_capability(name, capability).unwrap();
  }
  (template, choices)
}

fn publication_draft(template: &FlowDefinitionTemplate) -> Result<FactoryConfigurationDraft, FactoryError> {
  let published = template.instantiate(
    FlowDefinitionId::generate(),
    FlowDefinitionVersion::INITIAL,
    &BTreeMap::new(),
  )?;
  let budget = template.execution.budget();
  let limits = FlowAdmissionLimits::new(1, 4, 1, 7, 1, budget, FactoryPermissionSet::deny_all())?;
  let flow = FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(published.definition.reference(), vec![published.definition])?,
    limits,
    published.schemas,
    BTreeMap::new(),
  )?;
  Ok(FactoryConfigurationDraft {
    admission_policy: key("admission"),
    flow: Some(flow),
    wip_limits: FactoryWipLimits::new(1, 1)?,
    hard_budget: budget,
    permission_ceiling: key("ceiling"),
    enabled: true,
  })
}

#[test]
fn triage_normal_publication_and_admission_freeze_profiles_context_schemas_and_gate_policy() {
  let original = crate::flow_test_support::work();
  let project = original.subject().project_id();
  let (mut template, choices) = reviewed_template(project);
  let config = FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    project,
    FactoryDigest::from_bytes([81; 32]),
    publication_draft(&template).unwrap(),
    &choices,
  )
  .unwrap();
  let work = WorkEnvelope::new(
    original.id(),
    config.reference().clone(),
    original.external_identity().clone(),
    original.subject().clone(),
    original.artifacts().clone(),
    WorkClassification::new(original.priority(), original.risk(), original.metadata().clone()),
  )
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let admitted = AdmittedFlow::from_configuration(&config, &run).unwrap();
  let frozen = serde_json::to_vec(&admitted).unwrap();
  let prepare = |name: &str| {
    let root = admitted.closure().definition(admitted.closure().root()).unwrap();
    let schema = admitted
      .data_schema(root.node(&key(name)).unwrap().input_schema().unwrap())
      .unwrap();
    FlowNodeInput::prepare(FlowInputPreparation {
      work: &work,
      admitted: &admitted,
      flow: admitted.root_run(),
      cycle: admitted.initial_cycle(),
      node: key(name),
      schema,
      history: FlowRuntimeHistory {
        stops: &[],
        flow_runs: &[admitted.root_run().clone()],
        cycles: &[admitted.initial_cycle().clone()],
        attempts: &[],
        completions: &[],
      },
      records: &[],
      inputs: &[],
      incoming: None,
    })
  };
  let input = prepare("eligibility").unwrap();
  assert_eq!(
    input.payload().value(),
    &serde_json::json!({"task":work.artifacts().task(),"completion":work.artifacts().acceptance(),
    "work":{"priority":work.priority().get(),"admission_risk":work.risk()}})
  );
  assert_eq!(input.subject(), work.subject());
  assert!(input.sources().is_empty());
  assert!(
    prepare("classification").is_err(),
    "the analyst cannot borrow an absent or unaccepted gate handoff"
  );

  let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
  for node in template
    .nodes
    .iter()
    .filter(|node| node.kind == FlowNodeKind::Reasoning)
  {
    let retained = definition.node(&node.key).unwrap();
    assert_eq!(retained.context_binding(), node.context_binding.as_ref());
    assert_eq!(retained.profile_selection(), node.profile_selection.as_ref());
    assert_eq!(retained.budget(), node.budget);
  }
  for node in template
    .nodes
    .iter()
    .filter(|node| node.kind == FlowNodeKind::DeterministicGate)
  {
    assert_eq!(
      definition.node(&node.key).unwrap().gate(),
      Some(&node.gate.as_ref().unwrap().binding().unwrap())
    );
  }
  template
    .schemas
    .get_mut(&key("eligibility_observations"))
    .unwrap()
    .version = key("v2");
  let gate = template
    .nodes
    .iter_mut()
    .find(|n| n.key.as_str() == "eligibility_gate")
    .unwrap()
    .gate
    .as_mut()
    .unwrap();
  let mut edited = serde_json::to_value(&*gate).unwrap();
  edited["precondition"] = serde_json::json!({"operation":"all","predicates":[edited["precondition"].clone(),
    {"operation":"compare","left":{"source":"path","path":"/conclusion/assessment/fit"},"comparison":"equal","right":{"source":"literal","value":"fits"}}]});
  *gate = serde_json::from_value(edited).unwrap();
  template
    .nodes
    .iter_mut()
    .find(|n| n.key.as_str() == "classification")
    .unwrap()
    .context_binding = None;
  assert!(
    publication_draft(&template).is_err(),
    "a selected profile's required Context cannot be silently removed"
  );
  template
    .nodes
    .iter_mut()
    .find(|n| n.key.as_str() == "classification")
    .unwrap()
    .context_binding = definition
    .node(&key("classification"))
    .unwrap()
    .context_binding()
    .cloned();
  let replacement = config
    .replace(
      FactoryConfigurationVersion::new(2).unwrap(),
      FactoryDigest::from_bytes([82; 32]),
      publication_draft(&template).unwrap(),
      &choices,
    )
    .unwrap();
  assert_ne!(replacement.flow().closure.root(), config.flow().closure.root());
  let revised = replacement
    .flow()
    .closure
    .definition(replacement.flow().closure.root())
    .unwrap();
  assert_ne!(
    revised.node(&key("eligibility_gate")).unwrap().gate(),
    definition.node(&key("eligibility_gate")).unwrap().gate()
  );
  assert_ne!(
    revised
      .node(&key("eligibility"))
      .unwrap()
      .outcome(&key("observed"))
      .unwrap()
      .schema(),
    definition
      .node(&key("eligibility"))
      .unwrap()
      .outcome(&key("observed"))
      .unwrap()
      .schema()
  );
  assert_eq!(serde_json::to_vec(&admitted).unwrap(), frozen);
  let restored: AdmittedFlow = serde_json::from_slice(&frozen).unwrap();
  assert_eq!(restored.closure().root(), admitted.closure().root());
}

#[test]
fn triage_template_names_do_not_dispatch_and_invalid_profile_authority_or_budget_is_rejected() {
  let original = triage_template().unwrap();
  let wire = serde_json::to_string(&original)
    .unwrap()
    .replace("eligibility", "scope_check")
    .replace("classification", "analysis_check");
  let renamed: FlowDefinitionTemplate = serde_json::from_str(&wire).unwrap();
  let project = octacity_server_domain::ProjectId::generate();
  let profiles: BTreeMap<_, _> = renamed
    .nodes
    .iter()
    .filter_map(|node| node.build_profile.as_ref())
    .map(|name| {
      let mut selected = binding(project);
      selected.result_outcome = key("observed");
      (name.clone(), selected)
    })
    .collect();
  let published = renamed
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  assert!(published.definition.node(&key("scope_check")).is_some());
  assert!(published.definition.node(&key("analysis_check")).is_some());
  let (mut template, mut choices) = reviewed_template(project);
  let role = template.nodes[0].profile_selection.as_ref().unwrap().role.id;
  choices.exclude_role(role);
  assert!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      project,
      FactoryDigest::from_bytes([83; 32]),
      publication_draft(&template).unwrap(),
      &choices
    )
    .is_err(),
    "withdrawn role authority cannot publish"
  );
  template.nodes[0]
    .profile_selection
    .as_mut()
    .unwrap()
    .selection_budget
    .tokens = 1;
  assert!(
    publication_draft(&template).is_err(),
    "selection plus candidate usage cannot exceed the node budget"
  );
  template.nodes[0]
    .profile_selection
    .as_mut()
    .unwrap()
    .selection_budget
    .tokens = 0;
  template.nodes[0].build_profile = Some(key("duplicate"));
  let binding = template.nodes[0].profile_selection.as_ref().unwrap().candidates[&key("reviewed")]
    .build
    .clone();
  assert!(
    template
      .instantiate(
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL,
        &BTreeMap::from([(key("duplicate"), binding)])
      )
      .is_err(),
    "static binding and role selection cannot silently override one another"
  );
}
