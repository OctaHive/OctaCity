use crate::{
  FactoryKey,
  graph::{self, StructuralEdge, StructuralFacts, StructuralIssue},
};
use serde::{Deserialize, Serialize};

/// One canonical control edge from predecessor to successor.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FactoryFlowEdge {
  predecessor: FactoryKey,
  successor: FactoryKey,
}

impl FactoryFlowEdge {
  /// Constructs a directed control edge.
  #[must_use]
  pub(crate) const fn new(predecessor: FactoryKey, successor: FactoryKey) -> Self {
    Self { predecessor, successor }
  }

  /// Returns the node that must precede the successor.
  #[cfg(test)]
  #[must_use]
  pub(crate) const fn predecessor(&self) -> &FactoryKey {
    &self.predecessor
  }

  /// Returns the node made eligible by the predecessor outcome.
  #[cfg(test)]
  #[must_use]
  pub(crate) const fn successor(&self) -> &FactoryKey {
    &self.successor
  }
}

/// Deterministic structural problem found before Factory policy is applied.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum FactoryFlowGraphIssue {
  /// The node identity occurs more than once.
  DuplicateNode {
    /// Repeated node identity.
    node: FactoryKey,
  },
  /// The same directed edge occurs more than once.
  DuplicateEdge {
    /// Edge predecessor.
    predecessor: FactoryKey,
    /// Edge successor.
    successor: FactoryKey,
  },
  /// An edge or reachability root refers to an absent node.
  MissingNode {
    /// Missing node identity.
    node: FactoryKey,
  },
}

/// Bounded structural measurements used by Factory definition policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FactoryFlowGraphMeasurements {
  node_count: usize,
  edge_count: usize,
  max_depth: Option<usize>,
  max_fan_out: usize,
}

impl FactoryFlowGraphMeasurements {
  /// Returns the number of canonical nodes.
  #[cfg(test)]
  #[must_use]
  pub(crate) const fn node_count(self) -> usize {
    self.node_count
  }

  /// Returns the number of canonical directed edges.
  #[cfg(test)]
  #[must_use]
  pub(crate) const fn edge_count(self) -> usize {
    self.edge_count
  }

  /// Returns the longest path in edge units, or `None` when the graph is cyclic.
  #[cfg(test)]
  #[must_use]
  pub(crate) const fn max_depth(self) -> Option<usize> {
    self.max_depth
  }

  /// Returns the greatest number of direct successors of one node.
  #[must_use]
  pub(crate) const fn max_fan_out(self) -> usize {
    self.max_fan_out
  }
}

/// Canonical provider-neutral structure observed by Factory definition policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FactoryFlowGraphFacts {
  nodes: Vec<FactoryKey>,
  edges: Vec<FactoryFlowEdge>,
  reachable: Vec<FactoryKey>,
  components: Vec<Vec<FactoryKey>>,
  cyclic_components: Vec<Vec<FactoryKey>>,
  topological_order: Option<Vec<FactoryKey>>,
  measurements: FactoryFlowGraphMeasurements,
}

impl FactoryFlowGraphFacts {
  /// Borrows canonical nodes in ascending identity order.
  #[cfg(test)]
  #[must_use]
  pub(crate) fn nodes(&self) -> &[FactoryKey] {
    &self.nodes
  }

  /// Borrows canonical edges ordered by predecessor and successor.
  #[cfg(test)]
  #[must_use]
  pub(crate) fn edges(&self) -> &[FactoryFlowEdge] {
    &self.edges
  }

  /// Borrows nodes reachable from the requested roots, including the roots.
  #[must_use]
  pub(crate) fn reachable(&self) -> &[FactoryKey] {
    &self.reachable
  }

  /// Borrows canonical strongly connected components.
  #[cfg(test)]
  #[must_use]
  pub(crate) fn components(&self) -> &[Vec<FactoryKey>] {
    &self.components
  }

  /// Borrows components that contain a cycle or self-edge.
  #[must_use]
  pub(crate) fn cyclic_components(&self) -> &[Vec<FactoryKey>] {
    &self.cyclic_components
  }

  /// Borrows the canonical acyclic topological projection.
  #[cfg(test)]
  #[must_use]
  pub(crate) fn topological_order(&self) -> Option<&[FactoryKey]> {
    self.topological_order.as_deref()
  }

  /// Returns structural measurements for subsequent bounded policy checks.
  #[must_use]
  pub(crate) const fn measurements(&self) -> FactoryFlowGraphMeasurements {
    self.measurements
  }
}

/// Analyzes one Factory control graph without applying node or execution policy.
///
/// Edges are interpreted as `predecessor -> successor`. Control structure does
/// not grant context, Artifact, permission, or execution authority. Cycles are
/// reported as facts; Factory definition policy decides whether an explicit
/// bounded-repeat declaration is valid.
pub(crate) fn analyze_factory_flow_graph(
  nodes: impl IntoIterator<Item = FactoryKey>,
  edges: impl IntoIterator<Item = FactoryFlowEdge>,
  roots: impl IntoIterator<Item = FactoryKey>,
) -> Result<FactoryFlowGraphFacts, Vec<FactoryFlowGraphIssue>> {
  graph::analyze(
    nodes,
    edges
      .into_iter()
      .map(|edge| StructuralEdge::new(edge.predecessor, edge.successor)),
    roots,
  )
  .map(FactoryFlowGraphFacts::from)
  .map_err(|issues| issues.into_iter().map(FactoryFlowGraphIssue::from).collect())
}

impl From<StructuralFacts<FactoryKey>> for FactoryFlowGraphFacts {
  fn from(facts: StructuralFacts<FactoryKey>) -> Self {
    Self {
      nodes: facts.nodes,
      edges: facts
        .edges
        .into_iter()
        .map(|edge| FactoryFlowEdge::new(edge.predecessor, edge.successor))
        .collect(),
      reachable: facts.reachable,
      components: facts.components,
      cyclic_components: facts.cyclic_components,
      topological_order: facts.topological_order,
      measurements: FactoryFlowGraphMeasurements {
        node_count: facts.measurements.node_count,
        edge_count: facts.measurements.edge_count,
        max_depth: facts.measurements.max_depth,
        max_fan_out: facts.measurements.max_fan_out,
      },
    }
  }
}

impl From<StructuralIssue<FactoryKey>> for FactoryFlowGraphIssue {
  fn from(issue: StructuralIssue<FactoryKey>) -> Self {
    match issue {
      StructuralIssue::DuplicateNode(node) => Self::DuplicateNode { node },
      StructuralIssue::DuplicateEdge(edge) => Self::DuplicateEdge {
        predecessor: edge.predecessor,
        successor: edge.successor,
      },
      StructuralIssue::MissingNode(node) => Self::MissingNode { node },
    }
  }
}
