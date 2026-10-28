//! Native adapter projection of a generic role package; unsupported settings fail closed.
use super::*;
use octacity_server_factory::{FactoryRoleEnvironmentValue, FactoryTaskMode};

// The pinned native task limits MCP aliases more narrowly than FactoryKey.

pub(super) fn apply<'a>(
  role: &'a crate::FactoryRoleInvocation,
  consumer: FactoryCredentialConsumer,
  public: &mut BTreeMap<String, String>,
  secret: &mut BTreeMap<String, String>,
  variables: &mut BTreeMap<String, Value>,
  mcp_credentials: &BTreeMap<String, String>,
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
  // Only additive flags can cross this adapter; execution authority stays installed.
  if !mode_matches
    || profile.credential_purpose != consumer.purpose()
    || profile.execution.cli_arguments.iter().any(|argument| {
      !matches!(
        argument.as_str(),
        "--ephemeral" | "--skip-git-repo-check" | "--color=never" | "--color=always" | "--color=auto"
      )
    })
    || profile.execution.cli_arguments.iter().collect::<BTreeSet<_>>().len() != profile.execution.cli_arguments.len()
    || profile
      .execution
      .cli_arguments
      .iter()
      .filter(|argument| argument.starts_with("--color="))
      .count()
      > 1
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
    .map(|value| value.as_str().ok_or(unsupported))
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
        // The native client forwards these bootstrap variables to every stdio
        // peer, independently of the selected peer's explicit env_vars.
        if [
          "LOGNAME",
          "SHELL",
          "USER",
          "__CF_USER_TEXT_ENCODING",
          "LANG",
          "LC_ALL",
          "TERM",
          "TMPDIR",
          "TZ",
          "WINDIR",
          "SYSTEMDRIVE",
          "USERNAME",
          "USERDOMAIN",
          "HOMEDRIVE",
          "HOMEPATH",
          "PROGRAMFILES",
          "PROGRAMFILES(X86)",
          "PROGRAMW6432",
          "PROGRAMDATA",
          "TEMP",
          "TMP",
          "POWERSHELL",
          "PWSH",
          "REQUESTS_CA_BUNDLE",
          "CURL_CA_BUNDLE",
          "NODE_EXTRA_CA_CERTS",
          "GIT_SSL_CAINFO",
          "CARGO_HTTP_CAINFO",
          "PIP_CERT",
          "BUNDLE_SSL_CA_CERT",
          "NPM_CONFIG_CAFILE",
        ]
        .contains(&upper.as_str())
        {
          return Err(unsupported);
        }
        let variable = role
          .secret_variables()
          .iter()
          .find(|(_, approved)| *approved == reference)
          .map(|(variable, _)| variable)
          .ok_or(unsupported)?;
        // The installed purpose-bound credential boundary supplies this variable;
        // arbitrary new credential locators cannot be resolved by this adapter.
        if variable != CODEX_AUTH_VARIABLE && !mcp_credentials.contains_key(variable) {
          return Err(unsupported);
        }
        variables.insert(variable.clone(), json!({"required":true,"secret":true}));
        secret.insert(name.clone(), variable.clone());
      }
    }
  }
  if public.len() + secret.len() > octacity_server_factory::MAX_FACTORY_ROLE_PROFILE_ENTRIES {
    return Err(unsupported);
  }
  Ok(effort)
}

pub(super) fn mcp(
  role: &crate::FactoryRoleInvocation,
  configurations: &[crate::FactoryCodexMcpConfiguration],
  public: &BTreeMap<String, String>,
  secret: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, crate::FactoryCodexMcpServer>, CodexCompilationError> {
  let execution = &role.package().inputs().profile.definition().execution;
  let invalid = CodexCompilationError::UnsupportedToolchain;
  let mut used = BTreeSet::new();
  let mut servers = BTreeMap::new();
  for (name, binding) in &execution.mcp_servers {
    octacity_server_factory::validate_factory_mcp_server_name(name.as_str()).map_err(|_| invalid)?;
    let installed = configurations
      .iter()
      .find(|config| config.reference() == &binding.configuration)
      .ok_or(invalid)?;
    let integration = binding
      .integration
      .as_ref()
      .map(|alias| {
        used.insert(alias);
        execution.integrations.get(alias).ok_or(invalid)
      })
      .transpose()?;
    servers.insert(
      name.as_str().to_owned(),
      crate::factory_codex_mcp::resolve(installed, integration, &execution.environment, public, secret)?,
    );
  }
  if used.len() != execution.integrations.len() {
    return Err(invalid);
  }
  // An explicit empty selection tells the native client to disable ambient MCP
  // configuration too. Absence is reserved for the legacy non-Role compiler path.
  Ok(servers)
}
