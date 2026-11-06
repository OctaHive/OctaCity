//! Bounded composition of editable documents before immutable publication.
use crate::{
  FactoryError, FactoryInstructions, FactoryKey, FactoryRepositoryPath, FlowContextProjection, FlowDataProjection,
  FlowDefinitionTemplate, FlowExecutionPolicy, FlowGateProgram, FlowPredicate, FlowTemplateNode, FlowTemplateSchema,
  FlowTransition, FlowTransitionTarget, MAX_FLOW_DATA_BYTES, MAX_FLOW_DEFINITION_EDGES, MAX_FLOW_DEFINITION_NODES,
  MAX_FLOW_SCHEMA_ENTRIES,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
  input_schema: FactoryKey,
  entry: FactoryKey,
  execution: FlowExecutionPolicy,
  parts: Vec<FactoryRepositoryPath>,
  #[serde(default)]
  terminal_routes: BTreeMap<FactoryKey, Option<FactoryKey>>,
  #[serde(default)]
  without_pools: BTreeSet<FactoryKey>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Part {
  schemas: BTreeMap<FactoryKey, FlowTemplateSchema>,
  deliverable_policies: BTreeMap<FactoryKey, crate::CandidateDeliverablePolicy>,
  nodes: Vec<FlowTemplateNode>,
  gate_programs: BTreeMap<FactoryKey, FlowGateProgram>,
  gate_bindings: BTreeMap<FactoryKey, GateBinding>,
  assignments: BTreeMap<FactoryKey, FactoryInstructions>,
  transitions: Vec<FlowTransition>,
  terminals: BTreeMap<FactoryKey, FactoryKey>,
  context_projections: Vec<FlowContextProjection>,
  data_projections: Vec<FlowDataProjection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GateBinding {
  program: FactoryKey,
  precondition: FlowPredicate,
  #[serde(default)]
  outputs: BTreeMap<FactoryKey, crate::FlowDataMapping>,
}
impl FlowDefinitionTemplate {
  /// Assembles explicitly listed, flat configuration parts into the same editable
  /// template consumed by `instantiate`. File and node names carry no semantics.
  ///
  /// The caller resolves portable relative paths from its chosen configuration
  /// root; the domain performs no filesystem or network access. Resolve all parts
  /// before publication: admitted runs retain only the immutable compiled closure,
  /// never mutable file references. Nested imports and silent overrides are rejected.
  /// A manifest may explicitly connect a part's terminal to a node (or remove it
  /// with `null`) and remove a named node's queue. Parts may supply missing step assignments.
  pub fn from_manifest(
    bytes: &[u8],
    mut resolve: impl FnMut(&FactoryRepositoryPath) -> Result<Vec<u8>, FactoryError>,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if manifest.parts.is_empty() || manifest.parts.len() > MAX_FLOW_DEFINITION_NODES {
      return Err(invalid());
    }
    let mut template = Self {
      input_schema: manifest.input_schema,
      entry: manifest.entry,
      execution: manifest.execution,
      schemas: BTreeMap::new(),
      deliverable_policies: BTreeMap::new(),
      nodes: vec![],
      transitions: vec![],
      terminals: BTreeMap::new(),
      context_projections: vec![],
      data_projections: vec![],
    };
    let mut total = bytes.len();
    let mut programs = BTreeMap::new();
    let mut gate_bindings = BTreeMap::new();
    let mut assignments = BTreeMap::new();
    let mut paths = BTreeSet::new();
    for path in manifest.parts {
      if !paths.insert(path.clone()) {
        return Err(invalid());
      }
      let bytes = resolve(&path)?;
      total = total.checked_add(bytes.len()).ok_or_else(invalid)?;
      if total > MAX_FLOW_DATA_BYTES {
        return Err(invalid());
      }
      let part: Part = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
      for (key, schema) in part.schemas {
        if template.schemas.insert(key, schema).is_some() {
          return Err(invalid());
        }
      }
      for (key, policy) in part.deliverable_policies {
        if template.deliverable_policies.insert(key, policy).is_some() {
          return Err(invalid());
        }
      }
      for (key, program) in part.gate_programs {
        if program.has_precondition() || programs.insert(key, program).is_some() {
          return Err(invalid());
        }
      }
      for (node, binding) in part.gate_bindings {
        if gate_bindings.insert(node, binding).is_some() {
          return Err(invalid());
        }
      }
      for (node, task) in part.assignments {
        if assignments.insert(node, task).is_some() {
          return Err(invalid());
        }
      }
      for (key, schema) in part.terminals {
        if template.terminals.insert(key, schema).is_some() {
          return Err(invalid());
        }
      }
      for node in part.nodes {
        if template.nodes.iter().any(|selected| selected.key == node.key) {
          return Err(invalid());
        }
        template.nodes.push(node);
      }
      template.transitions.extend(part.transitions);
      template.context_projections.extend(part.context_projections);
      template.data_projections.extend(part.data_projections);
      if template.schemas.len() > MAX_FLOW_SCHEMA_ENTRIES
        || template.deliverable_policies.len() > MAX_FLOW_SCHEMA_ENTRIES
        || programs.len() > MAX_FLOW_DEFINITION_NODES
        || gate_bindings.len() > MAX_FLOW_DEFINITION_NODES
        || assignments.len() > MAX_FLOW_DEFINITION_NODES
        || template.nodes.len() > MAX_FLOW_DEFINITION_NODES
        || template.transitions.len() > MAX_FLOW_DEFINITION_EDGES
      {
        return Err(invalid());
      }
    }
    for (key, binding) in gate_bindings {
      let node = template
        .nodes
        .iter_mut()
        .find(|node| node.key == key)
        .ok_or_else(invalid)?;
      if node.gate.is_some() || node.kind != crate::FlowNodeKind::DeterministicGate {
        return Err(invalid());
      }
      let program = programs.get(&binding.program).ok_or_else(invalid)?;
      let mut program = program.clone().with_precondition(binding.precondition)?;
      for (outcome, mapping) in binding.outputs {
        program = program.with_output_mapping(outcome, mapping)?;
      }
      node.gate = Some(program);
    }
    for node in &mut template.nodes {
      if let Some(task) = assignments.remove(&node.key) {
        if node.task.is_some() || node.kind != crate::FlowNodeKind::Reasoning {
          return Err(invalid());
        }
        node.task = Some(task);
      }
    }
    if !assignments.is_empty() {
      return Err(invalid());
    }
    for key in manifest.without_pools {
      template
        .nodes
        .iter_mut()
        .find(|node| node.key == key)
        .and_then(|node| node.pool.take())
        .ok_or_else(invalid)?;
    }
    for (terminal, next) in manifest.terminal_routes {
      if template.terminals.remove(&terminal).is_none()
        || next
          .as_ref()
          .is_some_and(|next| !template.nodes.iter().any(|node| &node.key == next))
        || template.transitions.iter().any(|edge| {
          edge.exhausted_terminal() == Some(&terminal)
            || (edge.target() == &FlowTransitionTarget::Terminal(terminal.clone())
              && (edge.max_repeats() != 0 || edge.starts_cycle()))
        })
      {
        return Err(invalid());
      }
      template.transitions.retain_mut(|edge| {
        if !matches!(edge.target(), FlowTransitionTarget::Terminal(key) if key == &terminal) {
          return true;
        }
        let Some(next) = &next else { return false };
        *edge = FlowTransition::new(
          edge.predecessor().clone(),
          edge.outcome().clone(),
          FlowTransitionTarget::Node(next.clone()),
        );
        true
      });
    }
    Ok(template)
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow template manifest",
  }
}
