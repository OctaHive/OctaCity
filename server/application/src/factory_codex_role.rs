//! Native adapter projection of a generic role package; unsupported settings fail closed.
use super::*;
use octacity_server_factory::{FactoryRoleEnvironmentValue, FactoryTaskMode};

pub(super) fn apply<'a>(
  role: &'a crate::FactoryRoleInvocation,
  consumer: FactoryCredentialConsumer,
  public: &mut BTreeMap<String, String>,
  secret: &mut BTreeMap<String, String>,
  variables: &mut BTreeMap<String, Value>,
) -> Result<Option<&'a str>, CodexCompilationError> {
  let inputs = role.package().inputs();
  let profile = inputs.profile.definition();
  let unsupported = CodexCompilationError::UnsupportedToolchain;
  let mode_matches = match consumer {
    FactoryCredentialConsumer::CodingNode => {
      matches!(inputs.task.mode(), FactoryTaskMode::Implement | FactoryTaskMode::Rework)
    }
    FactoryCredentialConsumer::ReadOnlyReview => {
      matches!(inputs.task.mode(), FactoryTaskMode::Observe | FactoryTaskMode::Evaluate)
    }
    FactoryCredentialConsumer::ToolReasoning => inputs.task.mode() == FactoryTaskMode::Observe,
    _ => false,
  };
  // Octa's pinned native task has a fixed CLI and no per-task MCP/integration override.
  if !mode_matches
    || profile.credential_purpose != consumer.purpose()
    || !profile.execution.cli_arguments.is_empty()
    || !profile.execution.mcp_servers.is_empty()
    || !profile.execution.integrations.is_empty()
    || profile
      .model_parameters
      .keys()
      .any(|key| key.as_str() != "reasoning_effort")
  {
    return Err(unsupported);
  }
  let effort = profile
    .model_parameters
    .values()
    .next()
    .map(|value| {
      value
        .as_str()
        .filter(|value| matches!(*value, "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"))
        .ok_or(unsupported)
    })
    .transpose()?;
  let mut names: BTreeSet<_> = public
    .keys()
    .chain(secret.keys())
    .map(|name| name.to_ascii_uppercase())
    .collect();
  for (name, value) in &profile.execution.environment {
    let upper = name.to_ascii_uppercase();
    if !names.insert(upper.clone())
      || upper.starts_with("OCTACITY_")
      || upper.starts_with("CODEX_")
      || upper.starts_with("LD_")
      || upper.starts_with("DYLD_")
      || [
        "HOME",
        "PATH",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "SYSTEMROOT",
        "COMSPEC",
        "PATHEXT",
        "NODE_OPTIONS",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
      ]
      .contains(&upper.as_str())
    {
      return Err(unsupported);
    }
    match value {
      FactoryRoleEnvironmentValue::Public { value } => {
        let variable = format!("OCTACITY_ROLE_{name}");
        public.insert(name.clone(), variable.clone());
        variables.insert(variable, json!(value));
      }
      FactoryRoleEnvironmentValue::Secret { reference } => {
        let variable = role
          .secret_variables()
          .iter()
          .find(|(_, approved)| *approved == reference)
          .map(|(variable, _)| variable)
          .ok_or(unsupported)?;
        // The installed purpose-bound credential boundary supplies this variable;
        // arbitrary new credential locators cannot be resolved by this adapter.
        if variable != CODEX_AUTH_VARIABLE {
          return Err(unsupported);
        }
        secret.insert(name.clone(), variable.clone());
      }
    }
  }
  if public.len() + secret.len() > octacity_server_factory::MAX_FACTORY_ROLE_PROFILE_ENTRIES {
    return Err(unsupported);
  }
  Ok(effort)
}
