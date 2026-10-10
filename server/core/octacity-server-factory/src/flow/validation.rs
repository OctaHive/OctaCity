use std::collections::{BTreeMap, BTreeSet};

use crate::{
  FactoryError, FactoryKey, FlowAdmissionLimits, FlowDefinition, FlowDefinitionRef, FlowNodeKind,
  FlowOutcomeDefinition, FlowTerminalDefinition, FlowTransitionTarget, PinnedFlowDefinitionClosure,
  analyze_factory_flow_graph,
};

/// Fully validated immutable closure admitted for deterministic interpretation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedFlowDefinitionClosure {
  closure: PinnedFlowDefinitionClosure,
  limits: FlowAdmissionLimits,
  expanded_nodes: u32,
  max_depth: u16,
}

impl ValidatedFlowDefinitionClosure {
  /// Validates typed semantics, exact nesting, authority, and aggregate bounds.
  pub(crate) fn new(closure: PinnedFlowDefinitionClosure, limits: FlowAdmissionLimits) -> Result<Self, FactoryError> {
    let definitions = closure
      .definitions()
      .iter()
      .map(|definition| (definition.reference(), definition))
      .collect::<BTreeMap<_, _>>();
    for definition in closure.definitions() {
      validate_definition(definition, &definitions, &limits)?;
    }
    reject_recursive_definitions(closure.root(), &definitions, &mut BTreeSet::new(), &mut BTreeSet::new())?;
    let (expanded_nodes, max_depth) = expanded_measurements(closure.root(), &definitions, &mut BTreeMap::new())?;
    if expanded_nodes > limits.max_expanded_nodes() || max_depth > limits.max_depth() {
      return Err(invalid("Flow closure bounds"));
    }
    let root = definitions
      .get(&closure.root())
      .ok_or_else(|| invalid("Flow closure root"))?;
    if !root.execution().budget().fits_within(limits.budget())
      || !root.execution().permissions().is_no_broader_than(limits.permissions())
      || root.execution().max_active_nodes() > limits.max_wip()
    {
      return Err(invalid("Flow admission authority"));
    }
    Ok(Self {
      closure,
      limits,
      expanded_nodes,
      max_depth,
    })
  }

  /// Returns the exact pinned closure.
  #[must_use]
  pub const fn closure(&self) -> &PinnedFlowDefinitionClosure {
    &self.closure
  }

  /// Returns admission-wide ceilings retained for restart validation.
  #[must_use]
  pub const fn limits(&self) -> &FlowAdmissionLimits {
    &self.limits
  }

  /// Returns the conservative statically expanded node bound.
  #[must_use]
  pub const fn expanded_nodes(&self) -> u32 {
    self.expanded_nodes
  }

  /// Returns the maximum exact subflow nesting depth including the root.
  #[must_use]
  pub const fn max_depth(&self) -> u16 {
    self.max_depth
  }
}

fn validate_definition(
  definition: &FlowDefinition,
  definitions: &BTreeMap<FlowDefinitionRef, &FlowDefinition>,
  limits: &FlowAdmissionLimits,
) -> Result<(), FactoryError> {
  definition.validate_canonical_form()?;
  if definition.terminals().is_empty()
    || definition.execution().max_active_nodes() > limits.max_wip()
    || !definition.execution().budget().fits_within(limits.budget())
    || !definition
      .execution()
      .permissions()
      .is_no_broader_than(limits.permissions())
  {
    return Err(invalid("Flow definition bounds"));
  }
  if definition.node(definition.entry()).and_then(|node| node.input_schema()) != definition.input_schema() {
    return Err(invalid("Flow entry input schema"));
  }
  let facts = analyze_factory_flow_graph(
    definition.nodes().iter().map(|node| node.key().clone()),
    definition.structural_edges(),
    [definition.entry().clone()],
  )
  .map_err(|_| invalid("Flow control graph"))?;
  if facts.measurements().max_fan_out() > usize::from(limits.max_fan_out())
    || definition
      .nodes()
      .iter()
      .any(|node| node.is_required() && !facts.reachable().contains(node.key()))
  {
    return Err(invalid("Flow reachability or fan-out"));
  }

  let terminals = unique_terminals(definition.terminals())?;
  let mut used_terminals = BTreeSet::new();
  let cyclic_components = facts
    .cyclic_components()
    .iter()
    .enumerate()
    .flat_map(|(index, component)| component.iter().map(move |node| (node, index)))
    .collect::<BTreeMap<_, _>>();
  let mut routes = BTreeMap::<(&FactoryKey, &FactoryKey), usize>::new();
  for transition in definition.transitions() {
    let predecessor = definition
      .node(transition.predecessor())
      .ok_or_else(|| invalid("Flow transition predecessor"))?;
    let outcome = predecessor
      .outcome(transition.outcome())
      .ok_or_else(|| invalid("Flow transition outcome"))?;
    *routes
      .entry((transition.predecessor(), transition.outcome()))
      .or_default() += 1;
    match transition.target() {
      FlowTransitionTarget::Node(successor) => {
        definition
          .node(successor)
          .ok_or_else(|| invalid("Flow transition successor"))?;
        let cyclic = cyclic_components
          .get(transition.predecessor())
          .is_some_and(|component| cyclic_components.get(successor) == Some(component));
        if (transition.max_repeats() > 0 && !cyclic) || transition.max_repeats() > limits.max_repeat_count() {
          return Err(invalid("Flow repeat bound"));
        }
      }
      FlowTransitionTarget::Terminal(terminal) => {
        if terminals
          .get(terminal)
          .is_none_or(|terminal| terminal.schema() != outcome.schema())
          || transition.max_repeats() != 0
        {
          return Err(invalid("Flow terminal route"));
        }
        used_terminals.insert(terminal);
      }
    }
    match (transition.max_repeats(), transition.exhausted_terminal()) {
      (0, None) => {}
      (0, Some(_)) | (_, None) => return Err(invalid("Flow repeat exhaustion route")),
      (_, Some(terminal)) => {
        if terminals
          .get(terminal)
          .is_none_or(|terminal| terminal.schema() != outcome.schema())
        {
          return Err(invalid("Flow repeat exhaustion route"));
        }
        used_terminals.insert(terminal);
      }
    }
  }
  if definition.nodes().iter().any(|node| {
    node.outcomes().iter().any(|outcome| {
      let count = routes.get(&(node.key(), outcome.key())).copied().unwrap_or_default();
      count == 0 || (node.kind() != FlowNodeKind::FanOut && count != 1) || count > usize::from(limits.max_fan_out())
    })
  }) || used_terminals.len() != terminals.len()
  {
    return Err(invalid("Flow outcome routes"));
  }
  let unbounded = analyze_factory_flow_graph(
    definition.nodes().iter().map(|node| node.key().clone()),
    definition
      .transitions()
      .iter()
      .filter_map(|transition| match (transition.max_repeats(), transition.target()) {
        (0, FlowTransitionTarget::Node(successor)) => Some(crate::FactoryFlowEdge::new(
          transition.predecessor().clone(),
          successor.clone(),
        )),
        (_, FlowTransitionTarget::Node(_) | FlowTransitionTarget::Terminal(_)) => None,
      })
      .collect::<std::collections::BTreeSet<_>>(),
    [definition.entry().clone()],
  )
  .map_err(|_| invalid("Flow unbounded control graph"))?;
  if !unbounded.cyclic_components().is_empty() {
    return Err(invalid("Flow repeat bound"));
  }
  let mut terminating = definition
    .transitions()
    .iter()
    .filter(|transition| {
      matches!(transition.target(), FlowTransitionTarget::Terminal(_)) || transition.exhausted_terminal().is_some()
    })
    .map(|transition| transition.predecessor().clone())
    .collect::<BTreeSet<_>>();
  loop {
    let before = terminating.len();
    for transition in definition.transitions() {
      if matches!(transition.target(), FlowTransitionTarget::Node(successor) if terminating.contains(successor)) {
        terminating.insert(transition.predecessor().clone());
      }
    }
    if terminating.len() == before {
      break;
    }
  }
  if facts.reachable().iter().any(|node| !terminating.contains(node)) {
    return Err(invalid("Flow terminal reachability"));
  }
  validate_projections(definition)?;
  for node in definition.nodes() {
    if !node.budget().fits_within(definition.execution().budget())
      || !node
        .permissions()
        .is_no_broader_than(definition.execution().permissions())
    {
      return Err(invalid("Flow node authority"));
    }
    if let Some(reference) = node.subflow_definition() {
      let child = definitions
        .get(&reference)
        .ok_or_else(|| invalid("Flow subflow version"))?;
      validate_subflow_contract(
        node.input_schema(),
        node.outcomes(),
        node.budget(),
        node.permissions(),
        child,
      )?;
    }
  }
  Ok(())
}

fn unique_terminals(
  terminals: &[FlowTerminalDefinition],
) -> Result<BTreeMap<&FactoryKey, &FlowTerminalDefinition>, FactoryError> {
  let mut result = BTreeMap::new();
  for terminal in terminals {
    if result.insert(terminal.key(), terminal).is_some() {
      return Err(invalid("Flow terminal outcomes"));
    }
  }
  Ok(result)
}

fn validate_projections(definition: &FlowDefinition) -> Result<(), FactoryError> {
  let transitions = definition
    .transitions()
    .iter()
    .filter_map(|transition| match transition.target() {
      FlowTransitionTarget::Node(successor) => Some((transition.predecessor(), transition.outcome(), successor)),
      FlowTransitionTarget::Terminal(_) => None,
    })
    .collect::<BTreeSet<_>>();
  let mut data_routes = BTreeSet::new();
  for projection in definition.data_projections() {
    let route = (projection.predecessor(), projection.outcome(), projection.successor());
    if !transitions.contains(&route) || !data_routes.insert(route) {
      return Err(invalid("Flow data projection route"));
    }
    let source = definition
      .node(projection.predecessor())
      .ok_or_else(|| invalid("Flow data source"))?;
    let outcome = source
      .outcome(projection.outcome())
      .ok_or_else(|| invalid("Flow data outcome"))?;
    let target = definition
      .node(projection.successor())
      .ok_or_else(|| invalid("Flow data target"))?;
    if outcome.schema() != projection.schema() || target.input_schema() != Some(projection.schema()) {
      return Err(invalid("Flow data schema compatibility"));
    }
  }
  for projection in definition.context_projections() {
    if !transitions.contains(&(projection.predecessor(), projection.outcome(), projection.successor())) {
      return Err(invalid("Flow context projection route"));
    }
  }
  for node in definition.nodes() {
    if node.input_schema().is_some() && node.key() != definition.entry() {
      let incoming = transitions
        .iter()
        .filter(|(_, _, successor)| *successor == node.key())
        .count();
      let projected = data_routes
        .iter()
        .filter(|(_, _, successor)| *successor == node.key())
        .count();
      if incoming == 0 || incoming != projected {
        return Err(invalid("Flow typed input projection"));
      }
    }
  }
  Ok(())
}

fn validate_subflow_contract(
  input_schema: Option<&crate::ImmutableReference>,
  outcomes: &[FlowOutcomeDefinition],
  budget: crate::BudgetLimit,
  permissions: &crate::FactoryPermissionSet,
  child: &FlowDefinition,
) -> Result<(), FactoryError> {
  if input_schema != child.input_schema()
    || !child.execution().budget().fits_within(budget)
    || !child.execution().permissions().is_no_broader_than(permissions)
    || outcomes.len() != child.terminals().len()
    || outcomes.iter().any(|outcome| {
      child
        .terminals()
        .iter()
        .find(|terminal| terminal.key() == outcome.key())
        .is_none_or(|terminal| terminal.schema() != outcome.schema())
    })
  {
    return Err(invalid("Flow subflow contract"));
  }
  Ok(())
}

fn reject_recursive_definitions(
  reference: FlowDefinitionRef,
  definitions: &BTreeMap<FlowDefinitionRef, &FlowDefinition>,
  visiting: &mut BTreeSet<FlowDefinitionRef>,
  visited: &mut BTreeSet<FlowDefinitionRef>,
) -> Result<(), FactoryError> {
  if visited.contains(&reference) {
    return Ok(());
  }
  if !visiting.insert(reference) {
    return Err(invalid("recursive Flow Definition"));
  }
  let definition = definitions
    .get(&reference)
    .ok_or_else(|| invalid("Flow subflow version"))?;
  for child in definition.nodes().iter().filter_map(|node| node.subflow_definition()) {
    reject_recursive_definitions(child, definitions, visiting, visited)?;
  }
  visiting.remove(&reference);
  visited.insert(reference);
  Ok(())
}

fn expanded_measurements(
  reference: FlowDefinitionRef,
  definitions: &BTreeMap<FlowDefinitionRef, &FlowDefinition>,
  memo: &mut BTreeMap<FlowDefinitionRef, (u32, u16)>,
) -> Result<(u32, u16), FactoryError> {
  if let Some(measurement) = memo.get(&reference) {
    return Ok(*measurement);
  }
  let definition = definitions
    .get(&reference)
    .ok_or_else(|| invalid("Flow subflow version"))?;
  let repeat_multiplier = definition
    .transitions()
    .iter()
    .filter(|transition| transition.max_repeats() > 0)
    .try_fold(1_u32, |total, transition| {
      total.checked_mul(u32::from(transition.max_repeats()) + 1)
    })
    .ok_or_else(|| invalid("Flow repeat expansion"))?;
  let mut nodes = u32::try_from(definition.nodes().len()).map_err(|_| invalid("Flow node count"))?;
  let mut depth = 1_u16;
  for child in definition.nodes().iter().filter_map(|node| node.subflow_definition()) {
    let (child_nodes, child_depth) = expanded_measurements(child, definitions, memo)?;
    nodes = nodes
      .checked_add(child_nodes)
      .ok_or_else(|| invalid("Flow expanded node count"))?;
    depth = depth.max(
      child_depth
        .checked_add(1)
        .ok_or_else(|| invalid("Flow nesting depth"))?,
    );
  }
  nodes = nodes
    .checked_mul(repeat_multiplier)
    .ok_or_else(|| invalid("Flow repeat expansion"))?;
  let measurement = (nodes, depth);
  memo.insert(reference, measurement);
  Ok(measurement)
}

fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidConfiguration { field }
}
