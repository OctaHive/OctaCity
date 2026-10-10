use crate::{FactoryDigest, FactoryKey, FlowDefinitionRef, ImmutableReference};
use serde::{Deserialize, Serialize};

/// Exact tool/model/task selection for one externally executed Flow node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowBuildProfile {
  /// Exact definition containing the node.
  pub definition: FlowDefinitionRef,
  /// Stable node key in that definition.
  pub node: FactoryKey,
  /// Exact selected harness or deterministic tool.
  pub tool: ImmutableReference,
  /// Exact runner plugin.
  pub plugin: ImmutableReference,
  /// Exact model or command configuration.
  pub model_or_tool: ImmutableReference,
  /// Exact task/prompt contract.
  pub task_digest: FactoryDigest,
  /// Exact ordinary Build configuration that materializes this node's Job DAG.
  pub build_configuration: crate::BuildConfigurationRef,
  /// Declared logical output containing typed, non-authoritative observations.
  pub result_output: FactoryKey,
}
