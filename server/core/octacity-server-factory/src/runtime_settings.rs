//! Project selections for an installed Runtime; these values cannot create execution grants.
use crate::*;
use octacity_server_domain::ProjectId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Installation-declared configuration support, attached to the existing choice catalog.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRuntimeSettingsSupport {
  /// Qualified transport families. Absence denies configuring new transports.
  pub mcp_transports: BTreeSet<FactoryMcpTransport>,
  /// Qualified executable paths and exact identities for stdio, never client-installed binaries.
  pub mcp_executables: BTreeMap<String, ImmutableReference>,
}
/// Named configured MCP entry, with an optional alias in the same integration selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRuntimeMcpEntry {
  /// Existing structured transport contract, without secret values.
  pub server: FactoryMcpServer,
  /// Required managed integration alias, when applicable.
  pub integration: Option<FactoryKey>,
}
/// Only configured additions are retained; model discovery and whole profiles stay in their owners.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRuntimeSettingsDefinition {
  /// Reviewed installation pins; replacing an executable requires installation, not this operation.
  pub toolchain: FactoryTaskToolchain,
  /// Named MCP transports selected by ordinary profile bindings.
  pub mcp_servers: BTreeMap<FactoryKey, FactoryRuntimeMcpEntry>,
  /// Exact existing managed Project grants, without another connection registry.
  pub integrations: BTreeMap<FactoryKey, FactoryRoleIntegrationBinding>,
  /// Requested choices; application qualification preserves installation and local ceilings.
  pub permissions: FactoryPermissionSet,
}
impl FactoryRuntimeSettingsDefinition {
  /// Checks common bounds and exact same-Project dependencies without any I/O.
  pub fn validate(&self, project: ProjectId) -> Result<(), FactoryError> {
    if self.mcp_servers.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || self.integrations.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES
    {
      return Err(invalid());
    }
    for alias in self.mcp_servers.keys() {
      validate_factory_mcp_server_name(alias.as_str())?;
    }
    self
      .execution(FactoryRuntimeSettingsVersion::INITIAL)?
      .validate(project)?;
    let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
    if bytes.len() > MAX_FACTORY_ROLE_DEFINITION_BYTES {
      return Err(invalid());
    }
    Ok(())
  }
  fn configuration(
    &self,
    alias: &FactoryKey,
    version: FactoryRuntimeSettingsVersion,
  ) -> Result<Option<FactoryMcpConfiguration>, FactoryError> {
    self
      .mcp_servers
      .get(alias)
      .map(|entry| {
        let integration = entry
          .integration
          .as_ref()
          .map(|name| self.integrations.get(name).cloned().ok_or_else(invalid))
          .transpose()?;
        FactoryMcpConfiguration::new(
          alias.clone(),
          FactoryKey::new(format!("v{}", version.get()))?,
          entry.server.clone(),
          integration,
        )
      })
      .transpose()
  }
  fn execution(&self, version: FactoryRuntimeSettingsVersion) -> Result<FactoryRoleProfileExecution, FactoryError> {
    let mcp_servers = self
      .mcp_servers
      .iter()
      .map(|(name, entry)| {
        let configuration = self.configuration(name, version)?.ok_or_else(invalid)?;
        Ok((
          name.clone(),
          FactoryRoleMcpBinding {
            configuration: configuration.reference().clone(),
            integration: entry.integration.clone(),
          },
        ))
      })
      .collect::<Result<_, FactoryError>>()?;
    Ok(FactoryRoleProfileExecution {
      mcp_servers,
      integrations: self.integrations.clone(),
      ..Default::default()
    })
  }
}
/// Immutable bounded settings retained independently of mutable installation inventory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "RetainedSettings", deny_unknown_fields)]
pub struct FactoryRuntimeSettings {
  project_id: ProjectId,
  runtime: FactoryKey,
  version: FactoryRuntimeSettingsVersion,
  definition: FactoryRuntimeSettingsDefinition,
  digest: FactoryDigest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedSettings {
  project_id: ProjectId,
  runtime: FactoryKey,
  version: FactoryRuntimeSettingsVersion,
  definition: FactoryRuntimeSettingsDefinition,
  digest: FactoryDigest,
}
impl TryFrom<RetainedSettings> for FactoryRuntimeSettings {
  type Error = FactoryError;
  fn try_from(value: RetainedSettings) -> Result<Self, FactoryError> {
    let expected = Self::new(value.project_id, value.runtime, value.version, value.definition)?;
    if expected.digest != value.digest {
      return Err(invalid());
    }
    Ok(expected)
  }
}
impl FactoryRuntimeSettings {
  /// Constructs one immutable version with a domain-separated content identity.
  pub fn new(
    project_id: ProjectId,
    runtime: FactoryKey,
    version: FactoryRuntimeSettingsVersion,
    definition: FactoryRuntimeSettingsDefinition,
  ) -> Result<Self, FactoryError> {
    definition.validate(project_id)?;
    let bytes = serde_json::to_vec(&(&project_id, &runtime, version, &definition)).map_err(|_| invalid())?;
    let digest = FactoryDigest::sha256("octacity.factory.runtime-settings.v1", &[&bytes]);
    Ok(Self {
      project_id,
      runtime,
      version,
      definition,
      digest,
    })
  }
  /// Owning Project.
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }
  /// Existing installed alias.
  pub fn runtime(&self) -> &FactoryKey {
    &self.runtime
  }
  /// Immutable revision.
  pub const fn version(&self) -> FactoryRuntimeSettingsVersion {
    self.version
  }
  /// Original configured additions.
  pub const fn definition(&self) -> &FactoryRuntimeSettingsDefinition {
    &self.definition
  }
  /// Verified content identity, including Project, Runtime and revision.
  pub const fn digest(&self) -> FactoryDigest {
    self.digest
  }
  /// Resolves one exact retained transport using the shared pinning contract.
  pub fn mcp_configuration(&self, alias: &FactoryKey) -> Result<Option<FactoryMcpConfiguration>, FactoryError> {
    self.definition.configuration(alias, self.version)
  }
  /// Ordinary profile selections; no transport definitions or execution grant are copied into profiles.
  pub fn execution(&self) -> Result<FactoryRoleProfileExecution, FactoryError> {
    self.definition.execution(self.version)
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "runtime settings",
  }
}
