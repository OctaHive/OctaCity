//! A shared Issue slot can have several producers only on exclusive routes.
use crate::{
  FactoryError, FactoryKey, FlowDefinitionRef, FlowIssueOutcomeLocation, FlowIssueSlot, FlowIssueSlotConflict,
  FlowTransitionTarget, PinnedFlowDefinitionClosure,
};
use std::collections::{BTreeMap, BTreeSet};

type VertexId = usize;
type ProducerId = usize;

#[derive(Default)]
struct Vertex {
  // Node entries select one outcome. An outcome dispatches all of its routes.
  simultaneous: bool,
  scope: VertexId,
  successors: Vec<VertexId>,
}

struct Graph<'a> {
  remaining: usize,
  vertices: Vec<Vertex>,
  call_outcomes: Vec<(VertexId, VertexId)>,
  declarations: BTreeMap<(FlowDefinitionRef, &'a FactoryKey, &'a FactoryKey), ProducerId>,
  locations: Vec<FlowIssueOutcomeLocation>,
  slots: BTreeMap<FlowIssueSlot, Vec<(VertexId, ProducerId)>>,
}

impl<'a> Graph<'a> {
  fn vertex(&mut self, scope: VertexId, simultaneous: bool) -> Result<VertexId, FactoryError> {
    self.remaining = self
      .remaining
      .checked_sub(1)
      .ok_or(FactoryError::InvalidConfiguration {
        field: "Flow Issue presentation analysis exceeds the graph safety limit",
      })?;
    let id = self.vertices.len();
    self.vertices.push(Vertex {
      simultaneous,
      scope,
      successors: vec![],
    });
    Ok(id)
  }

  // Inline call sites so each child terminal continues from the matching
  // caller outcome. Occurrences of the same declaration share a producer ID.
  fn expand(
    &mut self,
    closure: &'a PinnedFlowDefinitionClosure,
    reference: FlowDefinitionRef,
    returned: &BTreeMap<&FactoryKey, VertexId>,
  ) -> Result<VertexId, FactoryError> {
    let scope = self.vertices.len();
    let definition = closure.definition(reference).expect("validated Flow reference");
    let entries = definition
      .nodes()
      .iter()
      .map(|node| Ok((node.key(), self.vertex(scope, false)?)))
      .collect::<Result<BTreeMap<_, _>, FactoryError>>()?;
    let mut outcomes = BTreeMap::new();
    for node in definition.nodes() {
      let mut exits = BTreeMap::new();
      for (outcome_index, outcome) in node.outcomes().iter().enumerate() {
        let vertex = self.vertex(scope, true)?;
        exits.insert(outcome.key(), vertex);
        outcomes.insert((node.key(), outcome.key()), vertex);
        if let Some(presentation) = outcome.issue_presentation() {
          let producer = *self
            .declarations
            .entry((reference, node.key(), outcome.key()))
            .or_insert_with(|| {
              let id = self.locations.len();
              self.locations.push(FlowIssueOutcomeLocation {
                definition: reference,
                node: node.key().clone(),
                outcome: outcome.key().clone(),
                outcome_index,
              });
              id
            });
          let slot = presentation.slot(reference.id(), node.key(), outcome.key());
          self.slots.entry(slot).or_default().push((vertex, producer));
        }
      }
      let successors = match node.subflow_definition() {
        Some(child) => {
          let call = entries[node.key()];
          self
            .call_outcomes
            .extend(exits.values().map(|&outcome| (call, outcome)));
          vec![self.expand(closure, child, &exits)?]
        }
        None => exits.into_values().collect(),
      };
      self.vertices[entries[node.key()]].successors = successors;
    }
    for route in definition.transitions() {
      let vertex = outcomes[&(route.predecessor(), route.outcome())];
      let target = match route.target() {
        FlowTransitionTarget::Node(node) => Some(entries[node]),
        FlowTransitionTarget::Terminal(terminal) => returned.get(terminal).copied(),
      };
      // An exhausted repeat and its continuation are alternative routes.
      let exhausted = route
        .exhausted_node()
        .map(|node| entries[node])
        .or_else(|| route.exhausted_terminal().and_then(|key| returned.get(key)).copied());
      let successors = target.into_iter().chain(exhausted).collect::<Vec<_>>();
      if successors.len() > 1 {
        let choice = self.vertex(scope, false)?;
        self.vertices[choice].successors = successors;
        self.vertices[vertex].successors.push(choice);
      } else {
        self.vertices[vertex].successors.extend(successors);
      }
    }
    Ok(entries[definition.entry()])
  }

  fn reachable(&self, entry: VertexId) -> BTreeSet<VertexId> {
    let mut reachable = BTreeSet::new();
    let mut pending = vec![entry];
    while let Some(vertex) = pending.pop() {
      if reachable.insert(vertex) {
        pending.extend(&self.vertices[vertex].successors);
      }
    }
    reachable
  }

  fn validate(&self, entry: VertexId) -> Result<(), FactoryError> {
    let reachable = self.reachable(entry);
    let mut predecessors = vec![vec![]; self.vertices.len()];
    let mut local_predecessors = vec![vec![]; self.vertices.len()];
    for &vertex in &reachable {
      for &successor in &self.vertices[vertex].successors {
        predecessors[successor].push(vertex);
        // Returning to a caller crosses out of this Flow invocation. In
        // the local graph the caller selects one outcome at the call site.
        if self.vertices[vertex].scope <= self.vertices[successor].scope {
          local_predecessors[successor].push(vertex);
        }
      }
    }
    for &(call, outcome) in &self.call_outcomes {
      if reachable.contains(&outcome) {
        local_predecessors[outcome].push(call);
      }
    }

    let mut all = ProducerReach::new(self.vertices.len());
    let mut local = ProducerReach::new(self.vertices.len());
    for (slot, producers) in &self.slots {
      let mut producers = producers
        .iter()
        .copied()
        .filter(|(vertex, _)| reachable.contains(vertex))
        .collect::<Vec<_>>();
      producers.sort_unstable_by_key(|&(_, producer)| producer);
      if producers.first().map(|pair| pair.1) == producers.last().map(|pair| pair.1) {
        continue;
      }

      let visited = all.collect(&predecessors, &producers);
      let locally_visited = local.collect(&local_predecessors, &producers);
      if let Some((left, right)) = producers
        .iter()
        .filter_map(|&(vertex, _)| all.range(vertex))
        .find(|&(left, right)| left != right)
        .or_else(|| {
          visited
            .iter()
            .find_map(|&vertex| self.parallel_conflict(vertex, &all, &local))
        })
      {
        let mut producers = [self.locations[left].clone(), self.locations[right].clone()];
        producers.sort_by(|left, right| {
          (&left.definition, &left.node, left.outcome_index).cmp(&(&right.definition, &right.node, right.outcome_index))
        });
        return Err(FactoryError::AmbiguousIssuePresentationSlot {
          context: Box::new(FlowIssueSlotConflict {
            slot: slot.clone(),
            producers,
          }),
        });
      }
      all.clear(visited);
      local.clear(locally_visited);
    }
    Ok(())
  }

  fn parallel_conflict(&self, vertex: VertexId, all: &ProducerReach, local: &ProducerReach) -> ProducerRange {
    let vertex = &self.vertices[vertex];
    if !vertex.simultaneous {
      return None;
    }
    let mut earlier = None;
    let mut earlier_local = None;
    for &successor in &vertex.successors {
      let branch = all.range(successor);
      let branch_local = if self.vertices[successor].scope == vertex.scope {
        local.range(successor)
      } else {
        None
      };
      // A fork in a child can produce several local results, but can
      // select only one caller outcome. Compare siblings only when at
      // least one producer can run inside the fork's Flow invocation.
      if let Some(pair) = conflict(earlier, branch_local).or_else(|| conflict(earlier_local, branch)) {
        return Some(pair);
      }
      earlier = union(earlier, branch);
      earlier_local = union(earlier_local, branch_local);
    }
    None
  }
}

type ProducerRange = Option<(ProducerId, ProducerId)>;

fn conflict(left: ProducerRange, right: ProducerRange) -> ProducerRange {
  let ((first, last), (other_first, other_last)) = left.zip(right)?;
  [(first, other_first), (first, other_last), (last, other_first)]
    .into_iter()
    .find(|&(a, b)| a != b)
}

fn union(left: ProducerRange, right: ProducerRange) -> ProducerRange {
  match (left, right) {
    (Some((a, b)), Some((c, d))) => Some((a.min(c), b.max(d))),
    (None, other) | (other, None) => other,
  }
}

struct ProducerReach {
  lowest: Vec<Option<ProducerId>>,
  highest: Vec<Option<ProducerId>>,
}

impl ProducerReach {
  fn new(vertices: usize) -> Self {
    Self {
      lowest: vec![None; vertices],
      highest: vec![None; vertices],
    }
  }

  fn collect(&mut self, predecessors: &[Vec<VertexId>], ordered: &[(VertexId, ProducerId)]) -> Vec<VertexId> {
    // Each pass reaches a vertex at most once, even through repeats.
    let visited = propagate(predecessors, ordered.iter().copied(), &mut self.lowest);
    propagate(predecessors, ordered.iter().rev().copied(), &mut self.highest);
    visited
  }

  fn range(&self, vertex: VertexId) -> ProducerRange {
    self.lowest[vertex].zip(self.highest[vertex])
  }

  fn clear(&mut self, visited: Vec<VertexId>) {
    for vertex in visited {
      self.lowest[vertex] = None;
      self.highest[vertex] = None;
    }
  }
}

fn propagate(
  predecessors: &[Vec<VertexId>],
  producers: impl Iterator<Item = (VertexId, ProducerId)>,
  selected: &mut [Option<ProducerId>],
) -> Vec<VertexId> {
  let mut visited = Vec::new();
  let mut pending = Vec::new();
  for (vertex, producer) in producers {
    pending.push(vertex);
    while let Some(vertex) = pending.pop() {
      if selected[vertex].is_none() {
        selected[vertex] = Some(producer);
        visited.push(vertex);
        pending.extend(&predecessors[vertex]);
      }
    }
  }
  visited
}

/// Called after structural, nesting and expansion bounds have been checked.
pub(super) fn validate(closure: &PinnedFlowDefinitionClosure) -> Result<(), FactoryError> {
  let mut slots = BTreeSet::new();
  let has_shared_slot = closure.definitions().iter().any(|definition| {
    definition.nodes().iter().any(|node| {
      node.outcomes().iter().any(|outcome| {
        outcome.issue_presentation().is_some_and(|presentation| {
          !slots.insert(presentation.slot(definition.reference().id(), node.key(), outcome.key()))
        })
      })
    })
  });
  if !has_shared_slot {
    return Ok(());
  }
  // Unlike structural expansion, the analysis charges each outcome and repeat choice.
  let mut graph = Graph {
    remaining: crate::MAX_FLOW_EXPANDED_NODES as usize,
    vertices: vec![],
    call_outcomes: vec![],
    declarations: BTreeMap::new(),
    locations: vec![],
    slots: BTreeMap::new(),
  };
  let entry = graph.expand(closure, closure.root(), &BTreeMap::new())?;
  graph.validate(entry)
}
