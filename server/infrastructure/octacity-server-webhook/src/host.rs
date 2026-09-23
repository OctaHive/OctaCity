//! Webhook protocol policy over the shared bounded adapter process host.

use std::time::Duration;

pub use octacity_server_adapter_host::{HostError as WebhookHostError, HostFailureClass};
use octacity_server_adapter_host::{ProcessRequest, cancel_request_id, execute_process};
use octacity_webhook_provider_protocol::{
  CancelOperation, Command as WebhookCommand, FailureClass, MAX_WEBHOOK_MESSAGE_BYTES, Outcome, Request, Response,
  WEBHOOK_PROTOCOL_VERSION, decode_response,
};
use tokio_util::sync::CancellationToken;

use crate::registry::InstalledWebhookAdapter;

impl InstalledWebhookAdapter {
  /// Executes one bounded request in a fresh, environment-cleared process.
  ///
  /// The shared host verifies the executable both before and immediately after
  /// spawn and writes no protected handle or body until both checks pass.
  pub async fn execute(
    &self,
    request: Request,
    operation_timeout: Duration,
    cancellation_grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Outcome, WebhookHostError> {
    let adapter = self.manifest.adapter_id.clone();
    request.validate().map_err(|error| WebhookHostError::InvalidRequest {
      message: error.to_string(),
    })?;
    if request.protocol_version != WEBHOOK_PROTOCOL_VERSION {
      return Err(WebhookHostError::InvalidRequest {
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
      MAX_WEBHOOK_MESSAGE_BYTES,
    );
    execute_process(process, operation_timeout, cancellation_grace, cancellation, |frame| {
      let response = decode_response(frame, &request).map_err(|error| error.to_string())?;
      validate_outcome(&request.command, &response).map_err(str::to_owned)?;
      Ok(response.outcome)
    })
    .await
  }
}

fn require_capability(adapter: &InstalledWebhookAdapter, command: &WebhookCommand) -> Result<(), WebhookHostError> {
  let capabilities = &adapter.manifest.capabilities;
  let supported = match command {
    WebhookCommand::VerifyDelivery(_) => capabilities.verify_delivery,
    WebhookCommand::CreateRegistration(_) => capabilities.create_registration,
    WebhookCommand::ObserveRegistration(_) => capabilities.observe_registration,
    WebhookCommand::RotateRegistration(_) => capabilities.rotate_registration,
    WebhookCommand::DeleteRegistration(_) => capabilities.delete_registration,
    WebhookCommand::Cancel(_) => true,
  };
  if supported {
    Ok(())
  } else {
    Err(WebhookHostError::Unsupported {
      adapter: adapter.manifest.adapter_id.clone(),
      operation: operation_name(command),
    })
  }
}

fn validate_outcome(command: &WebhookCommand, response: &Response) -> Result<(), &'static str> {
  match (command, &response.outcome) {
    (WebhookCommand::VerifyDelivery(request), Outcome::AuthenticatedEvent(event))
      if event.integration_id == request.integration_id =>
    {
      Ok(())
    }
    (WebhookCommand::VerifyDelivery(_), Outcome::Failure(_)) => Ok(()),
    (
      WebhookCommand::CreateRegistration(request)
      | WebhookCommand::ObserveRegistration(request)
      | WebhookCommand::RotateRegistration(request)
      | WebhookCommand::DeleteRegistration(request),
      Outcome::Registration(registration),
    ) if registration.integration_id == request.integration_id => Ok(()),
    (
      WebhookCommand::CreateRegistration(_)
      | WebhookCommand::ObserveRegistration(_)
      | WebhookCommand::RotateRegistration(_)
      | WebhookCommand::DeleteRegistration(_),
      Outcome::Failure(_),
    ) => Ok(()),
    (WebhookCommand::Cancel(request), Outcome::Acknowledged { operation_id })
      if operation_id == &request.target_operation_id =>
    {
      Ok(())
    }
    (WebhookCommand::Cancel(_), Outcome::Failure(failure)) if failure.class == FailureClass::Cancelled => Ok(()),
    _ => Err("response outcome does not match the requested operation or integration"),
  }
}

fn cancellation_request(request: &Request) -> Option<Request> {
  operation_id(&request.command).map(|operation_id| Request {
    protocol_version: WEBHOOK_PROTOCOL_VERSION,
    request_id: cancel_request_id(&request.request_id),
    command: WebhookCommand::Cancel(CancelOperation {
      target_operation_id: operation_id.to_owned(),
    }),
  })
}

fn encode(adapter: &str, request: &Request) -> Result<Vec<u8>, WebhookHostError> {
  serde_json::to_vec(request).map_err(|_| WebhookHostError::ProtocolFault {
    adapter: adapter.to_owned(),
    message: "request could not be encoded".to_owned(),
  })
}

fn operation_name(command: &WebhookCommand) -> &'static str {
  match command {
    WebhookCommand::VerifyDelivery(_) => "verify_delivery",
    WebhookCommand::CreateRegistration(_) => "create_registration",
    WebhookCommand::ObserveRegistration(_) => "observe_registration",
    WebhookCommand::RotateRegistration(_) => "rotate_registration",
    WebhookCommand::DeleteRegistration(_) => "delete_registration",
    WebhookCommand::Cancel(_) => "cancel",
  }
}

fn operation_id(command: &WebhookCommand) -> Option<&str> {
  match command {
    WebhookCommand::VerifyDelivery(value) => Some(&value.operation_id),
    WebhookCommand::CreateRegistration(value)
    | WebhookCommand::ObserveRegistration(value)
    | WebhookCommand::RotateRegistration(value)
    | WebhookCommand::DeleteRegistration(value) => Some(&value.operation_id),
    WebhookCommand::Cancel(_) => None,
  }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
