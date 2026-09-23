use octacity_server_domain::{
  JobName, PipelineId, PipelineName, PipelineNodeId, PipelineVersion, ProjectId, Timestamp,
};
use octacity_server_pipeline::ExecutionCapability;
use octacity_server_store::PublishedPipeline;
use serde::{Deserialize, Serialize};

use super::ProjectionError;

/// Immutable fan-in policy exposed by the application boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyPolicyProjection {
  /// Every predecessor must succeed.
  AllSucceeded,
  /// Every predecessor must finish.
  AllCompleted,
  /// At least one predecessor must succeed.
  AnySucceeded,
}

/// Repository-controlled execution fields exposed by the application boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JobExecutionProjection {
  /// Optional workspace-relative Octafile path.
  pub octafile: Option<String>,
  /// Non-empty task names.
  pub commands: Vec<String>,
  /// Positional task arguments.
  pub arguments: Vec<String>,
  /// Optional maximum task concurrency.
  pub concurrency: Option<usize>,
  /// Whether independent root commands may run concurrently.
  pub parallel: bool,
  /// Whether scheduling stops after the first failure.
  pub failfast: bool,
}

/// Safe application view of one immutable Pipeline node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PipelineNodeProjection {
  /// Stable node identity within the Pipeline version.
  pub id: PipelineNodeId,
  /// Operator-facing Job name.
  pub name: JobName,
  /// Immutable fan-in and failure-propagation policy.
  pub dependency_policy: DependencyPolicyProjection,
  /// Execution capabilities required specifically by this node.
  pub required_capabilities: Vec<ExecutionCapability>,
  /// Strict repository-controlled execution fields.
  pub execution: JobExecutionProjection,
}

/// One directed Pipeline dependency.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PipelineEdgeProjection {
  /// Direct predecessor node.
  pub predecessor: PipelineNodeId,
  /// Node whose readiness depends on the predecessor.
  pub dependent: PipelineNodeId,
}

/// Safe application projection of one immutable Pipeline version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
        serde_json::from_value::<octacity_server_job::JobExecutionTemplate>(node.template().clone())
          .map(|execution| PipelineNodeProjection {
            id: node.id().clone(),
            name: node.name().clone(),
            dependency_policy: match node.dependency_policy() {
              octacity_server_pipeline::DependencyPolicy::AllSucceeded => DependencyPolicyProjection::AllSucceeded,
              octacity_server_pipeline::DependencyPolicy::AllCompleted => DependencyPolicyProjection::AllCompleted,
              octacity_server_pipeline::DependencyPolicy::AnySucceeded => DependencyPolicyProjection::AnySucceeded,
            },
            required_capabilities: node.required_capabilities().to_vec(),
            execution: JobExecutionProjection {
              octafile: execution.octafile,
              commands: execution.commands,
              arguments: execution.arguments,
              concurrency: execution.concurrency.map(std::num::NonZeroUsize::get),
              parallel: execution.parallel,
              failfast: execution.failfast,
            },
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
