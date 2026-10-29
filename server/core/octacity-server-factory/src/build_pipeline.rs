//! Installation-owned ordinary Pipeline shape, shared with native materialization.
use crate::{BuildConfigurationRef, FlowBuildBinding, ImmutableReference};
use octacity_server_domain::PipelineNodeId;
/// Exact single-job contract declared by an installed Build compiler, never a client grant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBuildPipelineContract {
  /// Immutable ordinary Build selection.
  pub build_configuration: BuildConfigurationRef,
  /// Exact selected compiler/executable capability.
  pub tool: ImmutableReference,
  /// Exact task plugin.
  pub plugin: ImmutableReference,
  /// Exact installed model or command configuration.
  pub model_or_tool: ImmutableReference,
  /// Sole ordinary Job accepted by this compiler.
  pub pipeline_node: PipelineNodeId,
  /// Optional exact compiler facts projected from the same native installation.
  pub compilation: Option<FactoryBuildCompilationContract>,
}
impl FactoryBuildPipelineContract {
  /// Matches a frozen selection without depending on logical Flow node names.
  pub fn matches_binding(&self, binding: &FlowBuildBinding) -> bool {
    self.build_configuration == binding.build_configuration
      && self.tool == binding.tool
      && self.plugin == binding.plugin
      && self.model_or_tool == binding.model_or_tool
  }
  /// Pure shape check used before metadata readiness and before actual materialization.
  pub fn matches_nodes<'a>(&self, mut nodes: impl Iterator<Item = &'a PipelineNodeId>) -> bool {
    nodes.next() == Some(&self.pipeline_node) && nodes.next().is_none()
  }
}

/// Safe compiler-owned selections; a preset supplies its own logical result outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBuildCompilationContract {
  /// Existing native task identity before an optional exact Role/Profile binding.
  pub task_digest: crate::FactoryDigest,
  /// Whether the native compiler binds this task to an exact Role/Profile reference.
  pub role_profiles: bool,
  /// Primary output accepted by the installed native compiler.
  pub result_output: crate::FactoryKey,
  /// Existing publication category of the primary output.
  pub result_output_kind: crate::EvidenceOutputKind,
  /// Existing verifier required for captured MCP observations selected by a Profile.
  pub mcp_result_tool: Option<ImmutableReference>,
}
