use super::factory::*;
use octacity_server_domain::ArtifactId;
use std::collections::BTreeMap;
fn flow_id(value: u128) -> FlowDefinitionId {
  FlowDefinitionId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}
fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).unwrap()
}
fn exact(name: &str) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), digest(8))
}
pub(super) fn journey(checked: bool, nested: bool, budget: BudgetLimit) -> FactoryFlowConfiguration {
  let execution = FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 4).unwrap();
  let node_budget = BudgetLimit::new(1, 100, 10, 10, 100).unwrap();
  let mut extra_definitions = Vec::new();
  let mut phase = |input: TriageSchema, output: TriageSchema| {
    let schema = output.reference().unwrap();
    let mut draft = FlowDefinitionInput {
      id: flow_id(if input == TriageSchema::EligibilityInput {
        100
      } else {
        102
      }),
      version: FlowDefinitionVersion::new(1).unwrap(),
      input_schema: Some(input.reference().unwrap()),
      entry: key("observe"),
      nodes: vec![
        FlowNodeDefinition::new(FlowNodeDefinitionInput {
          key: key("observe"),
          kind: FlowNodeKind::Reasoning,
          input_schema: Some(input.reference().unwrap()),
          outcomes: vec![FlowOutcomeDefinition::new(
            key("observed"),
            FlowOutcomeKind::Success,
            schema.clone(),
          )],
          budget: node_budget,
          permissions: FactoryPermissionSet::deny_all(),
          required: true,
          subflow: None,
        })
        .unwrap(),
      ],
      transitions: vec![FlowTransition::new(
        key("observe"),
        key("observed"),
        FlowTransitionTarget::Terminal(key("observed")),
      )],
      terminals: vec![FlowTerminalDefinition::new(key("observed"), schema)],
      context_projections: vec![],
      data_projections: vec![],
      execution: execution.clone(),
    };
    if checked {
      draft.nodes.push(
        FlowNodeDefinition::new(FlowNodeDefinitionInput {
          key: key("verify"),
          kind: FlowNodeKind::DeterministicGate,
          input_schema: Some(output.reference().unwrap()),
          outcomes: vec![FlowOutcomeDefinition::new(
            key("observed"),
            FlowOutcomeKind::Success,
            output.reference().unwrap(),
          )],
          budget: node_budget,
          permissions: FactoryPermissionSet::deny_all(),
          required: true,
          subflow: None,
        })
        .unwrap(),
      );
      draft.transitions[0] = FlowTransition::new(
        key("observe"),
        key("observed"),
        FlowTransitionTarget::Node(key("verify")),
      );
      draft.transitions.push(FlowTransition::new(
        key("verify"),
        key("observed"),
        FlowTransitionTarget::Terminal(key("observed")),
      ));
      draft.data_projections.push(FlowDataProjection::new(
        key("observe"),
        key("observed"),
        key("verify"),
        output.reference().unwrap(),
      ));
    }
    let leaf = FlowDefinition::new(draft).unwrap();
    if !nested {
      return leaf;
    }
    let parent = FlowDefinition::new(FlowDefinitionInput {
      id: flow_id(if input == TriageSchema::EligibilityInput {
        101
      } else {
        103
      }),
      version: FlowDefinitionVersion::new(1).unwrap(),
      input_schema: Some(input.reference().unwrap()),
      entry: key("observe"),
      nodes: vec![
        FlowNodeDefinition::new(FlowNodeDefinitionInput {
          key: key("observe"),
          kind: FlowNodeKind::SubflowCall,
          input_schema: Some(input.reference().unwrap()),
          outcomes: vec![FlowOutcomeDefinition::new(
            key("observed"),
            FlowOutcomeKind::Success,
            output.reference().unwrap(),
          )],
          budget: execution.budget(),
          permissions: FactoryPermissionSet::deny_all(),
          required: true,
          subflow: Some(leaf.reference()),
        })
        .unwrap(),
      ],
      transitions: vec![FlowTransition::new(
        key("observe"),
        key("observed"),
        FlowTransitionTarget::Terminal(key("observed")),
      )],
      terminals: vec![FlowTerminalDefinition::new(
        key("observed"),
        output.reference().unwrap(),
      )],
      context_projections: vec![],
      data_projections: vec![],
      execution: execution.clone(),
    })
    .unwrap();
    extra_definitions.push(leaf);
    parent
  };
  let eligibility = phase(TriageSchema::EligibilityInput, TriageSchema::EligibilityResult);
  let classification = phase(TriageSchema::ClassificationInput, TriageSchema::TriageResult);
  let triage = compose_triage_flow(
    flow_id(104),
    FlowDefinitionVersion::new(1).unwrap(),
    &eligibility,
    &classification,
    TriageRoute::ALL.to_vec(),
    execution.clone(),
  )
  .unwrap();
  let schema = TriageSchema::Decision.reference().unwrap();
  let routes = [
    TriageRoute::Research,
    TriageRoute::Requirements,
    TriageRoute::ProtectedTest,
    TriageRoute::Development,
    TriageRoute::VerificationOnly,
  ];
  let mut nodes = vec![
    FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key("triage"),
      kind: FlowNodeKind::SubflowCall,
      input_schema: Some(TriageSchema::EligibilityInput.reference().unwrap()),
      outcomes: TriageRoute::ALL
        .into_iter()
        .map(|route| FlowOutcomeDefinition::new(key(route.as_str()), FlowOutcomeKind::Success, schema.clone()))
        .collect(),
      budget: execution.budget(),
      permissions: FactoryPermissionSet::deny_all(),
      required: true,
      subflow: Some(triage.reference()),
    })
    .unwrap(),
  ];
  let successor = |route: TriageRoute| match route {
    TriageRoute::ProtectedTest | TriageRoute::Development => "accepted_work_contract",
    _ => route.as_str(),
  };
  for name in [
    "research",
    "requirements",
    "accepted_work_contract",
    "verification_only",
  ] {
    nodes.push(
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: key(name),
        kind: FlowNodeKind::DeterministicGate,
        input_schema: Some(schema.clone()),
        outcomes: vec![FlowOutcomeDefinition::new(
          key("ready"),
          FlowOutcomeKind::Success,
          schema.clone(),
        )],
        budget: node_budget,
        permissions: FactoryPermissionSet::deny_all(),
        required: true,
        subflow: None,
      })
      .unwrap(),
    );
  }
  let mut transitions = TriageRoute::ALL
    .into_iter()
    .map(|route| {
      FlowTransition::new(
        key("triage"),
        key(route.as_str()),
        if routes.contains(&route) {
          FlowTransitionTarget::Node(key(successor(route)))
        } else {
          FlowTransitionTarget::Terminal(key(route.as_str()))
        },
      )
    })
    .collect::<Vec<_>>();
  for name in [
    "research",
    "requirements",
    "accepted_work_contract",
    "verification_only",
  ] {
    transitions.push(FlowTransition::new(
      key(name),
      key("ready"),
      FlowTransitionTarget::Terminal(key("finished")),
    ));
  }
  let root = FlowDefinition::new(FlowDefinitionInput {
    id: flow_id(105),
    version: FlowDefinitionVersion::new(1).unwrap(),
    input_schema: Some(TriageSchema::EligibilityInput.reference().unwrap()),
    entry: key("triage"),
    nodes,
    transitions,
    terminals: ["rejection", "escalation", "already_fixed", "finished"]
      .into_iter()
      .map(|name| FlowTerminalDefinition::new(key(name), schema.clone()))
      .collect(),
    context_projections: vec![],
    data_projections: TriageRoute::ALL
      .into_iter()
      .filter(|route| routes.contains(route))
      .map(|route| {
        FlowDataProjection::new(
          key("triage"),
          key(route.as_str()),
          key(successor(route)),
          schema.clone(),
        )
      })
      .collect(),
    execution: execution.clone(),
  })
  .unwrap();
  FactoryFlowConfiguration {
    closure: PinnedFlowDefinitionClosure::new(
      root.reference(),
      [
        vec![root, triage.clone(), eligibility, classification],
        extra_definitions,
      ]
      .concat(),
    )
    .unwrap(),
    limits: FlowAdmissionLimits::product_defaults(4, execution.budget(), FactoryPermissionSet::deny_all()).unwrap(),
    triage: FactoryTriageConfiguration {
      definition: triage.reference(),
      project_goals: artifact(20),
      eligibility: FactoryTriageExecutionProfile {
        producer: exact("fake.triage"),
        model_or_tool: exact("fake.model"),
        task_digest: digest(32),
      },
      classification: FactoryTriageExecutionProfile {
        producer: exact("fake.triage"),
        model_or_tool: exact("fake.model"),
        task_digest: digest(32),
      },
      policy: TriagePolicySettings {
        max_risk: RiskClass::Medium,
        small_work_route: Some(TriageRoute::ProtectedTest),
        budget: execution.budget(),
      },
      project_priority: 30,
      pools: routes
        .into_iter()
        .map(|route| {
          (
            route,
            PhasePoolPolicy {
              phase: key(route.as_str()),
              order: vec![
                PhasePoolOrder::Severity,
                PhasePoolOrder::ProjectPriority,
                PhasePoolOrder::Age,
              ],
              max_wip: 2,
              max_project_wip: 2,
              budget: execution.budget(),
            },
          )
        })
        .collect(),
      capabilities: BTreeMap::new(),
    },
  }
}
