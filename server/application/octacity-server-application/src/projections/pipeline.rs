use octacity_server_domain::{
  JobName, PipelineId, PipelineName, PipelineNodeId, PipelineVersion, ProjectId, Timestamp,
};
use octacity_server_job::JobExecutionTemplate;
use octacity_server_pipeline::{DependencyPolicy, ExecutionCapability};
use octacity_server_store::PublishedPipeline;
use serde::Serialize;

use super::ProjectionError;

/// Safe application view of one immutable Pipeline node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PipelineNodeProjection {
  /// Stable node identity within the Pipeline version.
  pub id: PipelineNodeId,
  /// Operator-facing Job name.
  pub name: JobName,
  /// Immutable fan-in and failure-propagation policy.
  pub dependency_policy: DependencyPolicy,
  /// Execution capabilities required specifically by this node.
  pub required_capabilities: Vec<ExecutionCapability>,
  /// Strict repository-controlled execution fields.
  pub execution: JobExecutionTemplate,
}

/// One directed Pipeline dependency.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PipelineEdgeProjection {
  /// Direct predecessor node.
  pub predecessor: PipelineNodeId,
  /// Node whose readiness depends on the predecessor.
  pub dependent: PipelineNodeId,
}

/// Safe application projection of one immutable Pipeline version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PipelineProjection {
  /// Stable Pipeline identity.
  pub id: PipelineId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Operator-facing Pipeline name.
  pub name: PipelineName,
  /// Exact immutable Pipeline version.
  pub version: PipelineVersion,
  /// Canonically ordered Pipeline nodes.
  pub nodes: Vec<PipelineNodeProjection>,
  /// Canonically ordered dependency edges.
  pub edges: Vec<PipelineEdgeProjection>,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl TryFrom<PublishedPipeline> for PipelineProjection {
  type Error = ProjectionError;

  fn try_from(pipeline: PublishedPipeline) -> Result<Self, Self::Error> {
    let nodes = pipeline
      .dag
      .nodes()
      .iter()
      .map(|node| {
        serde_json::from_value(node.template().clone())
          .map(|execution| PipelineNodeProjection {
            id: node.id().clone(),
            name: node.name().clone(),
            dependency_policy: node.dependency_policy(),
            required_capabilities: node.required_capabilities().to_vec(),
            execution,
          })
          .map_err(|_| ProjectionError::InvalidPipelineTemplate)
      })
      .collect::<Result<_, _>>()?;
    let edges = pipeline
      .dag
      .edges()
      .iter()
      .map(|edge| PipelineEdgeProjection {
        predecessor: edge.predecessor().clone(),
        dependent: edge.dependent().clone(),
      })
      .collect();
    Ok(Self {
      id: pipeline.id,
      project_id: pipeline.project_id,
      name: pipeline.name,
      version: pipeline.version,
      nodes,
      edges,
      published_at: pipeline.published_at,
    })
  }
}
