use std::{
  collections::{BTreeMap, BTreeSet},
  ops::Deref,
};

use octacity_server_domain::{JobName, PipelineNodeId, canonicalize_json};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::Value;

use crate::{CapabilityCatalog, DependencyPolicy, ExecutionCapability, PipelineError};

/// Maximum Job nodes in one immutable Pipeline version.
pub const MAX_PIPELINE_NODES: usize = 1_024;
/// Maximum dependency edges in one immutable Pipeline version.
pub const MAX_PIPELINE_EDGES: usize = 8_192;
/// Maximum direct predecessors of one Pipeline node.
pub const MAX_PIPELINE_FAN_IN: usize = 256;
/// Maximum direct dependents of one Pipeline node.
pub const MAX_PIPELINE_FAN_OUT: usize = 256;
/// Maximum encoded bytes in one Pipeline Job template.
pub const MAX_PIPELINE_NODE_TEMPLATE_BYTES: usize = 256 * 1_024;
/// Maximum encoded bytes in one complete immutable Pipeline snapshot.
pub const MAX_PIPELINE_SNAPSHOT_BYTES: usize = 8 * 1_024 * 1_024;

const PIPELINE_SNAPSHOT_SCHEMA_VERSION: u16 = 1;

/// One immutable Job template inside a Pipeline version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineNode {
  id: PipelineNodeId,
  name: JobName,
  dependency_policy: DependencyPolicy,
  required_capabilities: Vec<ExecutionCapability>,
  template: Value,
}

impl PipelineNode {
  /// Constructs and canonicalizes one bounded Job template.
  pub fn new(
    id: PipelineNodeId,
    name: JobName,
    dependency_policy: DependencyPolicy,
    mut required_capabilities: Vec<ExecutionCapability>,
    template: Value,
  ) -> Result<Self, PipelineError> {
    required_capabilities.sort_unstable();
    if required_capabilities.windows(2).any(|pair| pair[0] == pair[1]) {
      return Err(PipelineError::DuplicateCapability { node_id: id });
    }
    let template = canonicalize_json(template).map_err(|_| PipelineError::InvalidNodeTemplate)?;
    validate_template(&template)?;
    Ok(Self {
      id,
      name,
      dependency_policy,
      required_capabilities,
      template,
    })
  }

  /// Returns the stable node identity.
  #[must_use]
  pub fn id(&self) -> &PipelineNodeId {
    &self.id
  }

  /// Returns the bounded operator-facing Job name.
  #[must_use]
  pub fn name(&self) -> &JobName {
    &self.name
  }

  /// Returns the immutable dependency and failure-propagation policy.
  #[must_use]
  pub const fn dependency_policy(&self) -> DependencyPolicy {
    self.dependency_policy
  }

  /// Returns canonical required capabilities in stable lexical order.
  #[must_use]
  pub fn required_capabilities(&self) -> &[ExecutionCapability] {
    &self.required_capabilities
  }

  /// Returns the canonical bounded provider-neutral Job template.
  #[must_use]
  pub const fn template(&self) -> &Value {
    &self.template
  }

  fn canonicalize(&mut self) -> Result<(), PipelineError> {
    self.required_capabilities.sort_unstable();
    if self.required_capabilities.windows(2).any(|pair| pair[0] == pair[1]) {
      return Err(PipelineError::DuplicateCapability {
        node_id: self.id.clone(),
      });
    }
    self.template =
      canonicalize_json(std::mem::take(&mut self.template)).map_err(|_| PipelineError::InvalidNodeTemplate)?;
    validate_template(&self.template)
  }
}

/// One directed dependency from a predecessor to a dependent Job template.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineEdge {
  predecessor: PipelineNodeId,
  dependent: PipelineNodeId,
}

impl PipelineEdge {
  /// Constructs one directed dependency edge.
  #[must_use]
  pub const fn new(predecessor: PipelineNodeId, dependent: PipelineNodeId) -> Self {
    Self { predecessor, dependent }
  }

  /// Returns the direct predecessor node identity.
  #[must_use]
  pub fn predecessor(&self) -> &PipelineNodeId {
    &self.predecessor
  }

  /// Returns the dependent node identity.
  #[must_use]
  pub fn dependent(&self) -> &PipelineNodeId {
    &self.dependent
  }
}

/// Canonical validated immutable Pipeline DAG.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipelineDag {
  nodes: Vec<PipelineNode>,
  edges: Vec<PipelineEdge>,
}

/// A Pipeline DAG whose execution capabilities were checked for publication.
///
/// This proof type is intentionally not deserializable. Persisted historical
/// snapshots deserialize as [`PipelineDag`] and must be checked against the
/// current [`CapabilityCatalog`] before they can cross a publication seam.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PublishablePipelineDag(PipelineDag);

impl PublishablePipelineDag {
  /// Validates and canonicalizes a draft against publishable capabilities.
  pub fn new(
    nodes: Vec<PipelineNode>,
    edges: Vec<PipelineEdge>,
    capabilities: &CapabilityCatalog,
  ) -> Result<Self, PipelineError> {
    PipelineDag::build(nodes, edges, Some(capabilities)).map(Self)
  }

  /// Borrows the validated immutable DAG.
  #[must_use]
  pub const fn as_dag(&self) -> &PipelineDag {
    &self.0
  }

  /// Consumes the publication proof and returns the immutable snapshot.
  #[must_use]
  pub fn into_dag(self) -> PipelineDag {
    self.0
  }
}

impl Deref for PublishablePipelineDag {
  type Target = PipelineDag;

  fn deref(&self) -> &Self::Target {
    self.as_dag()
  }
}

impl PipelineDag {
  /// Checks a restored historical snapshot against the current publication catalog.
  pub fn for_publication(&self, capabilities: &CapabilityCatalog) -> Result<PublishablePipelineDag, PipelineError> {
    Self::build(self.nodes.clone(), self.edges.clone(), Some(capabilities)).map(PublishablePipelineDag)
  }

  /// Returns nodes in stable identity order.
  #[must_use]
  pub fn nodes(&self) -> &[PipelineNode] {
    &self.nodes
  }

  /// Returns edges in stable predecessor-then-dependent order.
  #[must_use]
  pub fn edges(&self) -> &[PipelineEdge] {
    &self.edges
  }

  /// Returns roots in stable identity order.
  #[must_use]
  pub fn roots(&self) -> Vec<&PipelineNode> {
    let dependents: BTreeSet<_> = self.edges.iter().map(|edge| &edge.dependent).collect();
    self
      .nodes
      .iter()
      .filter(|node| !dependents.contains(&node.id))
      .collect()
  }

  /// Returns a deterministic topological node order, breaking ties by identity.
  #[must_use]
  pub fn topological_order(&self) -> Vec<&PipelineNode> {
    let identities =
      topological_identities(&self.nodes, &self.edges).expect("a constructed Pipeline DAG is always acyclic");
    identities
      .into_iter()
      .map(|identity| {
        self
          .nodes
          .binary_search_by(|node| node.id.cmp(&identity))
          .ok()
          .and_then(|index| self.nodes.get(index))
          .expect("topological identities originate from canonical nodes")
      })
      .collect()
  }

  /// Revalidates structural bounds at an adapter seam.
  pub fn validate(&self) -> Result<(), PipelineError> {
    Self::build(self.nodes.clone(), self.edges.clone(), None).map(|_| ())
  }

  fn build(
    mut nodes: Vec<PipelineNode>,
    mut edges: Vec<PipelineEdge>,
    capabilities: Option<&CapabilityCatalog>,
  ) -> Result<Self, PipelineError> {
    if nodes.is_empty() {
      return Err(PipelineError::Empty);
    }
    if nodes.len() > MAX_PIPELINE_NODES {
      return Err(PipelineError::TooManyNodes);
    }
    if edges.len() > MAX_PIPELINE_EDGES {
      return Err(PipelineError::TooManyEdges);
    }
    for node in &mut nodes {
      node.canonicalize()?;
    }
    nodes.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    if let Some(pair) = nodes.windows(2).find(|pair| pair[0].id == pair[1].id) {
      return Err(PipelineError::DuplicateNode {
        node_id: pair[0].id.clone(),
      });
    }
    if let Some(capabilities) = capabilities {
      for node in &nodes {
        if let Some(capability) = node
          .required_capabilities
          .iter()
          .find(|capability| !capabilities.supports(capability))
        {
          return Err(PipelineError::UnavailableCapability {
            node_id: node.id.clone(),
            capability: capability.clone(),
          });
        }
      }
    }

    edges.sort_unstable();
    if edges.windows(2).any(|pair| pair[0] == pair[1]) {
      return Err(PipelineError::DuplicateEdge);
    }
    let identities: BTreeSet<_> = nodes.iter().map(|node| &node.id).collect();
    let mut fan_in = BTreeMap::<&PipelineNodeId, usize>::new();
    let mut fan_out = BTreeMap::<&PipelineNodeId, usize>::new();
    for edge in &edges {
      if edge.predecessor == edge.dependent {
        return Err(PipelineError::SelfDependency {
          node_id: edge.predecessor.clone(),
        });
      }
      for identity in [&edge.predecessor, &edge.dependent] {
        if !identities.contains(identity) {
          return Err(PipelineError::MissingNode {
            node_id: identity.clone(),
          });
        }
      }
      let incoming = fan_in.entry(&edge.dependent).or_default();
      *incoming += 1;
      if *incoming > MAX_PIPELINE_FAN_IN {
        return Err(PipelineError::FanInExceeded {
          node_id: edge.dependent.clone(),
        });
      }
      let outgoing = fan_out.entry(&edge.predecessor).or_default();
      *outgoing += 1;
      if *outgoing > MAX_PIPELINE_FAN_OUT {
        return Err(PipelineError::FanOutExceeded {
          node_id: edge.predecessor.clone(),
        });
      }
    }
    topological_identities(&nodes, &edges)?;
    let dag = Self { nodes, edges };
    if serde_json::to_vec(&dag)
      .map(|encoded| encoded.len() > MAX_PIPELINE_SNAPSHOT_BYTES)
      .unwrap_or(true)
    {
      return Err(PipelineError::SnapshotTooLarge);
    }
    Ok(dag)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PipelineDagWire {
  schema_version: u16,
  nodes: Vec<PipelineNode>,
  edges: Vec<PipelineEdge>,
}

#[derive(Serialize)]
struct PipelineDagRef<'a> {
  schema_version: u16,
  nodes: &'a [PipelineNode],
  edges: &'a [PipelineEdge],
}

impl Serialize for PipelineDag {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    PipelineDagRef {
      schema_version: PIPELINE_SNAPSHOT_SCHEMA_VERSION,
      nodes: &self.nodes,
      edges: &self.edges,
    }
    .serialize(serializer)
  }
}

impl<'de> Deserialize<'de> for PipelineDag {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = PipelineDagWire::deserialize(deserializer)?;
    if wire.schema_version != PIPELINE_SNAPSHOT_SCHEMA_VERSION {
      return Err(D::Error::custom(PipelineError::UnsupportedSchema));
    }
    Self::build(wire.nodes, wire.edges, None).map_err(D::Error::custom)
  }
}

fn validate_template(template: &Value) -> Result<(), PipelineError> {
  if !template.is_object()
    || serde_json::to_vec(template)
      .map(|encoded| encoded.len() > MAX_PIPELINE_NODE_TEMPLATE_BYTES)
      .unwrap_or(true)
  {
    return Err(PipelineError::InvalidNodeTemplate);
  }
  Ok(())
}

fn topological_identities(
  nodes: &[PipelineNode],
  edges: &[PipelineEdge],
) -> Result<Vec<PipelineNodeId>, PipelineError> {
  let mut incoming: BTreeMap<_, usize> = nodes.iter().map(|node| (node.id.clone(), 0)).collect();
  let mut outgoing = BTreeMap::<PipelineNodeId, Vec<PipelineNodeId>>::new();
  for edge in edges {
    *incoming
      .get_mut(&edge.dependent)
      .expect("validated edges reference existing nodes") += 1;
    outgoing
      .entry(edge.predecessor.clone())
      .or_default()
      .push(edge.dependent.clone());
  }
  let mut ready: BTreeSet<_> = incoming
    .iter()
    .filter_map(|(identity, count)| (*count == 0).then_some(identity.clone()))
    .collect();
  let mut ordered = Vec::with_capacity(nodes.len());
  while let Some(identity) = ready.pop_first() {
    ordered.push(identity.clone());
    for dependent in outgoing.get(&identity).into_iter().flatten() {
      let count = incoming
        .get_mut(dependent)
        .expect("validated edges reference existing nodes");
      *count -= 1;
      if *count == 0 {
        ready.insert(dependent.clone());
      }
    }
  }
  if ordered.len() != nodes.len() {
    return Err(PipelineError::Cycle);
  }
  Ok(ordered)
}
