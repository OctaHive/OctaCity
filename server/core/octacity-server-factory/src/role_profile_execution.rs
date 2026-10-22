//! Declarative runtime additions; installed qualification and credential resolution remain outside the core.
use crate::{FactoryKey, ImmutableReference};
use octacity_server_domain::{IntegrationId, IntegrationVersion, ProjectId};
use octacity_server_secrets::LogicalSecretReference;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Explicit child environment entry. Secret bytes cannot be encoded in the secret variant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryRoleEnvironmentValue {
  /// Reviewed public UTF-8 content; classification is qualified by the installed runtime.
  Public {
    /// Exact value, including whitespace and an intentionally empty value.
    value: String,
  },
  /// Existing logical credential locator, resolved only behind the credential boundary.
  Secret {
    /// Provider-neutral reference, never a credential value or executable path.
    reference: LogicalSecretReference,
  },
}
/// Exact installed MCP configuration. It contains no user-supplied command or service URL.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleMcpBinding {
  /// Qualified installed server/transport settings, including the exact executable when local.
  pub configuration: ImmutableReference,
  /// Optional alias of a separately pinned integration in this same profile.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub integration: Option<FactoryKey>,
}
/// Selection of an existing managed connection, resource and capability; not a new connection registry or grant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleIntegrationBinding {
  /// Project whose reviewed access is selected.
  pub project_id: ProjectId,
  /// Existing Integration Connection identity.
  pub connection_id: IntegrationId,
  /// Exact connection metadata revision.
  pub connection_version: IntegrationVersion,
  /// Exact reviewed Project access revision, rechecked by application qualification.
  pub access_version: IntegrationVersion,
  /// Immutable installed resource selection.
  pub resource: ImmutableReference,
  /// Exact installed tool/purpose capability, not provider token permission.
  pub capability: ImmutableReference,
}
/// Bounded additions to the existing toolchain/model-parameter settings.
/// Empty settings are omitted from retained definitions to preserve v1 content digests.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleProfileExecution {
  /// Explicit public values or logical secret references; no ambient inheritance.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub environment: BTreeMap<String, FactoryRoleEnvironmentValue>,
  /// Ordered argv values; never parsed as shell source. Protected options are qualified by the runtime.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub cli_arguments: Vec<String>,
  /// Named installed MCP server selections.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub mcp_servers: BTreeMap<FactoryKey, FactoryRoleMcpBinding>,
  /// Named exact existing connections, sharing the same Project authorization mechanisms.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub integrations: BTreeMap<FactoryKey, FactoryRoleIntegrationBinding>,
}
impl FactoryRoleProfileExecution {
  /// Reports the legacy absence of additional runtime settings without rewriting old digests.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.environment.is_empty()
      && self.cli_arguments.is_empty()
      && self.mcp_servers.is_empty()
      && self.integrations.is_empty()
  }
  pub(super) fn validate(&self, project: ProjectId) -> Result<(), crate::FactoryError> {
    let invalid = || super::role_profile::invalid("role execution settings");
    if self.environment.len() > crate::MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || self.mcp_servers.len() > crate::MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || self.integrations.len() > crate::MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || self.cli_arguments.len() > crate::MAX_COMMAND_ARGUMENTS
      || self.cli_arguments.iter().map(String::len).sum::<usize>() > crate::MAX_COMMAND_ARGUMENT_BYTES
    {
      return Err(invalid());
    }
    for (name, value) in &self.environment {
      let mut chars = name.chars();
      if name.len() > crate::MAX_FACTORY_KEY_BYTES
        || !chars.next().is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
      {
        return Err(invalid());
      }
      match value {
        FactoryRoleEnvironmentValue::Public { value } => {
          if value.len() > crate::MAX_FACTORY_TEXT_BYTES || value.contains('\0') {
            return Err(invalid());
          }
        }
        FactoryRoleEnvironmentValue::Secret { reference } => {
          LogicalSecretReference::new(reference.provider().clone(), reference.reference()).map_err(|_| invalid())?;
        }
      }
    }
    if self.cli_arguments.iter().any(|arg| arg.contains('\0'))
      || self.integrations.values().any(|binding| binding.project_id != project)
      || self.mcp_servers.values().any(|binding| {
        binding
          .integration
          .as_ref()
          .is_some_and(|key| !self.integrations.contains_key(key))
      })
    {
      return Err(invalid());
    }
    Ok(())
  }
}
