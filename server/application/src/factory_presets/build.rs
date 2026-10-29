//! Exact native compiler facts supply bindings; preset outcomes retain their existing owner.
use octacity_server_factory::*;

pub(super) fn for_profile(
  choices: &FactoryConfigurationChoices,
  profile: &FactoryRoleProfile,
  node: &FlowTemplateNode,
) -> Option<FlowBuildBinding> {
  let [outcome] = node.outcomes.as_slice() else {
    return None;
  };
  let definition = profile.definition();
  let mut candidates = choices.build_pipeline_contracts().iter().filter_map(|installed| {
    if installed.build_configuration != definition.build_configuration
      || installed.tool != definition.toolchain.executable
      || installed.plugin != definition.toolchain.plugin
      || installed.model_or_tool != definition.toolchain.model
    {
      return None;
    }
    let compilation = installed.compilation.as_ref()?;
    let result_tool = if definition.execution.mcp_servers.is_empty() {
      None
    } else {
      Some(compilation.mcp_result_tool.clone()?)
    };
    let build = FlowBuildBinding {
      tool: installed.tool.clone(),
      plugin: installed.plugin.clone(),
      model_or_tool: installed.model_or_tool.clone(),
      build_configuration: installed.build_configuration.clone(),
      task_digest: if compilation.role_profiles {
        crate::factory_role_task_digest(compilation.task_digest, profile)
      } else {
        compilation.task_digest
      },
      result_output: compilation.result_output.clone(),
      result_output_kind: compilation.result_output_kind,
      result_outcome: outcome.key.clone(),
      result_tool,
      capture_write_paths: None,
      capture_deliverables: None,
      evidence: vec![],
    };
    choices.supports_build_binding(&build).then_some(build)
  });
  let first = candidates.next()?;
  candidates.next().is_none().then_some(first)
}
