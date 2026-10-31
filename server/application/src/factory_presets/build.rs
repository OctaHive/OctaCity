//! Preset outcomes use the shared installed compiler binding owner.
use octacity_server_factory::*;
pub(super) fn for_profile(
  choices: &FactoryConfigurationChoices,
  profile: &FactoryRoleProfile,
  node: &FlowTemplateNode,
) -> Option<FlowBuildBinding> {
  let [outcome] = node.outcomes.as_slice() else {
    return None;
  };
  crate::factory_execution_bindings::for_profile(choices, profile, &outcome.key)
}
