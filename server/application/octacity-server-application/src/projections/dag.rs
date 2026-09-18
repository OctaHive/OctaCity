use std::collections::BTreeMap;

use octacity_server_domain::{AttemptId, JobId, PipelineNodeId};
use octacity_server_job::JobState;
use octacity_server_orchestrator::{JobGraphNode, validate_job_graph};
use octacity_server_pipeline::DependencyPolicy;
use serde::Serialize;

use super::{JobFailureClassification, JobProjection, ProjectionError};

/// One Job node in an Attempt's diagnostic DAG.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DagNodeProjection {
  /// Materialized Job identity.
  pub job_id: JobId,
  /// Immutable Pipeline node identity.
  pub pipeline_node_id: PipelineNodeId,
  /// Current Job state.
  pub state: JobState,
  /// Safe terminal failure classification, when present.
  pub failure: Option<JobFailureClassification>,
}

/// One causal edge from a predecessor Job to a dependent Job.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct DagEdgeProjection {
  /// Direct predecessor Job.
  pub predecessor_job_id: JobId,
  /// Job whose readiness depends on the predecessor.
  pub dependent_job_id: JobId,
  /// Fan-in and failure policy applied by the dependent Job.
  pub dependency_policy: DependencyPolicy,
}

/// Complete diagnostic DAG causality for one Attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DagCausalityProjection {
  /// Attempt whose materialized graph is represented.
  pub attempt_id: AttemptId,
  /// Jobs ordered by stable identity.
  pub nodes: Vec<DagNodeProjection>,
  /// Dependency edges ordered by predecessor then dependent identity.
  pub edges: Vec<DagEdgeProjection>,
}

impl DagCausalityProjection {
  /// Builds and validates one self-contained acyclic graph from Job projections.
  pub fn new(attempt_id: AttemptId, jobs: &[JobProjection]) -> Result<Self, ProjectionError> {
    let mut by_id = BTreeMap::new();
    for job in jobs {
      if job.attempt_id != attempt_id || by_id.insert(job.id, job).is_some() {
        return Err(ProjectionError::InvalidDagCausality);
      }
    }
    let graph = jobs
      .iter()
      .map(|job| JobGraphNode::new(job.id, job.state, job.dependencies.clone(), job.dependency_policy))
      .collect::<Result<Vec<_>, _>>()
      .map_err(|_| ProjectionError::InvalidDagCausality)?;
    validate_job_graph(graph).map_err(|_| ProjectionError::InvalidDagCausality)?;
    let mut edges = Vec::new();
    for job in jobs {
      for dependency in &job.dependencies {
        edges.push(DagEdgeProjection {
          predecessor_job_id: *dependency,
          dependent_job_id: job.id,
          dependency_policy: job.dependency_policy,
        });
      }
    }
    edges.sort_unstable_by_key(|edge| (edge.predecessor_job_id, edge.dependent_job_id));
    let nodes = by_id
      .into_values()
      .map(|job| DagNodeProjection {
        job_id: job.id,
        pipeline_node_id: job.pipeline_node_id.clone(),
        state: job.state,
        failure: job.terminal.and_then(|terminal| terminal.failure),
      })
      .collect();
    Ok(Self {
      attempt_id,
      nodes,
      edges,
    })
  }
}
