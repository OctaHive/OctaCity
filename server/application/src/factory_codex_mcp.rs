//! Provider adapter checks over the shared typed MCP transport and immutable pin.
use crate::CodexCompilationError;
pub use octacity_server_factory::FactoryMcpServer as FactoryCodexMcpServer;
use octacity_server_factory::FactoryRoleIntegrationBinding;
use octacity_server_factory::{FactoryKey, FactoryMcpConfiguration, ImmutableReference};
/// Compatibility adapter retaining the Codex compilation error and wire contracts.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct FactoryCodexMcpConfiguration(FactoryMcpConfiguration);
impl TryFrom<FactoryMcpConfiguration> for FactoryCodexMcpConfiguration {
  type Error = CodexCompilationError;
  fn try_from(value: FactoryMcpConfiguration) -> Result<Self, Self::Error> {
    value
      .validate()
      .map_err(|_| CodexCompilationError::UnsupportedToolchain)?;
    Ok(Self(value))
  }
}

// Both preparation and retained observation reconstruct the same exact compiler document.
pub(crate) async fn configurations(
  installed: &[FactoryCodexMcpConfiguration],
  role: Option<&crate::FactoryRoleInvocation>,
  store: Option<&dyn octacity_server_store::FactoryRuntimeSettingsStore>,
) -> Result<Vec<FactoryCodexMcpConfiguration>, crate::ApplicationError> {
  let mut configurations = installed.to_vec();
  if let Some(role) = role {
    let profile = &role.package().inputs().profile;
    let missing = profile
      .definition()
      .execution
      .mcp_servers
      .values()
      .map(|binding| &binding.configuration)
      .filter(|reference| !configurations.iter().any(|value| value.reference() == *reference))
      .cloned()
      .collect::<std::collections::BTreeSet<_>>()
      .into_iter()
      .collect::<Vec<_>>();
    if !missing.is_empty() {
      let mut restored = store
        .ok_or_else(crate::ApplicationError::capability_unavailable)?
        .factory_runtime_mcp_configurations(profile.reference().project_id, &missing)
        .await?;
      for reference in missing {
        let configuration = restored
          .remove(&reference)
          .filter(|value| value.reference() == &reference)
          .ok_or_else(crate::ApplicationError::capability_unavailable)?;
        configurations.push(
          configuration
            .try_into()
            .map_err(|_| crate::ApplicationError::invalid())?,
        );
      }
    }
  }
  Ok(configurations)
}
impl FactoryCodexMcpConfiguration {
  /// Reuses common transport pinning without a provider-specific content format.
  pub fn new(
    identity: FactoryKey,
    version: FactoryKey,
    server: FactoryCodexMcpServer,
    integration: Option<FactoryRoleIntegrationBinding>,
  ) -> Result<Self, CodexCompilationError> {
    FactoryMcpConfiguration::new(identity, version, server, integration)
      .map(Self)
      .map_err(|_| CodexCompilationError::UnsupportedToolchain)
  }
  /// Exact shared transport pin.
  pub const fn reference(&self) -> &ImmutableReference {
    self.0.reference()
  }
  /// Verifies retained bytes through the common contract.
  pub fn validate(&self) -> Result<(), CodexCompilationError> {
    self
      .0
      .validate()
      .map_err(|_| CodexCompilationError::UnsupportedToolchain)
  }
}

pub(crate) fn resolve(
  configuration: &FactoryCodexMcpConfiguration,
  integration: Option<&FactoryRoleIntegrationBinding>,
  declared: &std::collections::BTreeMap<String, octacity_server_factory::FactoryRoleEnvironmentValue>,
  public: &std::collections::BTreeMap<String, String>,
  secret: &std::collections::BTreeMap<String, String>,
) -> Result<FactoryCodexMcpServer, CodexCompilationError> {
  use CodexCompilationError::UnsupportedToolchain as Invalid;
  configuration.validate().map_err(|_| Invalid)?;
  if configuration.0.integration() != integration {
    return Err(Invalid);
  }
  crate::factory_mcp::validate_transport(configuration.0.server()).map_err(|_| Invalid)?;
  match configuration.0.server() {
    FactoryCodexMcpServer::Http {
      bearer_token_env_var, ..
    } => {
      if bearer_token_env_var.as_ref().is_some_and(|name| {
        !matches!(
          declared.get(name),
          Some(octacity_server_factory::FactoryRoleEnvironmentValue::Secret { .. })
        ) || !secret.contains_key(name)
      }) {
        return Err(Invalid);
      }
    }
    FactoryCodexMcpServer::Stdio { env_vars, .. } => {
      if env_vars
        .iter()
        .any(|name| !declared.contains_key(name) || (!public.contains_key(name) && !secret.contains_key(name)))
      {
        return Err(Invalid);
      }
    }
  }
  Ok(configuration.0.server().clone())
}
