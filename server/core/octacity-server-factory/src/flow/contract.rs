use serde::{Deserialize, Serialize};

use crate::{
  BudgetLimit, FactoryDigest, FactoryError, FactoryKey, FactoryPermissionSet, ImmutableReference,
  MAX_FACTORY_ACTIVE_STAGES,
};

/// Maximum permitted nesting depth of one admitted Flow Definition closure.
///
/// Sixteen levels leave room for phase and reusable policy composition while
/// bounding validation stack depth and operator diagnostics.
pub const MAX_FLOW_NESTING_DEPTH: u16 = 16;
/// Maximum statically expanded node executions admitted for one Flow closure.
///
/// The ceiling keeps complete-closure validation and durable snapshots
/// bounded even when nested definitions reuse the same exact version.
pub const MAX_FLOW_EXPANDED_NODES: u32 = 16_384;
/// Maximum declared traversals of one repeat transition.
///
/// Repeats are explicit recovery paths, not an unbounded workflow loop.
pub const MAX_FLOW_REPEAT_COUNT: u16 = 32;
/// Maximum declared successors selected by one node outcome.
///
/// This caps both one-step dispatch fan-out and the number of durable outbox
/// items created by a single interpreter transition.
pub const MAX_FLOW_FAN_OUT: u16 = 64;
/// Maximum context selections in one explicit projection.
///
/// Context transfer remains an auditable allowlist rather than an accidental
/// replacement for bounded summaries and typed artifacts.
pub const MAX_FLOW_CONTEXT_SELECTIONS: usize = 64;

/// Whether one typed node outcome is successful or follows a failure route.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowOutcomeKind {
  /// The node produced a usable result.
  Success,
  /// The node produced a typed failure that must follow a declared route.
  Failure,
}

/// One finite schema-bound outcome declared by a Flow node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowOutcomeDefinition {
  key: FactoryKey,
  kind: FlowOutcomeKind,
  schema: ImmutableReference,
}

impl FlowOutcomeDefinition {
  /// Constructs one declared typed outcome.
  #[must_use]
  pub const fn new(key: FactoryKey, kind: FlowOutcomeKind, schema: ImmutableReference) -> Self {
    Self { key, kind, schema }
  }

  /// Returns the finite outcome key.
  #[must_use]
  pub const fn key(&self) -> &FactoryKey {
    &self.key
  }

  /// Returns whether this is a success or failure route.
  #[must_use]
  pub const fn kind(&self) -> FlowOutcomeKind {
    self.kind
  }

  /// Returns the exact result schema.
  #[must_use]
  pub const fn schema(&self) -> &ImmutableReference {
    &self.schema
  }
}

/// One finite terminal outcome exported by a Flow Definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowTerminalDefinition {
  key: FactoryKey,
  schema: ImmutableReference,
}

impl FlowTerminalDefinition {
  /// Constructs one schema-bound terminal outcome.
  #[must_use]
  pub const fn new(key: FactoryKey, schema: ImmutableReference) -> Self {
    Self { key, schema }
  }

  /// Returns the terminal outcome key.
  #[must_use]
  pub const fn key(&self) -> &FactoryKey {
    &self.key
  }

  /// Returns the exact terminal result schema.
  #[must_use]
  pub const fn schema(&self) -> &ImmutableReference {
    &self.schema
  }
}

/// Declared destination of one node outcome.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "key", deny_unknown_fields)]
pub enum FlowTransitionTarget {
  /// Continue at another node in the same Flow Definition.
  Node(FactoryKey),
  /// Complete the Flow Run with one declared terminal outcome.
  Terminal(FactoryKey),
}

/// One code-owned control transition selected only by a declared outcome.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowTransition {
  predecessor: FactoryKey,
  outcome: FactoryKey,
  target: FlowTransitionTarget,
  max_repeats: u16,
  exhausted_terminal: Option<FactoryKey>,
}

impl FlowTransition {
  /// Constructs an ordinary non-repeating transition.
  #[must_use]
  pub const fn new(predecessor: FactoryKey, outcome: FactoryKey, target: FlowTransitionTarget) -> Self {
    Self {
      predecessor,
      outcome,
      target,
      max_repeats: 0,
      exhausted_terminal: None,
    }
  }

  /// Constructs an explicitly bounded repeat transition.
  pub fn repeated(
    predecessor: FactoryKey,
    outcome: FactoryKey,
    target: FlowTransitionTarget,
    max_repeats: u16,
    exhausted_terminal: FactoryKey,
  ) -> Result<Self, FactoryError> {
    if max_repeats == 0 || max_repeats > MAX_FLOW_REPEAT_COUNT {
      return Err(FactoryError::InvalidConfiguration {
        field: "Flow transition repeat bound",
      });
    }
    Ok(Self {
      predecessor,
      outcome,
      target,
      max_repeats,
      exhausted_terminal: Some(exhausted_terminal),
    })
  }

  /// Returns the node whose typed outcome selects this transition.
  #[must_use]
  pub const fn predecessor(&self) -> &FactoryKey {
    &self.predecessor
  }

  /// Returns the finite declared outcome selector.
  #[must_use]
  pub const fn outcome(&self) -> &FactoryKey {
    &self.outcome
  }

  /// Returns the declared destination.
  #[must_use]
  pub const fn target(&self) -> &FlowTransitionTarget {
    &self.target
  }

  /// Returns zero for an ordinary edge or the hard repeat count.
  #[must_use]
  pub const fn max_repeats(&self) -> u16 {
    self.max_repeats
  }

  /// Returns the declared terminal route selected when the repeat bound is exhausted.
  #[must_use]
  pub const fn exhausted_terminal(&self) -> Option<&FactoryKey> {
    self.exhausted_terminal.as_ref()
  }
}

/// Explicit context-only visibility from one predecessor outcome to a successor.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowContextProjection {
  predecessor: FactoryKey,
  outcome: FactoryKey,
  successor: FactoryKey,
  selections: Vec<FactoryKey>,
}

impl FlowContextProjection {
  /// Constructs one bounded context projection without granting typed result data.
  pub fn new(
    predecessor: FactoryKey,
    outcome: FactoryKey,
    successor: FactoryKey,
    mut selections: Vec<FactoryKey>,
  ) -> Result<Self, FactoryError> {
    selections.sort();
    if selections.is_empty()
      || selections.len() > MAX_FLOW_CONTEXT_SELECTIONS
      || selections.windows(2).any(|pair| pair[0] == pair[1])
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "Flow context projection",
      });
    }
    Ok(Self {
      predecessor,
      outcome,
      successor,
      selections,
    })
  }

  /// Returns the source node.
  #[must_use]
  pub const fn predecessor(&self) -> &FactoryKey {
    &self.predecessor
  }

  /// Returns the source outcome.
  #[must_use]
  pub const fn outcome(&self) -> &FactoryKey {
    &self.outcome
  }

  /// Returns the context consumer.
  #[must_use]
  pub const fn successor(&self) -> &FactoryKey {
    &self.successor
  }

  /// Returns the explicit bounded context selections.
  #[must_use]
  pub fn selections(&self) -> &[FactoryKey] {
    &self.selections
  }
}

/// Explicit typed data transfer from one predecessor outcome to a successor.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowDataProjection {
  predecessor: FactoryKey,
  outcome: FactoryKey,
  successor: FactoryKey,
  schema: ImmutableReference,
}

impl FlowDataProjection {
  /// Constructs one exact schema-bound data projection.
  #[must_use]
  pub const fn new(
    predecessor: FactoryKey,
    outcome: FactoryKey,
    successor: FactoryKey,
    schema: ImmutableReference,
  ) -> Self {
    Self {
      predecessor,
      outcome,
      successor,
      schema,
    }
  }

  /// Returns the source node.
  #[must_use]
  pub const fn predecessor(&self) -> &FactoryKey {
    &self.predecessor
  }

  /// Returns the source outcome.
  #[must_use]
  pub const fn outcome(&self) -> &FactoryKey {
    &self.outcome
  }

  /// Returns the typed data consumer.
  #[must_use]
  pub const fn successor(&self) -> &FactoryKey {
    &self.successor
  }

  /// Returns the exact transferred schema.
  #[must_use]
  pub const fn schema(&self) -> &ImmutableReference {
    &self.schema
  }
}

/// Definition-local execution ceilings enforced above the structural graph module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowExecutionPolicy {
  budget: BudgetLimit,
  permissions: FactoryPermissionSet,
  max_active_nodes: u16,
}

impl FlowExecutionPolicy {
  /// Constructs finite budget, authority, and WIP ceilings.
  pub fn new(
    budget: BudgetLimit,
    permissions: FactoryPermissionSet,
    max_active_nodes: u16,
  ) -> Result<Self, FactoryError> {
    if max_active_nodes == 0 || u32::from(max_active_nodes) > MAX_FACTORY_ACTIVE_STAGES {
      return Err(FactoryError::InvalidConfiguration {
        field: "Flow WIP bound",
      });
    }
    Ok(Self {
      budget,
      permissions,
      max_active_nodes,
    })
  }

  /// Returns the enclosing hard budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the deny-by-default permission ceiling.
  #[must_use]
  pub const fn permissions(&self) -> &FactoryPermissionSet {
    &self.permissions
  }

  /// Returns the maximum concurrently active nodes.
  #[must_use]
  pub const fn max_active_nodes(&self) -> u16 {
    self.max_active_nodes
  }
}

/// Admission-wide safety ceilings applied to an exact pinned closure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowAdmissionLimits {
  max_depth: u16,
  max_expanded_nodes: u32,
  max_repeat_count: u16,
  max_fan_out: u16,
  max_wip: u16,
  budget: BudgetLimit,
  permissions: FactoryPermissionSet,
}

impl FlowAdmissionLimits {
  /// Constructs bounded admission ceilings within product safety limits.
  pub fn new(
    max_depth: u16,
    max_expanded_nodes: u32,
    max_repeat_count: u16,
    max_fan_out: u16,
    max_wip: u16,
    budget: BudgetLimit,
    permissions: FactoryPermissionSet,
  ) -> Result<Self, FactoryError> {
    if max_depth == 0
      || max_depth > MAX_FLOW_NESTING_DEPTH
      || max_expanded_nodes == 0
      || max_expanded_nodes > MAX_FLOW_EXPANDED_NODES
      || max_repeat_count == 0
      || max_repeat_count > MAX_FLOW_REPEAT_COUNT
      || max_fan_out == 0
      || max_fan_out > MAX_FLOW_FAN_OUT
      || max_wip == 0
      || u32::from(max_wip) > MAX_FACTORY_ACTIVE_STAGES
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "Flow admission limits",
      });
    }
    Ok(Self {
      max_depth,
      max_expanded_nodes,
      max_repeat_count,
      max_fan_out,
      max_wip,
      budget,
      permissions,
    })
  }

  /// Returns conservative product safety ceilings with caller-owned authority.
  pub fn product_defaults(
    max_wip: u16,
    budget: BudgetLimit,
    permissions: FactoryPermissionSet,
  ) -> Result<Self, FactoryError> {
    Self::new(
      MAX_FLOW_NESTING_DEPTH,
      MAX_FLOW_EXPANDED_NODES,
      MAX_FLOW_REPEAT_COUNT,
      MAX_FLOW_FAN_OUT,
      max_wip,
      budget,
      permissions,
    )
  }

  /// Returns the maximum nested subflow depth including the root.
  #[must_use]
  pub const fn max_depth(&self) -> u16 {
    self.max_depth
  }

  /// Returns the maximum statically expanded node executions.
  #[must_use]
  pub const fn max_expanded_nodes(&self) -> u32 {
    self.max_expanded_nodes
  }

  /// Returns the maximum repeat bound on any transition.
  #[must_use]
  pub const fn max_repeat_count(&self) -> u16 {
    self.max_repeat_count
  }

  /// Returns the maximum successors selected by one outcome.
  #[must_use]
  pub const fn max_fan_out(&self) -> u16 {
    self.max_fan_out
  }

  /// Returns the admission-wide active-node ceiling.
  #[must_use]
  pub const fn max_wip(&self) -> u16 {
    self.max_wip
  }

  /// Returns the aggregate hard budget for the admitted closure.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the deny-by-default permission ceiling for the closure.
  #[must_use]
  pub const fn permissions(&self) -> &FactoryPermissionSet {
    &self.permissions
  }
}

pub(crate) fn internal_schema(identity: &str) -> Result<ImmutableReference, FactoryError> {
  let identity = FactoryKey::new(identity)?;
  let version = FactoryKey::new("v1")?;
  let digest = FactoryDigest::sha256("octacity.factory.flow-schema.v1", &[identity.as_str().as_bytes()]);
  Ok(ImmutableReference::new(identity, version, digest))
}
