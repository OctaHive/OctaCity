use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct StructuralEdge<Id> {
  pub(crate) predecessor: Id,
  pub(crate) successor: Id,
}

impl<Id> StructuralEdge<Id> {
  pub(crate) fn new(predecessor: Id, successor: Id) -> Self {
    Self { predecessor, successor }
  }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum StructuralIssue<Id> {
  DuplicateNode(Id),
  DuplicateEdge(StructuralEdge<Id>),
  MissingNode(Id),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StructuralMeasurements {
  pub(crate) node_count: usize,
  pub(crate) edge_count: usize,
  pub(crate) max_depth: Option<usize>,
  pub(crate) max_fan_out: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StructuralFacts<Id> {
  pub(crate) nodes: Vec<Id>,
  pub(crate) edges: Vec<StructuralEdge<Id>>,
  pub(crate) reachable: Vec<Id>,
  pub(crate) components: Vec<Vec<Id>>,
  pub(crate) cyclic_components: Vec<Vec<Id>>,
  pub(crate) topological_order: Option<Vec<Id>>,
  pub(crate) measurements: StructuralMeasurements,
}

pub(crate) fn analyze<Id: Clone + Ord>(
  nodes: impl IntoIterator<Item = Id>,
  edges: impl IntoIterator<Item = StructuralEdge<Id>>,
  roots: impl IntoIterator<Item = Id>,
) -> Result<StructuralFacts<Id>, Vec<StructuralIssue<Id>>> {
  let mut canonical_nodes = BTreeSet::new();
  let mut issues = BTreeSet::new();
  for node in nodes {
    if !canonical_nodes.insert(node.clone()) {
      issues.insert(StructuralIssue::DuplicateNode(node));
    }
  }

  let mut canonical_edges = BTreeSet::new();
  for edge in edges {
    if !canonical_edges.insert(edge.clone()) {
      issues.insert(StructuralIssue::DuplicateEdge(edge.clone()));
    }
    if !canonical_nodes.contains(&edge.predecessor) {
      issues.insert(StructuralIssue::MissingNode(edge.predecessor));
    }
    if !canonical_nodes.contains(&edge.successor) {
      issues.insert(StructuralIssue::MissingNode(edge.successor));
    }
  }

  let canonical_roots = roots.into_iter().collect::<BTreeSet<_>>();
  for root in &canonical_roots {
    if !canonical_nodes.contains(root) {
      issues.insert(StructuralIssue::MissingNode(root.clone()));
    }
  }
  if !issues.is_empty() {
    return Err(issues.into_iter().collect());
  }

  let adjacency = adjacency(&canonical_nodes, &canonical_edges);
  let components = strongly_connected_components(&canonical_nodes, &adjacency);
  let cyclic_components = components
    .iter()
    .filter(|component| is_cyclic_component(component, &adjacency))
    .cloned()
    .collect::<Vec<_>>();
  let topological_order = topological_order(&canonical_nodes, &adjacency);
  let max_depth = topological_order
    .as_deref()
    .map(|order| maximum_depth(order, &adjacency));

  Ok(StructuralFacts {
    nodes: canonical_nodes.iter().cloned().collect(),
    edges: canonical_edges.iter().cloned().collect(),
    reachable: reachable(&canonical_roots, &adjacency),
    components,
    cyclic_components,
    topological_order,
    measurements: StructuralMeasurements {
      node_count: canonical_nodes.len(),
      edge_count: canonical_edges.len(),
      max_depth,
      max_fan_out: adjacency.values().map(BTreeSet::len).max().unwrap_or(0),
    },
  })
}

fn adjacency<Id: Clone + Ord>(
  nodes: &BTreeSet<Id>,
  edges: &BTreeSet<StructuralEdge<Id>>,
) -> BTreeMap<Id, BTreeSet<Id>> {
  let mut adjacency = nodes
    .iter()
    .map(|node| (node.clone(), BTreeSet::new()))
    .collect::<BTreeMap<_, _>>();
  for edge in edges {
    adjacency
      .get_mut(&edge.predecessor)
      .expect("validated predecessor is present")
      .insert(edge.successor.clone());
  }
  adjacency
}

fn reachable<Id: Clone + Ord>(roots: &BTreeSet<Id>, adjacency: &BTreeMap<Id, BTreeSet<Id>>) -> Vec<Id> {
  let mut visited = BTreeSet::new();
  let mut pending = roots.iter().rev().cloned().collect::<Vec<_>>();
  while let Some(node) = pending.pop() {
    if !visited.insert(node.clone()) {
      continue;
    }
    if let Some(successors) = adjacency.get(&node) {
      pending.extend(successors.iter().rev().filter(|next| !visited.contains(*next)).cloned());
    }
  }
  visited.into_iter().collect()
}

fn strongly_connected_components<Id: Clone + Ord>(
  nodes: &BTreeSet<Id>,
  adjacency: &BTreeMap<Id, BTreeSet<Id>>,
) -> Vec<Vec<Id>> {
  let mut visited = BTreeSet::new();
  let mut finish_order = Vec::with_capacity(nodes.len());
  for node in nodes {
    visit_for_finish_order(node, adjacency, &mut visited, &mut finish_order);
  }

  let reverse = reverse_adjacency(nodes, adjacency);
  visited.clear();
  let mut components = Vec::new();
  for node in finish_order.into_iter().rev() {
    if visited.contains(&node) {
      continue;
    }
    let mut component = BTreeSet::new();
    let mut pending = vec![node];
    while let Some(current) = pending.pop() {
      if !visited.insert(current.clone()) {
        continue;
      }
      component.insert(current.clone());
      pending.extend(
        reverse[&current]
          .iter()
          .rev()
          .filter(|next| !visited.contains(*next))
          .cloned(),
      );
    }
    components.push(component.into_iter().collect::<Vec<_>>());
  }
  components.sort();
  components
}

fn visit_for_finish_order<Id: Clone + Ord>(
  root: &Id,
  adjacency: &BTreeMap<Id, BTreeSet<Id>>,
  visited: &mut BTreeSet<Id>,
  finish_order: &mut Vec<Id>,
) {
  if visited.contains(root) {
    return;
  }
  let mut pending = vec![(root.to_owned(), false)];
  while let Some((node, expanded)) = pending.pop() {
    if expanded {
      finish_order.push(node);
      continue;
    }
    if !visited.insert(node.clone()) {
      continue;
    }
    pending.push((node.clone(), true));
    pending.extend(
      adjacency[&node]
        .iter()
        .rev()
        .filter(|next| !visited.contains(*next))
        .map(|next| (next.clone(), false)),
    );
  }
}

fn reverse_adjacency<Id: Clone + Ord>(
  nodes: &BTreeSet<Id>,
  adjacency: &BTreeMap<Id, BTreeSet<Id>>,
) -> BTreeMap<Id, BTreeSet<Id>> {
  let mut reverse = nodes
    .iter()
    .map(|node| (node.clone(), BTreeSet::new()))
    .collect::<BTreeMap<_, _>>();
  for (predecessor, successors) in adjacency {
    for successor in successors {
      reverse
        .get_mut(successor)
        .expect("validated successor is present")
        .insert(predecessor.clone());
    }
  }
  reverse
}

fn is_cyclic_component<Id: Ord>(component: &[Id], adjacency: &BTreeMap<Id, BTreeSet<Id>>) -> bool {
  component.len() > 1 || component.first().is_some_and(|node| adjacency[node].contains(node))
}

fn topological_order<Id: Clone + Ord>(nodes: &BTreeSet<Id>, adjacency: &BTreeMap<Id, BTreeSet<Id>>) -> Option<Vec<Id>> {
  let mut in_degree = nodes
    .iter()
    .map(|node| (node.clone(), 0_usize))
    .collect::<BTreeMap<_, _>>();
  for successors in adjacency.values() {
    for successor in successors {
      *in_degree.get_mut(successor).expect("validated successor is present") += 1;
    }
  }
  let mut ready = in_degree
    .iter()
    .filter_map(|(node, count)| (*count == 0).then_some(node.clone()))
    .collect::<BTreeSet<_>>();
  let mut order = Vec::with_capacity(nodes.len());
  while let Some(node) = ready.pop_first() {
    order.push(node.clone());
    for successor in &adjacency[&node] {
      let count = in_degree.get_mut(successor).expect("validated successor is present");
      *count -= 1;
      if *count == 0 {
        ready.insert(successor.clone());
      }
    }
  }
  (order.len() == nodes.len()).then_some(order)
}

fn maximum_depth<Id: Clone + Ord>(order: &[Id], adjacency: &BTreeMap<Id, BTreeSet<Id>>) -> usize {
  let mut depths = order
    .iter()
    .map(|node| (node.clone(), 0_usize))
    .collect::<BTreeMap<_, _>>();
  for node in order {
    let next_depth = depths[node] + 1;
    for successor in &adjacency[node] {
      let depth = depths.get_mut(successor).expect("validated successor is present");
      *depth = (*depth).max(next_depth);
    }
  }
  depths.into_values().max().unwrap_or(0)
}
