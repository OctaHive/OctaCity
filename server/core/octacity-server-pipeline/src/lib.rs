//! Immutable Pipeline versions, DAG validation, and dependency policy.
//!
//! Attempt materialization derives server Jobs from validated snapshots owned
//! by this module. Construction canonicalizes node, edge, capability, and JSON
//! ordering, so callers cannot publish a partially validated graph.
//!
//! A **Pipeline** is the immutable DAG definition. An **Attempt** is one
//! numbered materialization of a Pipeline into server Jobs; neither term means
//! Build or retry. See the [canonical glossary] and [server ownership guide].
//!
//! [canonical glossary]: https://github.com/OctaHive/OctaCity/blob/main/CONTEXT.md
//! [server ownership guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/server-architecture.md

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod capability;
mod dag;
mod error;
mod policy;

pub use capability::{CapabilityCatalog, ExecutionCapability, MAX_EXECUTION_CAPABILITY_BYTES};
pub use dag::{
  MAX_PIPELINE_EDGES, MAX_PIPELINE_FAN_IN, MAX_PIPELINE_FAN_OUT, MAX_PIPELINE_NODE_TEMPLATE_BYTES, MAX_PIPELINE_NODES,
  MAX_PIPELINE_SNAPSHOT_BYTES, PipelineDag, PipelineEdge, PipelineNode, PublishablePipelineDag,
};
pub use error::PipelineError;
pub use policy::{DependencyDecision, DependencyOutcome, DependencyPolicy};
