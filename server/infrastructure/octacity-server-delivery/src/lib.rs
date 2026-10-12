//! Provider-neutral Delivery registry and bounded process execution.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use octacity_delivery_protocol::{
  AdapterManifest, Capabilities, Command, DELIVERY_PROTOCOL_VERSION, MAX_DELIVERY_MESSAGE_BYTES, Outcome,
  ProtocolRange, Request, decode_response,
};
use octacity_server_adapter_host::{
  AdapterRegistry, ProcessRequest, RegistryAdapter, VerifiedExecutable, cancel_request_id, execute_process,
  load_verified_adapter,
};
pub use octacity_server_adapter_host::{HostError, HostFailureClass, RegistryError};
use std::{path::Path, time::Duration};
use tokio_util::sync::CancellationToken;

/// Immutable operator inventory; repository or model input cannot install adapters.
pub type DeliveryAdapterRegistry = AdapterRegistry<InstalledDeliveryAdapter>;
/// One pinned Delivery capability of a provider plugin.
#[derive(Clone, Debug)]
pub struct InstalledDeliveryAdapter {
  /// Independently negotiated wire contract and profile identity.
  pub manifest: AdapterManifest,
  executable: VerifiedExecutable,
}
impl RegistryAdapter for InstalledDeliveryAdapter {
  fn load(directory: &Path) -> Result<Self, RegistryError> {
    let (manifest, executable) = load_verified_adapter(
      directory,
      |id, sha, protocol: ProtocolRange, capabilities: Capabilities| {
        let manifest = AdapterManifest {
          adapter_id: id.to_owned(),
          executable_sha256: sha.to_owned(),
          protocol,
          capabilities,
        };
        manifest.validate().map_err(|error| error.to_string())?;
        ProtocolRange {
          min: DELIVERY_PROTOCOL_VERSION,
          max: DELIVERY_PROTOCOL_VERSION,
        }
        .negotiate(protocol)
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
    "delivery"
  }
}
impl InstalledDeliveryAdapter {
  /// Executes one bounded, correlated request with clean environment and whole-tree cleanup.
  /// Profile/capability validation happens before any protected handle is passed.
  pub async fn execute(
    &self,
    request: Request,
    timeout: Duration,
    grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Outcome, HostError> {
    request.validate().map_err(|_| invalid())?;
    let capabilities = &self.manifest.capabilities;
    let supported = match &request.command {
      Command::Observe(intent) => {
        if intent.configuration_digest != capabilities.configuration_digest {
          return Err(invalid());
        }
        capabilities.observe
      }
      Command::Publish(intent) => {
        if intent.configuration_digest != capabilities.configuration_digest {
          return Err(invalid());
        }
        capabilities.publish
      }
      Command::Cancel { .. } => capabilities.cancel,
    };
    if !supported {
      return Err(HostError::Unsupported {
        adapter: self.manifest.adapter_id.clone(),
        operation: "delivery",
      });
    }
    let cancel = match &request.command {
      Command::Observe(intent) | Command::Publish(intent) => Some(Request {
        protocol_version: DELIVERY_PROTOCOL_VERSION,
        request_id: cancel_request_id(&request.request_id),
        command: Command::Cancel {
          target_delivery_id: intent.delivery_id.clone(),
        },
      }),
      Command::Cancel { .. } => None,
    };
    let process = ProcessRequest::new(
      &self.executable,
      &request.request_id,
      serde_json::to_vec(&request).map_err(|_| invalid())?,
      cancel
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|_| invalid())?,
      MAX_DELIVERY_MESSAGE_BYTES,
    );
    execute_process(process, timeout, grace, cancellation, |frame| {
      decode_response(frame, &request)
        .map(|response| response.outcome)
        .map_err(|error| error.to_string())
    })
    .await
  }
}
fn invalid() -> HostError {
  HostError::InvalidRequest {
    message: "Delivery request does not match its installed capability".into(),
  }
}
