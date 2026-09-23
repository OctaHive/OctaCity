//! VCS protocol policy over the shared bounded adapter process host.

use std::time::Duration;

pub use octacity_server_adapter_host::{HostError as VcsHostError, HostFailureClass};
use octacity_server_adapter_host::{ProcessRequest, cancel_request_id, execute_process};
use octacity_vcs_protocol::{
  CancelOperation, Command as VcsCommand, MAX_VCS_MESSAGE_BYTES, Outcome, Request, VCS_PROTOCOL_VERSION,
  decode_response,
};
use tokio_util::sync::CancellationToken;

use crate::registry::InstalledVcsAdapter;

impl InstalledVcsAdapter {
  /// Executes one bounded request in a fresh, environment-cleared process.
  ///
  /// The shared host verifies the executable both before and immediately after
  /// spawn and writes no protected handle or repository data until both checks pass.
  pub async fn execute(
    &self,
    request: Request,
    operation_timeout: Duration,
    cancellation_grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Outcome, VcsHostError> {
    let adapter = self.manifest.adapter_id.clone();
    request.validate().map_err(|error| VcsHostError::InvalidRequest {
      message: error.to_string(),
    })?;
    if request.protocol_version != VCS_PROTOCOL_VERSION {
      return Err(VcsHostError::InvalidRequest {
        message: "request version differs from the negotiated protocol".to_owned(),
      });
    }
    require_capability(self, &request.command)?;
    let request_frame = encode(&adapter, &request)?;
    let cancellation_frame = cancellation_request(&request)
      .map(|cancel| encode(&adapter, &cancel))
      .transpose()?;
    let process = ProcessRequest::new(
      self.executable(),
      &request.request_id,
      request_frame,
      cancellation_frame,
      MAX_VCS_MESSAGE_BYTES,
    );
    execute_process(process, operation_timeout, cancellation_grace, cancellation, |frame| {
      decode_response(frame, &request)
        .map(|response| response.outcome)
        .map_err(|error| error.to_string())
    })
    .await
  }
}

fn require_capability(adapter: &InstalledVcsAdapter, command: &VcsCommand) -> Result<(), VcsHostError> {
  let capabilities = &adapter.manifest.capabilities;
  let supported = match command {
    VcsCommand::ListReferences(_) => capabilities.list_references,
    VcsCommand::ReadCommit(_) => capabilities.read_commit,
    VcsCommand::ListTree(_) => capabilities.list_tree,
    VcsCommand::ReadFile(_) => capabilities.read_file,
    VcsCommand::ResolveRevision(_) => capabilities.resolve_revision,
    VcsCommand::Cancel(_) => true,
  };
  if supported {
    Ok(())
  } else {
    Err(VcsHostError::Unsupported {
      adapter: adapter.manifest.adapter_id.clone(),
      operation: operation_name(command),
    })
  }
}

fn cancellation_request(request: &Request) -> Option<Request> {
  operation_id(&request.command).map(|operation_id| Request {
    protocol_version: VCS_PROTOCOL_VERSION,
    request_id: cancel_request_id(&request.request_id),
    command: VcsCommand::Cancel(CancelOperation {
      target_operation_id: operation_id.to_owned(),
    }),
  })
}

fn encode(adapter: &str, request: &Request) -> Result<Vec<u8>, VcsHostError> {
  serde_json::to_vec(request).map_err(|_| VcsHostError::ProtocolFault {
    adapter: adapter.to_owned(),
    message: "request could not be encoded".to_owned(),
  })
}

fn operation_name(command: &VcsCommand) -> &'static str {
  match command {
    VcsCommand::ListReferences(_) => "list_references",
    VcsCommand::ReadCommit(_) => "read_commit",
    VcsCommand::ListTree(_) => "list_tree",
    VcsCommand::ReadFile(_) => "read_file",
    VcsCommand::ResolveRevision(_) => "resolve_revision",
    VcsCommand::Cancel(_) => "cancel",
  }
}

fn operation_id(command: &VcsCommand) -> Option<&str> {
  match command {
    VcsCommand::ListReferences(value) => Some(&value.repository.operation_id),
    VcsCommand::ReadCommit(value) => Some(&value.repository.operation_id),
    VcsCommand::ListTree(value) => Some(&value.repository.operation_id),
    VcsCommand::ReadFile(value) => Some(&value.repository.operation_id),
    VcsCommand::ResolveRevision(value) => Some(&value.repository.operation_id),
    VcsCommand::Cancel(_) => None,
  }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
