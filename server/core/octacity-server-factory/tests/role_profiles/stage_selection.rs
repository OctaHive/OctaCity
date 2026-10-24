use super::*;
use octacity_server_factory::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn budget() -> BudgetLimit {
  BudgetLimit::new(2, 2_000, 300, 150, 2_048).unwrap()
}
fn stage(project: ProjectId) -> Value {
  stage_with(project, |_, _| {})
}
fn stage_with(project: ProjectId, change: impl Fn(&str, &mut FactoryRoleProfileDefinition)) -> Value {
  let role = role(project, "Implement the declared stage contract.");
  let candidates: BTreeMap<_, _> = ["default", "pro", "specialized"]
    .into_iter()
    .map(|name| {
      let mut definition = profile(project, name);
      definition.context_requirements.clear();
      change(name, &mut definition);
      let profile = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).unwrap();
      let definition = profile.definition();
      let build = FlowBuildBinding {
        tool: definition.toolchain.executable.clone(),
        result_tool: Some(exact("result.verifier")),
        plugin: definition.toolchain.plugin.clone(),
        model_or_tool: definition.toolchain.model.clone(),
        task_digest: FactoryDigest::from_bytes([9; 32]),
        build_configuration: definition.build_configuration.clone(),
        capture_write_paths: None,
        capture_deliverables: None,
        result_output: key("observations"),
        result_output_kind: EvidenceOutputKind::Report,
        result_outcome: key("observed"),
        evidence: vec![],
      };
      (name, json!({"profile": profile, "build": build}))
    })
    .collect();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("implementation"),
    kind: FlowNodeKind::Reasoning,
    input_schema: Some(schema().reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("observed"),
      FlowOutcomeKind::Success,
      schema().reference().clone(),
    )],
    budget: budget(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap();
  let mut node = serde_json::to_value(node).unwrap();
  node["profile_selection"] = json!({"role": role.reference(), "candidates": candidates,
    "policy": {"mode": "manual"}, "failure": {"kind": "manual_selection"},
    "selection_budget": {"attempts": 1, "elapsed_millis": 1_000, "tokens": 100, "cost_micro_units": 50, "output_bytes": 1_024}});
  node
}
fn schema() -> FlowDataSchema {
  FlowDataSchema::new(key("stage.data"), key("v1"), FlowValueSchema::Boolean).unwrap()
}
fn flow(node: FlowNodeDefinition) -> FactoryFlowConfiguration {
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema().reference().clone()),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("implementation"),
      key("observed"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema().reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget(), FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget(), FactoryPermissionSet::deny_all()).unwrap(),
    vec![schema()],
    BTreeMap::new(),
  )
  .unwrap()
}

#[test]
fn publication_and_admission_retain_three_exact_profile_candidates() {
  let project = ProjectId::generate();
  let original = stage(project);
  let node: FlowNodeDefinition =
    serde_json::from_value(original.clone()).expect("selection binding is a public node contract");
  let flow = flow(node);
  flow
    .validate(
      &FactoryConfigurationRef::new(
        FactoryConfigurationId::generate(),
        FactoryConfigurationVersion::INITIAL,
        project,
        FactoryDigest::from_bytes([1; 32]),
      ),
      budget(),
      FactoryWipLimits::new(1, 1).unwrap(),
    )
    .unwrap();
  let published = publish(project, flow.clone(), &choices(&flow, None)).unwrap();
  let work = work(&published);
  let admitted =
    AdmittedFlow::from_configuration(&published, &FactoryRun::admitted(FactoryRunId::generate(), &work)).unwrap();
  admitted.validated().unwrap();
  let restored: AdmittedFlow = serde_json::from_value(serde_json::to_value(&admitted).unwrap()).unwrap();
  restored.validated().unwrap();
  let root = restored.closure().definition(restored.closure().root()).unwrap();
  let retained = serde_json::to_value(root.node(&key("implementation")).unwrap()).unwrap();
  assert_eq!(retained["profile_selection"], original["profile_selection"]);
  assert_eq!(
    retained["profile_selection"]["candidates"].as_object().unwrap().len(),
    3
  );
  assert!(retained.get("build").is_none());
}

#[test]
fn candidates_cannot_change_the_stage_contract_or_authority() {
  let project = ProjectId::generate();
  let valid = stage(project);
  let mut cases = Vec::new();
  for (name, path, replacement) in [
    (
      "static and selected bindings",
      "/build",
      valid["profile_selection"]["candidates"]["default"]["build"].clone(),
    ),
    ("non-model node", "/kind", json!("build_command")),
    ("missing task schema", "/input_schema", Value::Null),
    (
      "different role",
      "/profile_selection/candidates/pro/profile",
      stage(project)["profile_selection"]["candidates"]["pro"]["profile"].clone(),
    ),
    (
      "wrong harness",
      "/profile_selection/candidates/pro/build/tool",
      json!(exact("other.harness")),
    ),
    (
      "wrong model",
      "/profile_selection/candidates/pro/build/model_or_tool",
      json!(exact("other.model")),
    ),
    (
      "wrong plugin",
      "/profile_selection/candidates/pro/build/plugin",
      json!(exact("other.plugin")),
    ),
    (
      "different output",
      "/profile_selection/candidates/pro/build/result_output",
      json!("different"),
    ),
    (
      "different verifier",
      "/profile_selection/candidates/pro/build/result_tool",
      json!(exact("other.verifier")),
    ),
    (
      "different capture",
      "/profile_selection/candidates/pro/build/capture_write_paths",
      json!(["src"]),
    ),
    (
      "undeclared result schema",
      "/profile_selection/candidates/pro/build/result_outcome",
      json!("invented"),
    ),
  ] {
    let mut changed = valid.clone();
    // These optional fields are absent from the serialized static contract.
    if path == "/build" {
      changed["build"] = replacement;
    } else if path.ends_with("/capture_write_paths") {
      changed["profile_selection"]["candidates"]["pro"]["build"]["capture_write_paths"] = replacement;
    } else {
      *changed.pointer_mut(path).unwrap() = replacement;
    }
    cases.push((name, changed));
  }
  cases.push((
    "missing required context",
    stage_with(project, |name, profile| {
      if name == "pro" {
        profile.context_requirements.insert(key("requirements"));
      }
    }),
  ));
  cases.push((
    "different credential purpose",
    stage_with(project, |name, profile| {
      if name == "pro" {
        profile.credential_purpose = FactoryCredentialPurpose::Evaluator;
      }
    }),
  ));
  cases.push((
    "profile authority exceeds stage ceiling",
    stage_with(project, |name, profile| {
      if name == "specialized" {
        profile.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
          plugins: vec![exact("additional.plugin")],
          ..Default::default()
        })
        .unwrap();
      }
    }),
  ));
  let accepted: Vec<_> = cases
    .into_iter()
    .filter_map(|(name, value)| {
      serde_json::from_value::<FlowNodeDefinition>(value)
        .is_ok()
        .then_some(name)
    })
    .collect();
  assert!(accepted.is_empty(), "incompatible candidates accepted: {accepted:?}");
}

#[test]
fn selection_reserves_overhead_plus_each_candidate_and_bounds_the_closure() {
  let project = ProjectId::generate();
  let valid = stage(project);
  let mut cases = Vec::new();
  for (resource, value) in [
    ("attempts", json!(2)),
    ("elapsed_millis", json!(1_001)),
    ("tokens", json!(101)),
    ("cost_micro_units", json!(51)),
    ("output_bytes", json!(1_025)),
    ("tokens", json!(u64::MAX)),
  ] {
    let mut changed = valid.clone();
    changed["profile_selection"]["selection_budget"][resource] = value;
    cases.push((resource.to_string(), changed));
  }
  let mut expensive_reserve = stage_with(project, |name, profile| {
    if name == "specialized" {
      profile.budget = BudgetLimit::new(1, 1_000, 201, 100, 1_024).unwrap();
    }
  });
  expensive_reserve["profile_selection"]["failure"] = json!({"kind":"reserve_profile","candidate":"specialized"});
  cases.push(("reserve exceeds the combined ceiling".into(), expensive_reserve));
  let mut empty = valid.clone();
  empty["profile_selection"]["candidates"] = json!({});
  cases.push(("empty candidates".into(), empty));
  let mut duplicate = valid.clone();
  duplicate["profile_selection"]["candidates"]["pro"] = duplicate["profile_selection"]["candidates"]["default"].clone();
  cases.push(("duplicate profile aliases".into(), duplicate));
  let mut unbounded = valid.clone();
  let mut candidates = serde_json::Map::new();
  for index in 0..=MAX_FACTORY_ROLE_PROFILE_ENTRIES {
    let mut candidate = valid["profile_selection"]["candidates"]["default"].clone();
    // Each profile remains canonical and independent; the count itself must be rejected.
    let mut definition = profile(project, "medium");
    definition.context_requirements.clear();
    let role: FactoryRole = serde_json::from_value(json!({"reference": valid["profile_selection"]["role"], "definition": role(project, "Implement the declared stage contract.").definition()})).unwrap();
    candidate["profile"] =
      json!(FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).unwrap());
    candidate["build"]["build_configuration"] = candidate["profile"]["definition"]["build_configuration"].clone();
    candidates.insert(format!("candidate.{index}"), candidate);
  }
  unbounded["profile_selection"]["candidates"] = Value::Object(candidates);
  cases.push(("too many candidates".into(), unbounded));
  let mut unknown_reserve = valid;
  unknown_reserve["profile_selection"]["failure"] = json!({"kind":"reserve_profile", "candidate":"not.declared"});
  cases.push(("undeclared reserve".into(), unknown_reserve));
  let accepted: Vec<_> = cases
    .into_iter()
    .filter_map(|(name, value)| {
      serde_json::from_value::<FlowNodeDefinition>(value)
        .is_ok()
        .then_some(name)
    })
    .collect();
  assert!(accepted.is_empty(), "unbounded selections accepted: {accepted:?}");
}

#[test]
fn immutable_selection_policies_pin_inputs_rules_candidate_mappings_and_failure() {
  let valid = stage(ProjectId::generate());
  let condition = json!({"operation":"compare", "left":{"source":"path","path":"/risk"}, "comparison":"equal", "right":{"source":"literal","value":"high"}});
  let rules = json!({"mode":"rules", "rules":[{"when":condition,"candidate":"pro"}], "otherwise":"default"});
  let input = FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap();
  let decision = json!({"mode":"decision_api", "policy":exact("profile.selector"), "input":input});
  let mut pinned = Vec::new();
  for policy in [valid["profile_selection"]["policy"].clone(), rules.clone(), decision] {
    let mut node = valid.clone();
    node["profile_selection"]["policy"] = policy.clone();
    node["profile_selection"]["failure"] = json!({"kind":"reserve_profile","candidate":"specialized"});
    let node: FlowNodeDefinition = serde_json::from_value(node).expect("each mode has a closed publication contract");
    let flow = flow(node);
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    assert_eq!(
      serde_json::to_value(root.nodes()[0].profile_selection()).unwrap()["policy"],
      policy
    );
    pinned.push(root.reference().digest());
  }
  assert_ne!(pinned[0], pinned[1]);
  assert_ne!(pinned[1], pinned[2]);
  for policy in [
    json!({"mode":"rules", "rules":[{"when":condition,"candidate":"invented"}], "otherwise":"default"}),
    json!({"mode":"rules", "rules":[], "otherwise":"invented"}),
    json!({"mode":"rules", "rules":[{"when":{"operation":"absent","path":"invalid-pointer"},"candidate":"pro"}], "otherwise":"default"}),
    json!({"mode":"rules", "rules":vec![json!({"when":condition,"candidate":"pro"}); MAX_FACTORY_ROLE_PROFILE_ENTRIES+1], "otherwise":"default"}),
  ] {
    let mut node = valid.clone();
    node["profile_selection"]["policy"] = policy;
    assert!(serde_json::from_value::<FlowNodeDefinition>(node).is_err());
  }
}

fn choices(flow: &FactoryFlowConfiguration, omit: Option<&str>) -> FactoryConfigurationChoices {
  let root = flow.closure.definition(flow.closure.root()).unwrap();
  let selection = root.nodes()[0].profile_selection().unwrap();
  let mut entries = FactoryConfigurationChoiceEntries::default();
  entries.references.extend([
    FactoryReferenceChoice {
      kind: FactoryChoiceKind::AdmissionPolicy,
      alias: key("admission"),
      reference: exact("admission"),
    },
    FactoryReferenceChoice {
      kind: FactoryChoiceKind::PermissionCeiling,
      alias: key("permissions"),
      reference: exact("permissions"),
    },
  ]);
  for (name, candidate) in &selection.candidates {
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::NodeCapability,
      alias: name.clone(),
      reference: candidate.build.tool.clone(),
    });
    for (dependency, reference) in [
      ("verifier", candidate.build.result_tool()),
      ("plugin", &candidate.build.plugin),
      ("model", &candidate.build.model_or_tool),
    ] {
      entries.references.push(FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key(&format!("{name}.{dependency}")),
        reference: reference.clone(),
      });
    }
    for (index, reference) in candidate.profile.definition().skills.values().enumerate() {
      entries.references.push(FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key(&format!("{name}.skill.{index}")),
        reference: reference.clone(),
      });
    }
    entries
      .build_configurations
      .push((name.clone(), candidate.build.build_configuration.clone()));
  }
  let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
  for (name, candidate) in &selection.candidates {
    if Some(name.as_str()) == omit {
      continue;
    }
    choices = choices
      .with_role_profile_capability(
        name.clone(),
        FactoryRoleProfileCapability {
          ceiling: candidate.profile.definition().clone(),
          parameters: FlowDataSchema::new(
            key("parameters"),
            key("v1"),
            FlowValueSchema::Object {
              fields: [(
                key("reasoning_effort"),
                FlowFieldSchema::required(FlowValueSchema::String {
                  min_bytes: 1,
                  max_bytes: 100,
                }),
              )]
              .into(),
            },
          )
          .unwrap(),
          public_environment: BTreeMap::new(),
          local_permissions: LocalPermissionCeiling::fully_enforced(FactoryPermissionSet::deny_all()),
        },
      )
      .unwrap();
  }
  choices
}
fn publish(
  project: ProjectId,
  flow: FactoryFlowConfiguration,
  choices: &FactoryConfigurationChoices,
) -> Result<FactoryConfiguration, FactoryError> {
  FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    project,
    FactoryDigest::from_bytes([1; 32]),
    FactoryConfigurationDraft {
      flow: Some(flow),
      admission_policy: key("admission"),
      permission_ceiling: key("permissions"),
      wip_limits: FactoryWipLimits::new(1, 1).unwrap(),
      hard_budget: budget(),
      enabled: true,
    },
    choices,
  )
}
#[test]
fn configuration_publication_requires_installed_compatibility_for_every_candidate() {
  let project = ProjectId::generate();
  let flow = flow(serde_json::from_value(stage(project)).unwrap());
  assert!(publish(project, flow.clone(), &choices(&flow, None)).is_ok());
  assert!(
    publish(project, flow.clone(), &choices(&flow, Some("specialized"))).is_err(),
    "an unavailable third candidate must not be hidden by valid primary profiles"
  );
}

#[test]
fn admission_cannot_reuse_a_profile_closure_in_another_project() {
  let project = ProjectId::generate();
  let flow = flow(serde_json::from_value(stage(project)).unwrap());
  let foreign = FactoryConfigurationRef::new(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    ProjectId::generate(),
    FactoryDigest::from_bytes([1; 32]),
  );
  assert!(
    flow
      .validate(&foreign, budget(), FactoryWipLimits::new(1, 1).unwrap())
      .is_err()
  );
}

fn work(configuration: &FactoryConfiguration) -> WorkEnvelope {
  let project = configuration.reference().project_id();
  let artifact = |byte| {
    FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::from_bytes([byte; 32]),
      10,
    )
    .unwrap()
  };
  WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration.reference().clone(),
    ExternalWorkIdentity::new("manual.task").unwrap(),
    ExactSubject::new(
      project,
      octacity_server_domain::RepositoryId::generate(),
      octacity_server_domain::ImmutableRevision::new("base").unwrap(),
    ),
    WorkArtifacts::new(artifact(1), artifact(2), vec![]).unwrap(),
    WorkClassification::new(
      WorkPriority::new(1).unwrap(),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .unwrap()
}

#[test]
fn configured_selection_criteria_are_bounded_like_common_flow_predicates() {
  let mut node = stage(ProjectId::generate());
  node["profile_selection"]["policy"] = json!({"mode":"rules", "rules":[{"when":{"operation":"exists","operand":{"source":"literal","value":"x".repeat(MAX_FLOW_GATE_PARAMETER_BYTES)}},"candidate":"pro"}],"otherwise":"default"});
  assert!(
    serde_json::from_value::<FlowNodeDefinition>(node).is_err(),
    "selection must not bypass the common parameter byte bound"
  );
}

#[test]
fn required_context_is_explicit_and_private_profile_content_stays_out_of_diagnostics() {
  let project = ProjectId::generate();
  let mut value = stage_with(project, |_, profile| {
    profile.context_requirements.insert(key("requirements"));
  });
  let source = FactoryArtifactReference::new(
    octacity_server_domain::ArtifactId::generate(),
    FactoryDigest::from_bytes([11; 32]),
    10,
  )
  .unwrap();
  let context = FlowContextBinding::new(
    None,
    [(
      key("requirements"),
      FlowContextSelection {
        when_present: None,
        source_kind: ContextSourceKind::Specification,
        source: FlowDataMapping::new(FlowValueMapping::Constant {
          value: json!(FactoryContextReference::Artifact(source)),
        })
        .unwrap(),
      },
    )]
    .into(),
  )
  .unwrap();
  value["context_binding"] = json!(context);
  let node: FlowNodeDefinition = serde_json::from_value(value.clone()).unwrap();
  let build = node.profile_selection().unwrap().candidates[&key("default")]
    .build
    .clone();
  assert!(node.clone().with_build(build.clone()).is_err());
  let mut static_wire = value.clone();
  static_wire.as_object_mut().unwrap().remove("profile_selection");
  static_wire["build"] = json!(build);
  let static_node: FlowNodeDefinition = serde_json::from_value(static_wire).unwrap();
  assert!(
    static_node
      .with_profile_selection(node.profile_selection().unwrap().clone())
      .is_err()
  );
  let debug = format!("{node:?}");
  assert!(!debug.contains("Use selected skills."));
  assert!(!debug.contains("Local rules"));
  value["context_binding"]["entries"]["requirements"]["when_present"] = json!("/optional");
  assert!(
    serde_json::from_value::<FlowNodeDefinition>(value).is_err(),
    "a conditional source cannot satisfy an unconditional profile requirement"
  );
}

#[test]
fn fixed_manual_selection_names_one_exact_declared_candidate() {
  let mut node = stage(ProjectId::generate());
  node["profile_selection"]["policy"] = json!({"mode":"manual","candidate":"specialized"});
  assert!(serde_json::from_value::<FlowNodeDefinition>(node.clone()).is_ok());
  node["profile_selection"]["policy"]["candidate"] = json!("not.declared");
  assert!(
    serde_json::from_value::<FlowNodeDefinition>(node).is_err(),
    "a fixed manual choice cannot invent or resolve another profile"
  );
}

#[test]
fn deleted_agent_candidates_are_unavailable_without_changing_retained_flow_history() {
  let project = ProjectId::generate();
  let node: FlowNodeDefinition = serde_json::from_value(stage(project)).unwrap();
  let selected = node.profile_selection().unwrap().clone();
  let retained_flow = flow(node);
  let mut installed = choices(&retained_flow, None);
  assert!(selected.validate_capabilities(&installed).is_ok());
  let retained = serde_json::to_value(&retained_flow).unwrap();
  installed.exclude_role(selected.role.id);
  for candidate in selected.candidates.values() {
    assert!(candidate.validate_capabilities(&installed).is_err());
  }
  assert!(publish(project, retained_flow.clone(), &installed).is_err());
  assert_eq!(serde_json::to_value(&retained_flow).unwrap(), retained);
  let other = flow(serde_json::from_value(stage(project)).unwrap());
  let mut other_choices = choices(&other, None);
  other_choices.exclude_role(selected.role.id);
  assert!(publish(project, other, &other_choices).is_ok());
}
