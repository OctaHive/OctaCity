//! Installed Codex transport data; role definitions keep only immutable references.
use crate::CodexCompilationError;
use octacity_server_factory::{FactoryDigest, FactoryKey, FactoryRoleIntegrationBinding, ImmutableReference};
use serde::{Deserialize, Serialize};

// Transport limits of the pinned Codex task, independent of generic role limits.
const MAX_MCP_SELECTION_ENTRIES: usize = 64;
const MAX_MCP_TOOL_NAME_BYTES: usize = 256;

/// Exact explicit MCP settings accepted by the pinned native plugin.
/// Authentication fields contain child environment names, never credential values.
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryCodexMcpServer {
  /// Installed HTTP service; Project connection authorization stays with its existing owner.
  Http {
    /// Reviewed endpoint, independent of instruction content.
    url: String,
    /// Name of an independently approved secret environment mapping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bearer_token_env_var: Option<String>,
    /// Optional finite tool selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled_tools: Option<Vec<String>>,
  },
  /// Installed local process, still constrained by ordinary command and mount permissions.
  Stdio {
    /// Absolute executable path in the qualified backend image.
    command: String,
    /// Exact argv, without a shell.
    #[serde(default)]
    args: Vec<String>,
    /// Explicit already mapped child environment names.
    #[serde(default)]
    env_vars: Vec<String>,
    /// Optional finite tool selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled_tools: Option<Vec<String>>,
  },
}
impl FactoryCodexMcpServer {
  fn validate_selection_lists(&self) -> Result<(), CodexCompilationError> {
    use CodexCompilationError::UnsupportedToolchain as Invalid;
    let tools = match self {
      Self::Stdio {
        env_vars,
        enabled_tools,
        ..
      } => {
        if !bounded_unique_selection(env_vars) {
          return Err(Invalid);
        }
        enabled_tools
      }
      Self::Http { enabled_tools, .. } => enabled_tools,
    };
    if tools.as_ref().is_some_and(|tools| {
      !bounded_unique_selection(tools)
        || tools.iter().any(|name| {
          name.is_empty()
            || name.len() > MAX_MCP_TOOL_NAME_BYTES
            || name.trim() != name
            || name.chars().any(char::is_control)
        })
    }) {
      return Err(Invalid);
    }
    Ok(())
  }
}

fn bounded_unique_selection(values: &[String]) -> bool {
  values.len() <= MAX_MCP_SELECTION_ENTRIES
    && values.iter().collect::<std::collections::BTreeSet<_>>().len() == values.len()
}

/// Private deployment metadata bound to one exact transport and optional managed connection.
/// Keeping immutable older entries allows retained invocations to recover after authoring changes.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCodexMcpConfiguration {
  reference: ImmutableReference,
  server: FactoryCodexMcpServer,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  integration: Option<FactoryRoleIntegrationBinding>,
}
impl FactoryCodexMcpConfiguration {
  /// Pins the transport bytes and exact existing Project connection selection together.
  pub fn new(
    identity: FactoryKey,
    version: FactoryKey,
    server: FactoryCodexMcpServer,
    integration: Option<FactoryRoleIntegrationBinding>,
  ) -> Result<Self, CodexCompilationError> {
    server.validate_selection_lists()?;
    let bytes = serde_json::to_vec(&(&server, &integration)).map_err(|_| CodexCompilationError::Serialization)?;
    if bytes.len() > 64 * 1024 {
      return Err(CodexCompilationError::UnsupportedToolchain);
    }
    Ok(Self {
      reference: ImmutableReference::new(identity, version, FactoryDigest::content_sha256(&bytes)),
      server,
      integration,
    })
  }
  /// Exact pin selected by the provider-independent Role/Profile contract.
  pub const fn reference(&self) -> &ImmutableReference {
    &self.reference
  }
  /// Rejects substituted deployment bytes before this configuration can be installed.
  pub fn validate(&self) -> Result<(), CodexCompilationError> {
    let expected = Self::new(
      self.reference.identity().clone(),
      self.reference.version().clone(),
      self.server.clone(),
      self.integration.clone(),
    )?;
    if expected.reference != self.reference {
      return Err(CodexCompilationError::UnsupportedToolchain);
    }
    Ok(())
  }
  pub(crate) fn resolve(
    &self,
    integration: Option<&FactoryRoleIntegrationBinding>,
    declared: &std::collections::BTreeMap<String, octacity_server_factory::FactoryRoleEnvironmentValue>,
    public: &std::collections::BTreeMap<String, String>,
    secret: &std::collections::BTreeMap<String, String>,
  ) -> Result<FactoryCodexMcpServer, CodexCompilationError> {
    use CodexCompilationError::UnsupportedToolchain as Invalid;
    self.validate()?;
    if self.integration.as_ref() != integration {
      return Err(Invalid);
    }
    match &self.server {
      FactoryCodexMcpServer::Http {
        url,
        bearer_token_env_var,
        ..
      } => {
        let url = url::Url::parse(url).map_err(|_| Invalid)?;
        if !matches!(url.scheme(), "http" | "https")
          || url.host_str().is_none()
          || !url.username().is_empty()
          || url.password().is_some()
          || url.fragment().is_some()
          || bearer_token_env_var.as_ref().is_some_and(|name| {
            url.scheme() != "https"
              || !matches!(
                declared.get(name),
                Some(octacity_server_factory::FactoryRoleEnvironmentValue::Secret { .. })
              )
              || !secret.contains_key(name)
          })
        {
          return Err(Invalid);
        }
      }
      FactoryCodexMcpServer::Stdio {
        command,
        args,
        env_vars,
        ..
      } => {
        if !command.starts_with('/')
          || command.contains('\0')
          || args.len() > 64
          || args.iter().any(|arg| arg.len() > 4096 || arg.contains('\0'))
          || env_vars
            .iter()
            .any(|name| !declared.contains_key(name) || (!public.contains_key(name) && !secret.contains_key(name)))
        {
          return Err(Invalid);
        }
      }
    }
    Ok(self.server.clone())
  }
}
