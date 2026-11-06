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
  /// Exact static command outputs and request ceiling, without invocation arguments.
  pub command: Option<FactoryBuildCommandContract>,
}
/// Safe native command compiler facts available for ordinary authoring.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBuildCommandContract {
  /// Digest of the installed invocation.
  pub task_digest: crate::FactoryDigest,
  /// Configured outputs; selecting one leaves the others as required evidence.
  pub outputs: Vec<crate::EvidenceRequirement>,
  /// Canonical observation report shape where the command owns it.
  pub observation_schema: Option<crate::FlowDataSchema>,
  /// Reviewed installation ceiling. Selecting a command does not grant it.
  pub permissions: crate::FactoryPermissionSet,
}
impl FactoryBuildCommandContract {
  /// A command with an observation contract must use its verified report as the primary output.
  pub fn accepts_primary_output(
    observation_schema: Option<&ImmutableReference>,
    kind: crate::EvidenceOutputKind,
    schema: &ImmutableReference,
  ) -> bool {
    observation_schema.is_none_or(|report| kind == crate::EvidenceOutputKind::Report && schema == report)
  }

  /// Checks the immutable invocation and output contract used by a command step.
  pub fn matches_binding(&self, binding: &FlowBuildBinding, schema: &ImmutableReference) -> bool {
    binding.task_digest == self.task_digest
      && Self::accepts_primary_output(
        self.observation_schema.as_ref().map(crate::FlowDataSchema::reference),
        binding.result_output_kind,
        schema,
      )
      && binding.matches_outputs(schema, &self.outputs)
  }
}
impl FactoryBuildPipelineContract {
  /// Older command compilers declare a fixed primary output without a Role/Profile.
  pub fn legacy_command(&self) -> Option<&FactoryBuildCompilationContract> {
    self.compilation.as_ref().filter(|compiler| !compiler.role_profiles)
  }

  /// Checks that this installed Build can execute the configured command and produce its result.
  pub fn matches_command(&self, binding: &FlowBuildBinding, schema: &ImmutableReference) -> bool {
    if let Some(command) = &self.command {
      command.matches_binding(binding, schema)
    } else {
      self.legacy_command().is_some_and(|compiler| {
        binding.task_digest == compiler.task_digest
          && binding.result_output == compiler.result_output
          && binding.result_output_kind == compiler.result_output_kind
          && binding.result_tool() == &self.tool
          && binding.evidence.is_empty()
      })
    }
  }

  /// Matches a frozen selection without depending on logical Flow node names.
  pub fn matches_binding(&self, binding: &FlowBuildBinding) -> bool {
    self.matches_toolchain(binding) && self.command_digest().is_none_or(|digest| digest == binding.task_digest)
  }

  pub(crate) fn matches_toolchain(&self, binding: &FlowBuildBinding) -> bool {
    self.selection()
      == (
        &binding.build_configuration,
        &binding.tool,
        &binding.plugin,
        &binding.model_or_tool,
      )
  }
  /// Whether another declaration would compete for this exact Build selection.
  pub(crate) fn shares_selection(&self, other: &Self) -> bool {
    self.selection() == other.selection()
      && self
        .command_digest()
        .zip(other.command_digest())
        .is_none_or(|(left, right)| left == right)
  }

  fn command_digest(&self) -> Option<crate::FactoryDigest> {
    self
      .command
      .as_ref()
      .map(|command| command.task_digest)
      .or_else(|| self.legacy_command().map(|compiler| compiler.task_digest))
  }
  fn selection(
    &self,
  ) -> (
    &BuildConfigurationRef,
    &ImmutableReference,
    &ImmutableReference,
    &ImmutableReference,
  ) {
    (&self.build_configuration, &self.tool, &self.plugin, &self.model_or_tool)
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
  /// Whether compilation requires an explicit, unconditional Task Context input.
  pub requires_task_context: bool,
  /// Primary output accepted by the installed native compiler.
  pub result_output: crate::FactoryKey,
  /// Existing publication category of the primary output.
  pub result_output_kind: crate::EvidenceOutputKind,
  /// Existing verifier required for captured MCP observations selected by a Profile.
  pub mcp_result_tool: Option<ImmutableReference>,
}
