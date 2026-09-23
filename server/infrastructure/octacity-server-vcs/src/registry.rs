//! Operator-owned VCS adapter discovery and protocol validation.

use std::path::Path;

pub use octacity_server_adapter_host::RegistryError;
use octacity_server_adapter_host::{
  AdapterRegistry, RegistryAdapter, VerifiedExecutable, read_manifest, verify_executable,
};
use octacity_vcs_protocol::{AdapterManifest, Capabilities, ProtocolRange, VCS_PROTOCOL_VERSION};
use serde::Deserialize;

const ADAPTER_MANIFEST_VERSION: u16 = 1;

/// Immutable inventory of operator-installed VCS adapters.
pub type VcsAdapterRegistry = AdapterRegistry<InstalledVcsAdapter>;

/// One protocol-validated manifest and its shared verified executable.
#[derive(Clone, Debug)]
pub struct InstalledVcsAdapter {
  /// Protocol identity, digest, range, and advertised capabilities.
  pub manifest: AdapterManifest,
  executable: VerifiedExecutable,
}

impl InstalledVcsAdapter {
  pub(crate) const fn executable(&self) -> &VerifiedExecutable {
    &self.executable
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallManifest {
  manifest_version: u16,
  adapter_id: String,
  executable: String,
  executable_sha256: String,
  protocol: ProtocolRange,
  capabilities: Capabilities,
}

impl RegistryAdapter for InstalledVcsAdapter {
  fn load(directory: &Path) -> Result<Self, RegistryError> {
    let (manifest_path, contents) = read_manifest(directory)?;
    let installed: InstallManifest = toml::from_str(&contents).map_err(|error| RegistryError::ParseManifest {
      path: manifest_path,
      message: error.to_string(),
    })?;
    if installed.manifest_version != ADAPTER_MANIFEST_VERSION {
      return Err(invalid_entry(directory, "unsupported adapter manifest version"));
    }
    let manifest = AdapterManifest {
      adapter_id: installed.adapter_id,
      executable_sha256: installed.executable_sha256,
      protocol: installed.protocol,
      capabilities: installed.capabilities,
    };
    manifest
      .validate()
      .map_err(|error| invalid_entry(directory, error.to_string()))?;
    ProtocolRange {
      min: VCS_PROTOCOL_VERSION,
      max: VCS_PROTOCOL_VERSION,
    }
    .negotiate(manifest.protocol)
    .map_err(|error| invalid_entry(directory, error.to_string()))?;
    let executable = verify_executable(
      directory,
      manifest.adapter_id.clone(),
      &installed.executable,
      manifest.executable_sha256.clone(),
    )?;
    Ok(Self { manifest, executable })
  }

  fn adapter_id(&self) -> &str {
    &self.manifest.adapter_id
  }

  fn executable_sha256(&self) -> &str {
    &self.manifest.executable_sha256
  }

  fn protocol_family() -> &'static str {
    "vcs"
  }
}

fn invalid_entry(path: &Path, message: impl Into<String>) -> RegistryError {
  RegistryError::InvalidEntry {
    path: path.to_owned(),
    message: message.into(),
  }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
