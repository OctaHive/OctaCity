//! Verified provider-neutral Work Source and Reporter process execution.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use octacity_server_adapter_host::{
  AdapterRegistry, ProcessRequest, RegistryAdapter, VerifiedExecutable, cancel_request_id, execute_process,
  load_verified_adapter,
};
pub use octacity_server_adapter_host::{HostError, HostFailureClass, RegistryError};
use octacity_work_source_protocol::{
  AdapterManifest, Capabilities, MAX_REQUEST_BYTES, ProtocolRange, ProtocolRanges, Request, Response, decode_response,
  reporter, source,
};
use std::{path::Path, time::Duration};
use tokio_util::sync::CancellationToken;

/// Immutable verified inventory; Work data cannot install an executable or grant a role.
pub type WorkAdapterRegistry = AdapterRegistry<InstalledWorkAdapter>;
/// Independent Work integration roles of one verified provider executable.
#[derive(Clone, Debug)]
pub struct InstalledWorkAdapter {
  /// Explicit role versions, immutable profiles and credential classes.
  pub manifest: AdapterManifest,
  executable: VerifiedExecutable,
}
impl RegistryAdapter for InstalledWorkAdapter {
  fn load(directory: &Path) -> Result<Self, RegistryError> {
    let (manifest, executable) = load_verified_adapter(
      directory,
      |id, sha, protocol: ProtocolRanges, capabilities: Capabilities| {
        let manifest = AdapterManifest {
          adapter_id: id.to_owned(),
          executable_sha256: sha.to_owned(),
          protocol,
          capabilities,
        };
        manifest.validate().map_err(|error| error.to_string())?;
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
    "work integration"
  }
}
impl InstalledWorkAdapter {
  /// Runs one role-scoped operation with the shared verified host's deadlines and tree cleanup.
  /// Scope/class/profile/negotiation checks precede exposure of protected handles.
  pub async fn execute(
    &self,
    request: Request,
    timeout: Duration,
    grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Response, HostError> {
    request.validate().map_err(|_| invalid())?;
    let cancel = self.validate_capability(&request)?;
    let frame = serde_json::to_vec(&request).map_err(|_| invalid())?;
    if frame.len() > MAX_REQUEST_BYTES {
      return Err(invalid());
    }
    let id = match &request {
      Request::Source(request) => &request.request_id,
      Request::Reporter(request) => &request.request_id,
    };
    let process = ProcessRequest::new(
      &self.executable,
      id,
      frame,
      cancel
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|_| invalid())?,
      request.max_response_bytes(),
    );
    execute_process(process, timeout, grace, cancellation, |frame| {
      decode_response(frame, &request).map_err(|error| error.to_string())
    })
    .await
  }
  fn validate_capability(&self, request: &Request) -> Result<Option<Request>, HostError> {
    match request {
      Request::Source(request) => {
        let capabilities = self
          .manifest
          .capabilities
          .source
          .as_ref()
          .ok_or_else(|| self.unsupported("source"))?;
        let protocol = self
          .manifest
          .protocol
          .source
          .ok_or_else(|| self.unsupported("source"))?;
        ProtocolRange {
          min: source::PROTOCOL_VERSION,
          max: source::PROTOCOL_VERSION,
        }
        .negotiate(protocol)
        .map_err(|_| self.unsupported("source"))?;
        let (profile, scan_id) = match &request.command {
          source::Command::Scan(intent) => (&intent.configuration_digest, &intent.scan_id),
          source::Command::Check(intent) if capabilities.check => (&intent.configuration_digest, &intent.check_id),
          source::Command::Authorize(intent) if capabilities.authorize => {
            (&intent.configuration_digest, &intent.scan_id)
          }
          source::Command::Cancel { .. } if capabilities.cancel => return Ok(None),
          _ => return Err(self.unsupported("source operation")),
        };
        if profile != &capabilities.configuration_digest {
          return Err(invalid());
        }
        Ok(capabilities.cancel.then(|| {
          Request::Source(source::Request {
            protocol_version: request.protocol_version,
            request_id: cancel_request_id(&request.request_id),
            command: source::Command::Cancel {
              target_scan_id: scan_id.clone(),
            },
          })
        }))
      }
      Request::Reporter(request) => {
        let capabilities = self
          .manifest
          .capabilities
          .reporter
          .as_ref()
          .ok_or_else(|| self.unsupported("reporter"))?;
        let protocol = self
          .manifest
          .protocol
          .reporter
          .ok_or_else(|| self.unsupported("reporter"))?;
        ProtocolRange {
          min: request.protocol_version,
          max: request.protocol_version,
        }
        .negotiate(protocol)
        .map_err(|_| self.unsupported("reporter"))?;
        if let reporter::Command::Check(intent) = &request.command {
          if !capabilities.check {
            return Err(self.unsupported("connection check"));
          }
          if intent.configuration_digest != capabilities.configuration_digest {
            return Err(invalid());
          }
          return Ok(capabilities.cancel.then(|| {
            Request::Reporter(reporter::Request {
              protocol_version: request.protocol_version,
              request_id: cancel_request_id(&request.request_id),
              command: reporter::Command::Cancel {
                target_projection_id: intent.check_id.clone(),
              },
            })
          }));
        }
        let intent = match &request.command {
          reporter::Command::Observe(intent) if capabilities.observe => intent,
          reporter::Command::Inspect(intent) if capabilities.observe => intent,
          reporter::Command::Project(intent) if capabilities.project => intent,
          reporter::Command::Apply(step) if capabilities.project => &step.intent,
          reporter::Command::Cancel { .. } if capabilities.cancel => return Ok(None),
          _ => return Err(self.unsupported("reporter operation")),
        };
        if intent.configuration_digest != capabilities.configuration_digest {
          return Err(invalid());
        }
        Ok(capabilities.cancel.then(|| {
          Request::Reporter(reporter::Request {
            protocol_version: request.protocol_version,
            request_id: cancel_request_id(&request.request_id),
            command: reporter::Command::Cancel {
              target_projection_id: intent.projection_id.clone(),
            },
          })
        }))
      }
    }
  }
  fn unsupported(&self, operation: &'static str) -> HostError {
    HostError::Unsupported {
      adapter: self.manifest.adapter_id.clone(),
      operation,
    }
  }
}
fn invalid() -> HostError {
  HostError::InvalidRequest {
    message: "Work integration request does not match its installed capability".into(),
  }
}
