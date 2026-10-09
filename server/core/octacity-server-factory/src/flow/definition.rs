use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
  BudgetLimit, FactoryConfiguration, FactoryDigest, FactoryError, FactoryFlowEdge, FactoryKey, FactoryPermissionSet,
  FactoryStageKind, FlowAdmissionLimits, FlowContextProjection, FlowDataProjection, FlowDefinitionId,
  FlowDefinitionVersion, FlowExecutionPolicy, FlowNodeKind, FlowOutcomeDefinition, FlowOutcomeKind,
  FlowTerminalDefinition, FlowTransition, FlowTransitionTarget, ImmutableReference, analyze_factory_flow_graph,
};

/// Maximum nodes stored in one immutable Flow Definition.
pub const MAX_FLOW_DEFINITION_NODES: usize = 256;
/// Maximum control transitions stored in one immutable Flow Definition.
pub const MAX_FLOW_DEFINITION_EDGES: usize = 1_024;
/// Maximum immutable definitions pinned by one admitted root closure.
pub const MAX_FLOW_DEFINITION_CLOSURE: usize = 64;

/// Exact immutable identity of one Flow Definition version.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowDefinitionRef {
  id: FlowDefinitionId,
  version: FlowDefinitionVersion,
  digest: FactoryDigest,
}

impl FlowDefinitionRef {
  /// Constructs an exact content-bound definition reference.
  #[must_use]
  pub const fn new(id: FlowDefinitionId, version: FlowDefinitionVersion, digest: FactoryDigest) -> Self {
    Self { id, version, digest }
  }

  /// Returns the stable definition identity.
  #[must_use]
  pub const fn id(self) -> FlowDefinitionId {
    self.id
  }
  /// Returns the immutable definition version.
  #[must_use]
  pub const fn version(self) -> FlowDefinitionVersion {
    self.version
  }
  /// Returns the canonical content digest.
  #[must_use]
  pub const fn digest(self) -> FactoryDigest {
    self.digest
  }
}

/// Caller-owned fields for one immutable Flow node declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowNodeDefinitionInput {
  /// Stable node key within the definition.
  pub key: FactoryKey,
  /// Closed provider-neutral primitive kind.
  pub kind: FlowNodeKind,
  /// Exact input schema, or `None` for a control-only node.
  pub input_schema: Option<ImmutableReference>,
  /// Finite typed outcomes the node may publish.
  pub outcomes: Vec<FlowOutcomeDefinition>,
  /// Hard budget enclosing one attempt.
  pub budget: BudgetLimit,
  /// Permission ceiling applied to this node.
  pub permissions: FactoryPermissionSet,
  /// Whether admission requires this node to be reachable.
  pub required: bool,
  /// Exact nested definition for `subflow_call`.
  pub subflow: Option<FlowDefinitionRef>,
}

/// One immutable node declaration from the closed Factory primitive set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowNodeDefinition {
  key: FactoryKey,
  kind: FlowNodeKind,
  input_schema: Option<ImmutableReference>,
  outcomes: Vec<FlowOutcomeDefinition>,
  budget: BudgetLimit,
  permissions: FactoryPermissionSet,
  required: bool,
  subflow: Option<FlowDefinitionRef>,
  stage_projection: Option<FactoryStageKind>,
}

impl FlowNodeDefinition {
  /// Constructs a bounded typed primitive or exact subflow-call node.
  pub fn new(mut input: FlowNodeDefinitionInput) -> Result<Self, FactoryError> {
    input.outcomes.sort_by(|left, right| left.key().cmp(right.key()));
    if input.outcomes.is_empty() || input.outcomes.windows(2).any(|pair| pair[0].key() == pair[1].key()) {
      return Err(FactoryError::InvalidConfiguration {
        field: "Flow node outcomes",
      });
    }
    if (input.kind == FlowNodeKind::SubflowCall) != input.subflow.is_some() {
      return Err(FactoryError::InvalidReference {
        relationship: "subflow definition",
      });
    }
    Ok(Self {
      key: input.key,
      kind: input.kind,
      input_schema: input.input_schema,
      outcomes: input.outcomes,
      budget: input.budget,
      permissions: input.permissions,
      required: input.required,
      subflow: input.subflow,
      stage_projection: None,
    })
  }

  /// Returns the stable node key within its definition.
  #[must_use]
  pub const fn key(&self) -> &FactoryKey {
    &self.key
  }
  /// Returns the closed primitive kind.
  #[must_use]
  pub const fn kind(&self) -> FlowNodeKind {
    self.kind
  }
  /// Returns the exact input schema, if the node consumes typed data.
  #[must_use]
  pub const fn input_schema(&self) -> Option<&ImmutableReference> {
    self.input_schema.as_ref()
  }
  /// Returns the finite typed outcomes in canonical order.
  #[must_use]
  pub fn outcomes(&self) -> &[FlowOutcomeDefinition] {
    &self.outcomes
  }
  /// Finds one declared typed outcome.
  #[must_use]
  pub fn outcome(&self, key: &FactoryKey) -> Option<&FlowOutcomeDefinition> {
    self.outcomes.iter().find(|outcome| outcome.key() == key)
  }
  /// Returns the hard budget for one attempt.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }
  /// Returns the node permission ceiling.
  #[must_use]
  pub const fn permissions(&self) -> &FactoryPermissionSet {
    &self.permissions
  }
  /// Reports whether this node must be reachable before publication.
  #[must_use]
  pub const fn is_required(&self) -> bool {
    self.required
  }
  /// Returns the exact nested definition selected by a subflow call.
  #[must_use]
  pub const fn subflow_definition(&self) -> Option<FlowDefinitionRef> {
    self.subflow
  }
  /// Returns the fixed-stage kind retained by a pre-graph definition projection.
  #[must_use]
  pub const fn stage_projection(&self) -> Option<FactoryStageKind> {
    self.stage_projection
  }

  fn projected_stage(
    key: FactoryKey,
    kind: FactoryStageKind,
    budget: BudgetLimit,
    outcomes: Vec<FlowOutcomeDefinition>,
  ) -> Self {
    Self {
      key,
      kind: FlowNodeKind::BuildCommand,
      input_schema: None,
      outcomes,
      budget,
      permissions: FactoryPermissionSet::deny_all(),
      required: true,
      subflow: None,
      stage_projection: Some(kind),
    }
  }
}

/// Caller-owned complete immutable Flow Definition content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowDefinitionInput {
  /// Stable definition identity.
  pub id: FlowDefinitionId,
  /// Immutable definition version.
  pub version: FlowDefinitionVersion,
  /// Exact input schema, or `None` for a control-only root.
  pub input_schema: Option<ImmutableReference>,
  /// Entry node key.
  pub entry: FactoryKey,
  /// Closed typed node declarations.
  pub nodes: Vec<FlowNodeDefinition>,
  /// Outcome-selected control transitions.
  pub transitions: Vec<FlowTransition>,
  /// Finite terminal outcomes.
  pub terminals: Vec<FlowTerminalDefinition>,
  /// Explicit context-only visibility projections.
  pub context_projections: Vec<FlowContextProjection>,
  /// Explicit typed data projections.
  pub data_projections: Vec<FlowDataProjection>,
  /// Definition-local budget, permission, and WIP ceilings.
  pub execution: FlowExecutionPolicy,
}

#[derive(Serialize)]
struct FlowDefinitionContent<'a> {
  id: FlowDefinitionId,
  version: FlowDefinitionVersion,
  input_schema: &'a Option<ImmutableReference>,
  entry: &'a FactoryKey,
  nodes: &'a [FlowNodeDefinition],
  transitions: &'a [FlowTransition],
  terminals: &'a [FlowTerminalDefinition],
  context_projections: &'a [FlowContextProjection],
  data_projections: &'a [FlowDataProjection],
  execution: &'a FlowExecutionPolicy,
}

/// Canonically ordered immutable semantics of one Factory Flow version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowDefinition {
  reference: FlowDefinitionRef,
  input_schema: Option<ImmutableReference>,
  entry: FactoryKey,
  nodes: Vec<FlowNodeDefinition>,
  transitions: Vec<FlowTransition>,
  terminals: Vec<FlowTerminalDefinition>,
  context_projections: Vec<FlowContextProjection>,
  data_projections: Vec<FlowDataProjection>,
  execution: FlowExecutionPolicy,
}

impl FlowDefinition {
  /// Canonicalizes and structurally validates one immutable definition.
  pub fn new(mut input: FlowDefinitionInput) -> Result<Self, FactoryError> {
    validate_size(input.nodes.len(), input.transitions.len())?;
    input.nodes.sort_by(|left, right| left.key.cmp(&right.key));
    input.transitions.sort();
    input.terminals.sort_by(|left, right| left.key().cmp(right.key()));
    input.context_projections.sort();
    input.data_projections.sort();
    if input
      .nodes
      .iter()
      .filter_map(FlowNodeDefinition::subflow_definition)
      .any(|nested| nested.id() == input.id && nested.version() == input.version)
    {
      return Err(FactoryError::InvalidReference {
        relationship: "recursive Flow Definition",
      });
    }
    let edges = input
      .transitions
      .iter()
      .filter_map(|transition| match transition.target() {
        FlowTransitionTarget::Node(successor) => Some(FactoryFlowEdge::new(
          transition.predecessor().clone(),
          successor.clone(),
        )),
        FlowTransitionTarget::Terminal(_) => None,
      })
      .collect::<BTreeSet<_>>()
      .into_iter()
      .collect::<Vec<_>>();
    analyze_factory_flow_graph(
      input.nodes.iter().map(|node| node.key.clone()),
      edges.iter().cloned(),
      [input.entry.clone()],
    )
    .map_err(|_| FactoryError::InvalidReference {
      relationship: "Flow Definition graph",
    })?;
    let content = FlowDefinitionContent {
      id: input.id,
      version: input.version,
      input_schema: &input.input_schema,
      entry: &input.entry,
      nodes: &input.nodes,
      transitions: &input.transitions,
      terminals: &input.terminals,
      context_projections: &input.context_projections,
      data_projections: &input.data_projections,
      execution: &input.execution,
    };
    let digest = definition_digest(&content)?;
    Ok(Self {
      reference: FlowDefinitionRef::new(input.id, input.version, digest),
      input_schema: input.input_schema,
      entry: input.entry,
      nodes: input.nodes,
      transitions: input.transitions,
      terminals: input.terminals,
      context_projections: input.context_projections,
      data_projections: input.data_projections,
      execution: input.execution,
    })
  }

  /// Projects the fixed-stage configuration into one fully typed root definition.
  pub fn from_stage_projection(configuration: &FactoryConfiguration) -> Result<Self, FactoryError> {
    let reference = configuration.reference();
    let id = FlowDefinitionId::from_uuid(reference.id().as_uuid())?;
    let version = FlowDefinitionVersion::new(reference.version().get())?;
    let result_schema = super::contract::internal_schema("factory.stage-result")?;
    let outcome = |name: &str, kind| {
      Ok(FlowOutcomeDefinition::new(
        FactoryKey::new(name)?,
        kind,
        result_schema.clone(),
      ))
    };
    let mut keys = BTreeMap::new();
    let nodes = configuration
      .stages()
      .iter()
      .map(|stage| {
        keys.insert(stage.kind(), stage.key().clone());
        Ok(FlowNodeDefinition::projected_stage(
          stage.key().clone(),
          stage.kind(),
          stage.budget(),
          vec![
            outcome("succeeded", FlowOutcomeKind::Success)?,
            outcome("failed", FlowOutcomeKind::Failure)?,
            outcome("cancelled", FlowOutcomeKind::Failure)?,
          ],
        ))
      })
      .collect::<Result<Vec<_>, FactoryError>>()?;
    let entry = keys
      .get(&FactoryStageKind::Implementation)
      .cloned()
      .ok_or(FactoryError::InvalidReference {
        relationship: "implementation stage",
      })?;
    let succeeded = FactoryKey::new("succeeded")?;
    let failed = FactoryKey::new("failed")?;
    let cancelled = FactoryKey::new("cancelled")?;
    let mut transitions = Vec::new();
    connect(
      &keys,
      FactoryStageKind::Implementation,
      &succeeded,
      FactoryStageKind::Validation,
      &mut transitions,
    );
    connect(
      &keys,
      FactoryStageKind::Validation,
      &succeeded,
      FactoryStageKind::Evaluation,
      &mut transitions,
    );
    connect(
      &keys,
      FactoryStageKind::Validation,
      &failed,
      FactoryStageKind::Rework,
      &mut transitions,
    );
    connect(
      &keys,
      FactoryStageKind::Evaluation,
      &failed,
      FactoryStageKind::Rework,
      &mut transitions,
    );
    if let (Some(rework), Some(validation)) = (
      keys.get(&FactoryStageKind::Rework),
      keys.get(&FactoryStageKind::Validation),
    ) {
      let max_cycles = configuration.rework().max_cycles();
      if max_cycles == 0 {
        return Err(FactoryError::InvalidConfiguration {
          field: "rework stage without cycles",
        });
      }
      transitions.push(FlowTransition::repeated(
        rework.clone(),
        succeeded.clone(),
        FlowTransitionTarget::Node(validation.clone()),
        max_cycles,
        failed.clone(),
      )?);
    }
    let terminal = |predecessor: &FactoryKey, outcome: &FactoryKey, target: &str| -> Result<_, FactoryError> {
      Ok(FlowTransition::new(
        predecessor.clone(),
        outcome.clone(),
        FlowTransitionTarget::Terminal(FactoryKey::new(target)?),
      ))
    };
    for node in &nodes {
      let has_route = |key: &FactoryKey| {
        transitions
          .iter()
          .any(|transition| transition.predecessor() == node.key() && transition.outcome() == key)
      };
      if !has_route(&failed) {
        transitions.push(terminal(node.key(), &failed, "failed")?);
      }
      transitions.push(terminal(node.key(), &cancelled, "cancelled")?);
    }
    if let Some(evaluation) = keys.get(&FactoryStageKind::Evaluation) {
      transitions.push(terminal(evaluation, &succeeded, "succeeded")?);
    }
    let max_active_nodes = u16::try_from(configuration.wip_limits().max_active_stages()).map_err(|_| {
      FactoryError::InvalidConfiguration {
        field: "Flow WIP bound",
      }
    })?;
    Self::new(FlowDefinitionInput {
      id,
      version,
      input_schema: None,
      entry,
      nodes,
      transitions,
      terminals: ["cancelled", "failed", "succeeded"]
        .into_iter()
        .map(|name| {
          Ok(FlowTerminalDefinition::new(
            FactoryKey::new(name)?,
            result_schema.clone(),
          ))
        })
        .collect::<Result<_, FactoryError>>()?,
      context_projections: Vec::new(),
      data_projections: Vec::new(),
      execution: FlowExecutionPolicy::new(
        configuration.hard_budget(),
        FactoryPermissionSet::deny_all(),
        max_active_nodes,
      )?,
    })
  }

  /// Returns the exact immutable reference.
  #[must_use]
  pub const fn reference(&self) -> FlowDefinitionRef {
    self.reference
  }
  /// Returns the exact Flow input schema, when any.
  #[must_use]
  pub const fn input_schema(&self) -> Option<&ImmutableReference> {
    self.input_schema.as_ref()
  }
  /// Returns the entry node key.
  #[must_use]
  pub const fn entry(&self) -> &FactoryKey {
    &self.entry
  }
  /// Returns canonical nodes ordered by key.
  #[must_use]
  pub fn nodes(&self) -> &[FlowNodeDefinition] {
    &self.nodes
  }
  /// Finds one declared node by its stable key.
  #[must_use]
  pub fn node(&self, key: &FactoryKey) -> Option<&FlowNodeDefinition> {
    self.nodes.iter().find(|node| node.key() == key)
  }
  /// Returns outcome-selected control transitions.
  #[must_use]
  pub fn transitions(&self) -> &[FlowTransition] {
    &self.transitions
  }
  /// Returns finite terminal outcomes.
  #[must_use]
  pub fn terminals(&self) -> &[FlowTerminalDefinition] {
    &self.terminals
  }
  /// Returns explicit context-only projections.
  #[must_use]
  pub fn context_projections(&self) -> &[FlowContextProjection] {
    &self.context_projections
  }
  /// Returns explicit typed data projections.
  #[must_use]
  pub fn data_projections(&self) -> &[FlowDataProjection] {
    &self.data_projections
  }
  /// Returns definition-local execution ceilings.
  #[must_use]
  pub const fn execution(&self) -> &FlowExecutionPolicy {
    &self.execution
  }

  pub(crate) fn validate_canonical_form(&self) -> Result<(), FactoryError> {
    let content = FlowDefinitionContent {
      id: self.reference.id(),
      version: self.reference.version(),
      input_schema: &self.input_schema,
      entry: &self.entry,
      nodes: &self.nodes,
      transitions: &self.transitions,
      terminals: &self.terminals,
      context_projections: &self.context_projections,
      data_projections: &self.data_projections,
      execution: &self.execution,
    };
    let canonical = self.nodes.windows(2).all(|pair| pair[0].key() < pair[1].key())
      && self.transitions.windows(2).all(|pair| pair[0] < pair[1])
      && self.terminals.windows(2).all(|pair| pair[0].key() < pair[1].key())
      && self.context_projections.windows(2).all(|pair| pair[0] < pair[1])
      && self.data_projections.windows(2).all(|pair| pair[0] < pair[1]);
    if !canonical || self.reference.digest() != definition_digest(&content)? {
      return Err(FactoryError::InvalidReference {
        relationship: "canonical Flow Definition",
      });
    }
    Ok(())
  }

  pub(crate) fn structural_edges(&self) -> Vec<FactoryFlowEdge> {
    self
      .transitions
      .iter()
      .filter_map(|transition| match transition.target() {
        FlowTransitionTarget::Node(successor) => Some(FactoryFlowEdge::new(
          transition.predecessor().clone(),
          successor.clone(),
        )),
        FlowTransitionTarget::Terminal(_) => None,
      })
      .collect::<BTreeSet<_>>()
      .into_iter()
      .collect()
  }
}

fn definition_digest(content: &FlowDefinitionContent<'_>) -> Result<FactoryDigest, FactoryError> {
  let bytes = serde_json::to_vec(content).map_err(|_| FactoryError::InvalidReference {
    relationship: "Flow Definition serialization",
  })?;
  Ok(FactoryDigest::sha256("octacity.factory.flow-definition.v2", &[&bytes]))
}

fn validate_size(nodes: usize, transitions: usize) -> Result<(), FactoryError> {
  if nodes == 0 || nodes > MAX_FLOW_DEFINITION_NODES {
    return Err(FactoryError::CollectionLimitExceeded {
      collection: "Flow Definition nodes",
    });
  }
  if transitions > MAX_FLOW_DEFINITION_EDGES {
    return Err(FactoryError::CollectionLimitExceeded {
      collection: "Flow Definition transitions",
    });
  }
  Ok(())
}

fn connect(
  keys: &BTreeMap<FactoryStageKind, FactoryKey>,
  predecessor: FactoryStageKind,
  outcome: &FactoryKey,
  successor: FactoryStageKind,
  transitions: &mut Vec<FlowTransition>,
) {
  if let (Some(predecessor), Some(successor)) = (keys.get(&predecessor), keys.get(&successor)) {
    transitions.push(FlowTransition::new(
      predecessor.clone(),
      outcome.clone(),
      FlowTransitionTarget::Node(successor.clone()),
    ));
  }
}

/// Exact reachable immutable Flow Definition closure pinned at admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedFlowDefinitionClosure {
  root: FlowDefinitionRef,
  definitions: Vec<FlowDefinition>,
}

impl PinnedFlowDefinitionClosure {
  /// Pins exactly the definitions reachable through exact subflow references.
  pub fn new(root: FlowDefinitionRef, mut definitions: Vec<FlowDefinition>) -> Result<Self, FactoryError> {
    if definitions.is_empty() || definitions.len() > MAX_FLOW_DEFINITION_CLOSURE {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "Flow Definition closure",
      });
    }
    definitions.sort_by_key(FlowDefinition::reference);
    let available = definitions
      .iter()
      .map(FlowDefinition::reference)
      .collect::<BTreeSet<_>>();
    if available.len() != definitions.len() || !available.contains(&root) {
      return Err(invalid_closure());
    }
    let mut reachable = BTreeSet::from([root]);
    let mut pending = vec![root];
    while let Some(reference) = pending.pop() {
      let definition = definitions
        .iter()
        .find(|definition| definition.reference == reference)
        .ok_or_else(invalid_closure)?;
      for nested in definition
        .nodes
        .iter()
        .filter_map(FlowNodeDefinition::subflow_definition)
      {
        if !available.contains(&nested) {
          return Err(invalid_closure());
        }
        if reachable.insert(nested) {
          pending.push(nested);
        }
      }
    }
    if reachable != available {
      return Err(invalid_closure());
    }
    Ok(Self { root, definitions })
  }

  /// Creates the one-definition closure for the fixed-stage runtime.
  pub fn from_stage_projection(configuration: &FactoryConfiguration) -> Result<Self, FactoryError> {
    let definition = FlowDefinition::from_stage_projection(configuration)?;
    Self::new(definition.reference(), vec![definition])
  }
  /// Returns the exact root definition.
  #[must_use]
  pub const fn root(&self) -> FlowDefinitionRef {
    self.root
  }
  /// Returns every reachable exact definition in canonical order.
  #[must_use]
  pub fn definitions(&self) -> &[FlowDefinition] {
    &self.definitions
  }
  /// Finds one exact definition in the pinned reachable closure.
  #[must_use]
  pub fn definition(&self, reference: FlowDefinitionRef) -> Option<&FlowDefinition> {
    self
      .definitions
      .iter()
      .find(|definition| definition.reference() == reference)
  }
  /// Validates all typed and bounded semantics before admission.
  pub fn validate(self, limits: FlowAdmissionLimits) -> Result<crate::ValidatedFlowDefinitionClosure, FactoryError> {
    crate::ValidatedFlowDefinitionClosure::new(self, limits)
  }
}

fn invalid_closure() -> FactoryError {
  FactoryError::InvalidReference {
    relationship: "reachable Flow Definition closure",
  }
}
