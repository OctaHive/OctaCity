//! Shared structured MCP transports and immutable pins; adapters own execution and secret resolution.
use crate::{FactoryDigest, FactoryError, FactoryKey, FactoryRoleIntegrationBinding, ImmutableReference};
use octacity_server_secrets::LogicalSecretReference;
use serde::{Deserialize, Serialize};

/// Supported investigation credential purpose; it cannot authorize a business write.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryMcpCredentialPurpose {
  /// A separately issued read-only, resource-scoped MCP credential.
  McpRead,
}

/// Installed connection authority. Values stay with the existing private credential owner.
/// Resource and read-only restrictions must also be enforced by the qualified peer/token.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryMcpAuthorization {
  /// Independent Agent credential profile; never the model or Source/Reporter profile.
  pub credential_profile: FactoryKey,
  /// Exact logical locator approved in the Role capability.
  pub secret: LogicalSecretReference,
  /// Only this transport's named environment entry receives the selected value.
  pub environment: String,
  /// Supported credential consumer purpose, distinct from model and delivery authority.
  pub purpose: FactoryMcpCredentialPurpose,
  /// Immutable resource scope of the operator-qualified read credential.
  pub resource: ImmutableReference,
}

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
  #[serde(default, skip_serializing_if = "Option::is_none")]
  authorization: Option<FactoryMcpAuthorization>,
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
      authorization: None,
    })
  }
  /// Exact pin selected by the provider-independent Role/Profile contract.
  pub const fn reference(&self) -> &ImmutableReference {
    &self.reference
  }
  /// Rejects substituted deployment bytes before this configuration can be installed.
  pub fn validate(&self) -> Result<(), FactoryError> {
    let mut expected = Self::new(
      self.reference.identity().clone(),
      self.reference.version().clone(),
      self.server.clone(),
      self.integration.clone(),
    )?;
    if let Some(authorization) = &self.authorization {
      expected = expected.with_authorization(authorization.clone())?;
    }
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
  /// Pins independently approved authentication together with the exact transport.
  /// Legacy unauthenticated configuration digests retain their original encoding.
  pub fn with_authorization(mut self, authorization: FactoryMcpAuthorization) -> Result<Self, FactoryError> {
    let invalid = || FactoryError::InvalidConfiguration {
      field: "MCP authorization",
    };
    let name = &authorization.environment;
    let mut chars = name.chars();
    if name.len() > crate::MAX_FACTORY_KEY_BYTES
      || !chars.next().is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
      || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
      || self
        .integration
        .as_ref()
        .is_some_and(|binding| binding.resource != authorization.resource)
    {
      return Err(invalid());
    }
    let mapped = match &self.server {
      FactoryMcpServer::Http {
        bearer_token_env_var, ..
      } => bearer_token_env_var.as_ref() == Some(name),
      FactoryMcpServer::Stdio { env_vars, .. } => env_vars.contains(name),
    };
    if !mapped {
      return Err(invalid());
    }
    let bytes = serde_json::to_vec(&(&self.server, &self.integration, &authorization)).map_err(|_| invalid())?;
    if bytes.len() > MAX_FACTORY_MCP_CONFIGURATION_BYTES {
      return Err(invalid());
    }
    self.reference = ImmutableReference::new(
      self.reference.identity().clone(),
      self.reference.version().clone(),
      FactoryDigest::content_sha256(&bytes),
    );
    self.authorization = Some(authorization);
    Ok(self)
  }
  /// Exact connection credential/scope selection; absent means no authenticated MCP grant.
  pub const fn authorization(&self) -> Option<&FactoryMcpAuthorization> {
    self.authorization.as_ref()
  }

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
