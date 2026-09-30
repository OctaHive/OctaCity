//! Operator-owned webhook adapter discovery and protocol validation.

use std::path::Path;

pub use octacity_server_adapter_host::RegistryError;
use octacity_server_adapter_host::{AdapterRegistry, RegistryAdapter, VerifiedExecutable, load_verified_adapter};
use octacity_webhook_provider_protocol::{AdapterManifest, Capabilities, ProtocolRange, WEBHOOK_PROTOCOL_VERSION};

/// Immutable inventory of operator-installed webhook adapters.
pub type WebhookAdapterRegistry = AdapterRegistry<InstalledWebhookAdapter>;

/// One protocol-validated manifest and its shared verified executable.
#[derive(Clone, Debug)]
pub struct InstalledWebhookAdapter {
  /// Protocol identity, digest, range, and advertised capabilities.
  pub manifest: AdapterManifest,
  executable: VerifiedExecutable,
}

impl InstalledWebhookAdapter {
  pub(crate) const fn executable(&self) -> &VerifiedExecutable {
    &self.executable
  }
}

impl RegistryAdapter for InstalledWebhookAdapter {
  fn load(directory: &Path) -> Result<Self, RegistryError> {
    let (manifest, executable) = load_verified_adapter(
      directory,
      |adapter_id, executable_sha256, protocol: ProtocolRange, capabilities: Capabilities| {
        let manifest = AdapterManifest {
          adapter_id: adapter_id.to_owned(),
          executable_sha256: executable_sha256.to_owned(),
          protocol,
          capabilities,
        };
        manifest.validate().map_err(|error| error.to_string())?;
        ProtocolRange {
          min: WEBHOOK_PROTOCOL_VERSION,
          max: WEBHOOK_PROTOCOL_VERSION,
        }
        .negotiate(manifest.protocol)
        .map_err(|error| error.to_string())?;
        Ok(manifest)
      },
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
    "webhook"
  }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
