//! Shared structured MCP transports and immutable pins; adapters own execution and secret resolution.
use crate::{FactoryDigest, FactoryError, FactoryKey, FactoryRoleIntegrationBinding, ImmutableReference};
use serde::{Deserialize, Serialize};

// Shared finite transport selection bounds, also used by the existing compiler.
/// Maximum explicit tool or environment names in one MCP transport.
pub const MAX_FACTORY_MCP_SELECTION_ENTRIES: usize = 64;
/// Maximum UTF-8 bytes in one MCP tool name.
pub const MAX_FACTORY_MCP_TOOL_NAME_BYTES: usize = 256;
/// Maximum serialized transport and integration pinning document.
pub const MAX_FACTORY_MCP_CONFIGURATION_BYTES: usize = 64 * 1024;
/// Maximum bytes in one explicit MCP process argument.
pub const MAX_FACTORY_MCP_ARGUMENT_BYTES: usize = 4096;
/// Shared named-server execution namespace bound.
pub const MAX_FACTORY_MCP_SERVER_NAME_BYTES: usize = 64;

/// Explicit MCP transport declaration; support is independently qualified by installation.
/// Authentication fields contain child environment names, never credential values.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryMcpServer {
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
impl FactoryMcpServer {
  fn validate_selection_lists(&self) -> Result<(), FactoryError> {
    let invalid = || FactoryError::InvalidConfiguration {
      field: "MCP configuration",
    };
    let tools = match self {
      Self::Stdio {
        env_vars,
        enabled_tools,
        ..
      } => {
        if !bounded_unique_selection(env_vars) {
          return Err(invalid());
        }
        enabled_tools
      }
      Self::Http { enabled_tools, .. } => enabled_tools,
    };
    if tools.as_ref().is_some_and(|tools| {
      !bounded_unique_selection(tools)
        || tools.iter().any(|name| {
          name.is_empty()
            || name.len() > MAX_FACTORY_MCP_TOOL_NAME_BYTES
            || name.trim() != name
            || name.chars().any(char::is_control)
        })
    }) {
      return Err(invalid());
    }
    Ok(())
  }
}

fn bounded_unique_selection(values: &[String]) -> bool {
  values.len() <= MAX_FACTORY_MCP_SELECTION_ENTRIES
    && values.iter().collect::<std::collections::BTreeSet<_>>().len() == values.len()
}

/// Private deployment metadata bound to one exact transport and optional managed connection.
/// Keeping immutable older entries allows retained invocations to recover after authoring changes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryMcpConfiguration {
  reference: ImmutableReference,
  server: FactoryMcpServer,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  integration: Option<FactoryRoleIntegrationBinding>,
}
impl FactoryMcpConfiguration {
  /// Pins the transport bytes and exact existing Project connection selection together.
  pub fn new(
    identity: FactoryKey,
    version: FactoryKey,
    server: FactoryMcpServer,
    integration: Option<FactoryRoleIntegrationBinding>,
  ) -> Result<Self, FactoryError> {
    server.validate_selection_lists()?;
    let bytes = serde_json::to_vec(&(&server, &integration)).map_err(|_| FactoryError::InvalidConfiguration {
      field: "MCP serialization",
    })?;
    if bytes.len() > MAX_FACTORY_MCP_CONFIGURATION_BYTES {
      return Err(FactoryError::InvalidConfiguration {
        field: "MCP configuration",
      });
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
  pub fn validate(&self) -> Result<(), FactoryError> {
    let expected = Self::new(
      self.reference.identity().clone(),
      self.reference.version().clone(),
      self.server.clone(),
      self.integration.clone(),
    )?;
    if expected.reference != self.reference {
      return Err(FactoryError::InvalidConfiguration {
        field: "MCP configuration",
      });
    }
    Ok(())
  }
}

/// Transport families advertised by an installed Runtime, never inferred from its name.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryMcpTransport {
  /// Explicit HTTP service.
  Http,
  /// Explicit installed child process.
  Stdio,
}
impl FactoryMcpServer {
  /// Typed transport family used by installation qualification.
  pub const fn transport(&self) -> FactoryMcpTransport {
    match self {
      Self::Http { .. } => FactoryMcpTransport::Http,
      Self::Stdio { .. } => FactoryMcpTransport::Stdio,
    }
  }
}
impl FactoryMcpConfiguration {
  /// Structured retained transport; contains no credential values.
  pub const fn server(&self) -> &FactoryMcpServer {
    &self.server
  }
  /// Exact managed connection selection, when required.
  pub const fn integration(&self) -> Option<&FactoryRoleIntegrationBinding> {
    self.integration.as_ref()
  }
}

/// Validates the shared bounded execution namespace for named MCP server bindings.
/// Configuration reference identities remain ordinary Factory keys.
pub fn validate_factory_mcp_server_name(name: &str) -> Result<(), FactoryError> {
  if name.is_empty()
    || name.len() > MAX_FACTORY_MCP_SERVER_NAME_BYTES
    || name
      .bytes()
      .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b'-'))
  {
    return Err(FactoryError::InvalidConfiguration {
      field: "MCP server name",
    });
  }
  Ok(())
}
