use crate::{
  FactoryDigest, FactoryError, FactoryKey, FlowDataProjection, FlowDefinition, FlowDefinitionId, FlowDefinitionInput,
  FlowDefinitionRef, FlowDefinitionVersion, FlowExecutionPolicy, FlowNodeDefinition, FlowNodeDefinitionInput,
  FlowNodeKind, FlowOutcomeDefinition, FlowOutcomeKind, FlowTerminalDefinition, FlowTransition, FlowTransitionTarget,
  ImmutableReference,
};

use super::{TriageRoute, invalid};

/// Canonical roles owned by the v1 two-phase triage composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriageNode {
  /// Replaceable eligibility observation subflow.
  Eligibility,
  /// Code-owned eligibility decision gate.
  EligibilityPolicy,
  /// Replaceable classification observation subflow.
  Classification,
  /// Code-owned finite routing decision gate.
  RoutingPolicy,
}

impl TriageNode {
  /// Returns the canonical immutable v1 node key.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Eligibility => "eligibility",
      Self::EligibilityPolicy => "eligibility_policy",
      Self::Classification => "classification",
      Self::RoutingPolicy => "routing_policy",
    }
  }

  /// Returns the validated canonical node key.
  #[must_use]
  pub fn key(self) -> FactoryKey {
    FactoryKey::new(self.as_str()).expect("canonical triage node key")
  }

  fn from_key(key: &FactoryKey) -> Option<Self> {
    [
      Self::Eligibility,
      Self::EligibilityPolicy,
      Self::Classification,
      Self::RoutingPolicy,
    ]
    .into_iter()
    .find(|role| role.as_str() == key.as_str())
  }
}

/// Exact versioned schema identities used at triage nested-flow boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriageSchema {
  /// Frozen eligibility input.
  EligibilityInput,
  /// Non-authoritative eligibility observations.
  EligibilityResult,
  /// Policy-accepted eligibility input and observations.
  ClassificationInput,
  /// Non-authoritative classification observations.
  TriageResult,
  /// Phase-tagged `TriageDisposition` selected by deterministic policy.
  Decision,
}

impl TriageSchema {
  /// Returns the canonical immutable v1 schema reference.
  pub fn reference(self) -> Result<ImmutableReference, FactoryError> {
    let name = match self {
      Self::EligibilityInput => "triage.eligibility-input",
      Self::EligibilityResult => "triage.eligibility-result",
      Self::ClassificationInput => "triage.classification-input",
      Self::TriageResult => "triage.result",
      Self::Decision => "triage.decision",
    };
    Ok(ImmutableReference::new(
      FactoryKey::new(name)?,
      FactoryKey::new("v1")?,
      FactoryDigest::sha256("octacity.factory.triage-schema.v1", &[name.as_bytes()]),
    ))
  }
}

/// Composes replaceable observation subflows with separate code-owned eligibility and route gates.
///
/// Each supplied phase exports only `observed`. It cannot export a dispatch
/// outcome. Terminal eligibility edges complete the parent without entering
/// classification. Only explicit typed projections pass results to the gates.
pub fn compose_triage_flow(
  id: FlowDefinitionId,
  version: FlowDefinitionVersion,
  eligibility: &FlowDefinition,
  classification: &FlowDefinition,
  mut routes: Vec<TriageRoute>,
  execution: FlowExecutionPolicy,
) -> Result<FlowDefinition, FactoryError> {
  validate_phase(
    eligibility,
    TriageSchema::EligibilityInput,
    TriageSchema::EligibilityResult,
  )?;
  validate_phase(
    classification,
    TriageSchema::ClassificationInput,
    TriageSchema::TriageResult,
  )?;
  routes.sort();
  if routes.is_empty()
    || routes.windows(2).any(|pair| pair[0] == pair[1])
    || !routes.contains(&TriageRoute::Escalation)
    || !routes.contains(&TriageRoute::Rejection)
  {
    return Err(invalid("triage finite fallback routes"));
  }
  let outcome = |name: &str, schema: TriageSchema| -> Result<FlowOutcomeDefinition, FactoryError> {
    Ok(FlowOutcomeDefinition::new(
      FactoryKey::new(name)?,
      FlowOutcomeKind::Success,
      schema.reference()?,
    ))
  };
  let node = |name: &str, schema: TriageSchema, outcomes, subflow: Option<FlowDefinitionRef>| {
    FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: FactoryKey::new(name)?,
      kind: if subflow.is_some() {
        FlowNodeKind::SubflowCall
      } else {
        FlowNodeKind::DeterministicGate
      },
      input_schema: Some(schema.reference()?),
      outcomes,
      budget: execution.budget(),
      permissions: execution.permissions().clone(),
      required: true,
      subflow,
    })
  };
  let mut transitions = vec![
    transition(
      TriageNode::Eligibility.as_str(),
      "observed",
      FlowTransitionTarget::Node(FactoryKey::new(TriageNode::EligibilityPolicy.as_str())?),
    )?,
    transition(
      TriageNode::EligibilityPolicy.as_str(),
      "classify",
      FlowTransitionTarget::Node(FactoryKey::new(TriageNode::Classification.as_str())?),
    )?,
    transition(
      TriageNode::EligibilityPolicy.as_str(),
      "rejection",
      FlowTransitionTarget::Terminal(FactoryKey::new("rejection")?),
    )?,
    transition(
      TriageNode::EligibilityPolicy.as_str(),
      "escalation",
      FlowTransitionTarget::Terminal(FactoryKey::new("escalation")?),
    )?,
    transition(
      TriageNode::Classification.as_str(),
      "observed",
      FlowTransitionTarget::Node(FactoryKey::new(TriageNode::RoutingPolicy.as_str())?),
    )?,
  ];
  for route in &routes {
    transitions.push(transition(
      TriageNode::RoutingPolicy.as_str(),
      route.as_str(),
      FlowTransitionTarget::Terminal(FactoryKey::new(route.as_str())?),
    )?);
  }
  let data_projections = [
    (
      TriageNode::Eligibility.as_str(),
      "observed",
      TriageNode::EligibilityPolicy.as_str(),
      TriageSchema::EligibilityResult,
    ),
    (
      TriageNode::EligibilityPolicy.as_str(),
      "classify",
      TriageNode::Classification.as_str(),
      TriageSchema::ClassificationInput,
    ),
    (
      TriageNode::Classification.as_str(),
      "observed",
      TriageNode::RoutingPolicy.as_str(),
      TriageSchema::TriageResult,
    ),
  ]
  .into_iter()
  .map(|(source, selected, target, schema)| {
    Ok(FlowDataProjection::new(
      FactoryKey::new(source)?,
      FactoryKey::new(selected)?,
      FactoryKey::new(target)?,
      schema.reference()?,
    ))
  })
  .collect::<Result<Vec<_>, FactoryError>>()?;
  let terminals = routes
    .iter()
    .map(|route| {
      Ok(FlowTerminalDefinition::new(
        FactoryKey::new(route.as_str())?,
        TriageSchema::Decision.reference()?,
      ))
    })
    .collect::<Result<Vec<_>, FactoryError>>()?;
  FlowDefinition::new(FlowDefinitionInput {
    id,
    version,
    input_schema: Some(TriageSchema::EligibilityInput.reference()?),
    entry: FactoryKey::new(TriageNode::Eligibility.as_str())?,
    nodes: vec![
      node(
        TriageNode::Eligibility.as_str(),
        TriageSchema::EligibilityInput,
        vec![outcome("observed", TriageSchema::EligibilityResult)?],
        Some(eligibility.reference()),
      )?,
      node(
        TriageNode::EligibilityPolicy.as_str(),
        TriageSchema::EligibilityResult,
        vec![
          outcome("classify", TriageSchema::ClassificationInput)?,
          outcome("rejection", TriageSchema::Decision)?,
          outcome("escalation", TriageSchema::Decision)?,
        ],
        None,
      )?,
      node(
        TriageNode::Classification.as_str(),
        TriageSchema::ClassificationInput,
        vec![outcome("observed", TriageSchema::TriageResult)?],
        Some(classification.reference()),
      )?,
      node(
        TriageNode::RoutingPolicy.as_str(),
        TriageSchema::TriageResult,
        routes
          .iter()
          .map(|route| outcome(route.as_str(), TriageSchema::Decision))
          .collect::<Result<Vec<_>, _>>()?,
        None,
      )?,
    ],
    transitions,
    terminals,
    context_projections: vec![],
    data_projections,
    execution,
  })
}

fn transition(source: &str, outcome: &str, target: FlowTransitionTarget) -> Result<FlowTransition, FactoryError> {
  Ok(FlowTransition::new(
    FactoryKey::new(source)?,
    FactoryKey::new(outcome)?,
    target,
  ))
}

fn validate_phase(definition: &FlowDefinition, input: TriageSchema, output: TriageSchema) -> Result<(), FactoryError> {
  if definition.input_schema() != Some(&input.reference()?)
    || definition.terminals().len() != 1
    || definition.terminals()[0].key().as_str() != "observed"
    || definition.terminals()[0].schema() != &output.reference()?
  {
    return Err(invalid("triage phase schema contract"));
  }
  Ok(())
}

pub(super) fn validate_composition(root: &FlowDefinition) -> Result<(), FactoryError> {
  if root.nodes().len() != 4 || root.entry().as_str() != TriageNode::Eligibility.as_str() {
    return Err(invalid("triage composition"));
  }
  for (name, kind, schema) in [
    (
      TriageNode::Eligibility.as_str(),
      FlowNodeKind::SubflowCall,
      TriageSchema::EligibilityInput,
    ),
    (
      TriageNode::EligibilityPolicy.as_str(),
      FlowNodeKind::DeterministicGate,
      TriageSchema::EligibilityResult,
    ),
    (
      TriageNode::Classification.as_str(),
      FlowNodeKind::SubflowCall,
      TriageSchema::ClassificationInput,
    ),
    (
      TriageNode::RoutingPolicy.as_str(),
      FlowNodeKind::DeterministicGate,
      TriageSchema::TriageResult,
    ),
  ] {
    let expected_schema = schema.reference()?;
    if root
      .node(&FactoryKey::new(name)?)
      .is_none_or(|node| node.kind() != kind || node.input_schema() != Some(&expected_schema))
    {
      return Err(invalid("triage gate authority"));
    }
  }
  for transition in root.transitions() {
    let expected = match (
      TriageNode::from_key(transition.predecessor()),
      transition.outcome().as_str(),
    ) {
      (Some(TriageNode::Eligibility), "observed") => {
        FlowTransitionTarget::Node(FactoryKey::new(TriageNode::EligibilityPolicy.as_str())?)
      }
      (Some(TriageNode::EligibilityPolicy), "classify") => {
        FlowTransitionTarget::Node(FactoryKey::new(TriageNode::Classification.as_str())?)
      }
      (Some(TriageNode::EligibilityPolicy), outcome @ ("rejection" | "escalation")) => {
        FlowTransitionTarget::Terminal(FactoryKey::new(outcome)?)
      }
      (Some(TriageNode::Classification), "observed") => {
        FlowTransitionTarget::Node(FactoryKey::new(TriageNode::RoutingPolicy.as_str())?)
      }
      (Some(TriageNode::RoutingPolicy), outcome) if TriageRoute::ALL.iter().any(|route| route.as_str() == outcome) => {
        FlowTransitionTarget::Terminal(FactoryKey::new(outcome)?)
      }
      _ => return Err(invalid("triage declared control route")),
    };
    if transition.target() != &expected || transition.max_repeats() != 0 {
      return Err(invalid("triage terminal eligibility boundary"));
    }
    let schema = match (
      TriageNode::from_key(transition.predecessor()),
      transition.outcome().as_str(),
    ) {
      (Some(TriageNode::Eligibility), _) => TriageSchema::EligibilityResult,
      (Some(TriageNode::EligibilityPolicy), "classify") => TriageSchema::ClassificationInput,
      (Some(TriageNode::Classification), _) => TriageSchema::TriageResult,
      _ => TriageSchema::Decision,
    }
    .reference()?;
    if root
      .node(transition.predecessor())
      .and_then(|node| node.outcome(transition.outcome()))
      .is_none_or(|outcome| outcome.schema() != &schema)
    {
      return Err(invalid("triage result schema authority"));
    }
  }
  if ["rejection", "escalation"]
    .iter()
    .any(|name| root.terminals().iter().all(|terminal| terminal.key().as_str() != *name))
  {
    return Err(invalid("triage terminal fallback"));
  }
  Ok(())
}
