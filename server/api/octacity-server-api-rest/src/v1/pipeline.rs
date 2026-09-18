use serde::{Deserialize, Serialize};

/// Immutable dependency and failure-propagation policy for one Pipeline node.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyPolicy {
  /// Every predecessor must succeed.
  AllSucceeded,
  /// Every predecessor must finish, regardless of outcome.
  AllCompleted,
  /// At least one predecessor must succeed.
  AnySucceeded,
}

/// Repository-controlled execution fields accepted for one Pipeline node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobExecution {
  /// Optional workspace-relative Octafile path.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub octafile: Option<String>,
  /// Non-empty Octa task names to execute.
  pub commands: Vec<String>,
  /// Positional runtime-template values.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub arguments: Vec<String>,
  /// Optional maximum task concurrency.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub concurrency: Option<usize>,
  /// Whether independent root commands may run concurrently.
  #[serde(default)]
  pub parallel: bool,
  /// Whether scheduling stops after the first failure.
  #[serde(default)]
  pub failfast: bool,
}

/// One immutable Job template inside a Pipeline version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineNode {
  /// Stable node identity within the Pipeline.
  pub id: String,
  /// Operator-facing Job name.
  pub name: String,
  /// Fan-in and failure-propagation policy.
  pub dependency_policy: DependencyPolicy,
  /// Execution capabilities required specifically by this node.
  pub required_capabilities: Vec<String>,
  /// Strict repository-controlled execution fields.
  pub execution: JobExecution,
}

/// One directed Pipeline dependency.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineEdge {
  /// Direct predecessor node identity.
  pub predecessor: String,
  /// Node whose readiness depends on the predecessor.
  pub dependent: String,
}

/// Complete immutable Pipeline DAG supplied or returned over REST.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineDag {
  /// Canonically ordered Job nodes.
  pub nodes: Vec<PipelineNode>,
  /// Canonically ordered dependency edges.
  pub edges: Vec<PipelineEdge>,
}

/// Request body for creating a Pipeline and its first immutable version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePipelineRequest {
  /// Existing owning Project identity.
  pub project_id: String,
  /// Project-local Pipeline name.
  pub name: String,
  /// Validated immutable initial DAG.
  pub dag: PipelineDag,
}

/// Request body for appending the next immutable Pipeline version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishPipelineVersionRequest {
  /// Complete replacement DAG for the new immutable version.
  pub dag: PipelineDag,
}

/// REST representation of one exact immutable Pipeline version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineResource {
  /// Stable Pipeline identity shared by all versions.
  pub id: String,
  /// Owning Project identity.
  pub project_id: String,
  /// Project-local Pipeline name.
  pub name: String,
  /// Exact positive immutable version.
  pub version: u64,
  /// Complete immutable DAG.
  pub dag: PipelineDag,
  /// Authoritative publication time as Unix milliseconds.
  pub published_at_unix_ms: i64,
}
