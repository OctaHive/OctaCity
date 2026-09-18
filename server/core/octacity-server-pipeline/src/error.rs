use octacity_server_domain::PipelineNodeId;
use thiserror::Error;

use crate::ExecutionCapability;

/// Stable validation failure returned while constructing a Pipeline DAG.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PipelineError {
  /// An execution-capability identity is malformed or outside its byte bound.
  #[error("an execution capability is invalid")]
  InvalidCapability,
  /// A Job template must be a bounded JSON object.
  #[error("a pipeline node template must be a bounded JSON object")]
  InvalidNodeTemplate,
  /// A Pipeline must contain at least one Job node.
  #[error("a pipeline must contain at least one node")]
  Empty,
  /// A Pipeline contains more Job nodes than one immutable version permits.
  #[error("a pipeline contains too many nodes")]
  TooManyNodes,
  /// A Pipeline contains more dependency edges than one immutable version permits.
  #[error("a pipeline contains too many edges")]
  TooManyEdges,
  /// Two Job templates use the same stable node identity.
  #[error("pipeline node {node_id} is duplicated")]
  DuplicateNode {
    /// Repeated node identity.
    node_id: PipelineNodeId,
  },
  /// One Job template repeats the same required capability.
  #[error("pipeline node {node_id} repeats an execution capability")]
  DuplicateCapability {
    /// Node containing the duplicate requirement.
    node_id: PipelineNodeId,
  },
  /// A Job template requests a capability absent from the publication catalog.
  #[error("pipeline node {node_id} requests unavailable capability {capability}")]
  UnavailableCapability {
    /// Node containing the unsupported requirement.
    node_id: PipelineNodeId,
    /// Unsupported capability identity.
    capability: ExecutionCapability,
  },
  /// A dependency edge is repeated.
  #[error("pipeline dependency edge is duplicated")]
  DuplicateEdge,
  /// A node cannot depend directly on itself.
  #[error("pipeline node {node_id} depends on itself")]
  SelfDependency {
    /// Self-referencing node.
    node_id: PipelineNodeId,
  },
  /// A dependency edge references a node absent from the version.
  #[error("pipeline dependency references missing node {node_id}")]
  MissingNode {
    /// Missing predecessor or dependent identity.
    node_id: PipelineNodeId,
  },
  /// One node exceeds the supported number of direct predecessors.
  #[error("pipeline node {node_id} exceeds the fan-in limit")]
  FanInExceeded {
    /// Node with excessive fan-in.
    node_id: PipelineNodeId,
  },
  /// One node exceeds the supported number of direct dependents.
  #[error("pipeline node {node_id} exceeds the fan-out limit")]
  FanOutExceeded {
    /// Node with excessive fan-out.
    node_id: PipelineNodeId,
  },
  /// The dependency graph contains a directed cycle.
  #[error("a pipeline dependency graph must be acyclic")]
  Cycle,
  /// The encoded immutable snapshot exceeds its aggregate byte bound.
  #[error("a pipeline snapshot exceeds its encoded byte bound")]
  SnapshotTooLarge,
  /// A stored Pipeline snapshot uses an unsupported schema version.
  #[error("a pipeline snapshot schema version is unsupported")]
  UnsupportedSchema,
}
