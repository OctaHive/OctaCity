use std::{sync::Arc, time::Duration};

use octacity_server_application::{
  AuthenticatedWebhookEvent, ManagedWebhookRegistration, ManagedWebhookRegistrationRequest,
  ManagedWebhookRegistrationStatus, VerifyWebhookDelivery, WebhookDeliveryVerifier, WebhookManagementProvider,
  WebhookVerificationError, WebhookVerificationFailure,
};
use octacity_server_store::ManagedWebhookOperation;
use octacity_server_webhook::{HostFailureClass, WebhookAdapterRegistry};
use tokio_util::sync::CancellationToken;

pub(super) struct HostedWebhookVerifier {
  pub(super) registry: Arc<WebhookAdapterRegistry>,
  pub(super) operation_timeout: Duration,
  pub(super) cancellation_grace: Duration,
  pub(super) cancellation: CancellationToken,
}

#[async_trait::async_trait]
impl WebhookDeliveryVerifier for HostedWebhookVerifier {
  async fn verify(
    &self,
    delivery: VerifyWebhookDelivery,
  ) -> Result<AuthenticatedWebhookEvent, WebhookVerificationError> {
    use base64::Engine as _;
    use octacity_webhook_provider_protocol::{
      Command, FailureClass, Outcome, Request, VerifyDelivery, WEBHOOK_PROTOCOL_VERSION,
    };

    let adapter = self
      .registry
      .resolve(&delivery.adapter_id, &delivery.adapter_sha256)
      .map_err(|_| WebhookVerificationError::InvalidConfiguration)?;
    let operation_id = delivery.delivery_id.to_string();
    let outcome = adapter
      .execute(
        Request {
          protocol_version: WEBHOOK_PROTOCOL_VERSION,
          request_id: uuid::Uuid::new_v4().to_string(),
          command: Command::VerifyDelivery(VerifyDelivery {
            operation_id,
            integration_id: delivery.integration_id.to_string(),
            verification_material_handle: delivery.verification_material_handle,
            headers: delivery.headers,
            body_base64: base64::engine::general_purpose::STANDARD.encode(delivery.body),
          }),
        },
        self.operation_timeout,
        self.cancellation_grace,
        self.cancellation.child_token(),
      )
      .await
      .map_err(|error| match error.class() {
        HostFailureClass::Transient => WebhookVerificationError::Unavailable,
        HostFailureClass::Cancelled => WebhookVerificationError::Cancelled,
        HostFailureClass::Permanent => WebhookVerificationError::Permanent,
        HostFailureClass::Unsupported => WebhookVerificationError::Unsupported,
        HostFailureClass::InvalidRequest | HostFailureClass::ProtocolFault => WebhookVerificationError::InvalidResponse,
      })?;
    let event = match outcome {
      Outcome::AuthenticatedEvent(event) => event,
      Outcome::Failure(failure) => {
        let classification = match failure.class {
          FailureClass::Permanent => WebhookVerificationFailure::Permanent,
          FailureClass::Transient => WebhookVerificationFailure::Unavailable,
          FailureClass::Cancelled => WebhookVerificationFailure::Cancelled,
          FailureClass::Unsupported => WebhookVerificationFailure::Unsupported,
          FailureClass::InvalidRequest | FailureClass::ProtocolFault => WebhookVerificationFailure::InvalidResponse,
        };
        return Err(WebhookVerificationError::provider(
          classification,
          failure.code,
          failure.diagnostic,
          failure.retry_after_ms,
        ));
      }
      Outcome::Registration(_) | Outcome::Acknowledged { .. } => return Err(WebhookVerificationError::InvalidResponse),
    };
    Ok(AuthenticatedWebhookEvent {
      integration_id: event
        .integration_id
        .parse()
        .map_err(|_| WebhookVerificationError::InvalidResponse)?,
      delivery_id: event
        .delivery_id
        .parse()
        .map_err(|_| WebhookVerificationError::InvalidResponse)?,
      event_kind: octacity_server_store::TriggerEventKind::new(event.event_kind)
        .map_err(|_| WebhookVerificationError::InvalidResponse)?,
      repository_id: event
        .repository_id
        .parse()
        .map_err(|_| WebhookVerificationError::InvalidResponse)?,
      reference: event
        .reference
        .map(|value| value.parse())
        .transpose()
        .map_err(|_| WebhookVerificationError::InvalidResponse)?,
      revision: event
        .revision
        .map(|value| value.parse())
        .transpose()
        .map_err(|_| WebhookVerificationError::InvalidResponse)?,
      provider_time: match event.provider_time_unix_ms {
        Some(value) => Some(
          octacity_server_domain::Timestamp::from_unix_millis(
            i64::try_from(value).map_err(|_| WebhookVerificationError::InvalidResponse)?,
          )
          .map_err(|_| WebhookVerificationError::InvalidResponse)?,
        ),
        None => None,
      },
      actor_display_name: event.actor_display_name,
      metadata: event.metadata,
    })
  }
}

#[async_trait::async_trait]
impl WebhookManagementProvider for HostedWebhookVerifier {
  async fn validate_configuration(
    &self,
    adapter_id: &str,
    adapter_sha256: &str,
  ) -> Result<(), WebhookVerificationError> {
    self
      .registry
      .resolve(adapter_id, adapter_sha256)
      .map(|_| ())
      .map_err(|_| WebhookVerificationError::InvalidConfiguration)
  }

  async fn validate_managed_operation(
    &self,
    adapter_id: &str,
    adapter_sha256: &str,
    operation: ManagedWebhookOperation,
  ) -> Result<(), WebhookVerificationError> {
    let adapter = self
      .registry
      .resolve(adapter_id, adapter_sha256)
      .map_err(|_| WebhookVerificationError::InvalidConfiguration)?;
    let supported = match operation {
      ManagedWebhookOperation::Create => adapter.manifest.capabilities.create_registration,
      ManagedWebhookOperation::Observe => adapter.manifest.capabilities.observe_registration,
      ManagedWebhookOperation::Rotate => adapter.manifest.capabilities.rotate_registration,
      ManagedWebhookOperation::Delete => adapter.manifest.capabilities.delete_registration,
    };
    if supported {
      Ok(())
    } else {
      Err(WebhookVerificationError::Unsupported)
    }
  }

  async fn manage_registration(
    &self,
    request: ManagedWebhookRegistrationRequest,
  ) -> Result<ManagedWebhookRegistration, WebhookVerificationError> {
    use octacity_webhook_provider_protocol::{
      Command, FailureClass, ManagedRegistrationOperation, Outcome, RegistrationStatus, Request,
      WEBHOOK_PROTOCOL_VERSION,
    };

    let adapter = self
      .registry
      .resolve(&request.adapter_id, &request.adapter_sha256)
      .map_err(|_| WebhookVerificationError::InvalidConfiguration)?;
    let operation = ManagedRegistrationOperation {
      operation_id: uuid::Uuid::new_v4().to_string(),
      integration_id: request.integration_id.to_string(),
      idempotency_key: request.idempotency_key.to_string(),
      callback_url: request.callback_url,
      credential_handle: request.administration_credential_handle,
    };
    let command = match request.operation {
      ManagedWebhookOperation::Create => Command::CreateRegistration(operation),
      ManagedWebhookOperation::Observe => Command::ObserveRegistration(operation),
      ManagedWebhookOperation::Rotate => Command::RotateRegistration(operation),
      ManagedWebhookOperation::Delete => Command::DeleteRegistration(operation),
    };
    let outcome = adapter
      .execute(
        Request {
          protocol_version: WEBHOOK_PROTOCOL_VERSION,
          request_id: uuid::Uuid::new_v4().to_string(),
          command,
        },
        self.operation_timeout,
        self.cancellation_grace,
        self.cancellation.child_token(),
      )
      .await
      .map_err(|error| match error.class() {
        HostFailureClass::Unsupported => WebhookVerificationError::Unsupported,
        HostFailureClass::Transient => WebhookVerificationError::Unavailable,
        HostFailureClass::Cancelled => WebhookVerificationError::Cancelled,
        HostFailureClass::Permanent => WebhookVerificationError::InvalidConfiguration,
        HostFailureClass::InvalidRequest | HostFailureClass::ProtocolFault => WebhookVerificationError::InvalidResponse,
      })?;
    let registration = match outcome {
      Outcome::Registration(registration) => registration,
      Outcome::Failure(failure) => {
        let classification = match failure.class {
          FailureClass::Unsupported => WebhookVerificationFailure::Unsupported,
          FailureClass::Permanent => WebhookVerificationFailure::Permanent,
          FailureClass::Transient => WebhookVerificationFailure::Unavailable,
          FailureClass::Cancelled => WebhookVerificationFailure::Cancelled,
          FailureClass::InvalidRequest | FailureClass::ProtocolFault => WebhookVerificationFailure::InvalidResponse,
        };
        return Err(WebhookVerificationError::provider(
          classification,
          failure.code,
          failure.diagnostic,
          failure.retry_after_ms,
        ));
      }
      Outcome::AuthenticatedEvent(_) | Outcome::Acknowledged { .. } => {
        return Err(WebhookVerificationError::InvalidResponse);
      }
    };
    Ok(ManagedWebhookRegistration {
      registration_id: registration.registration_id,
      status: match registration.status {
        RegistrationStatus::Active => ManagedWebhookRegistrationStatus::Active,
        RegistrationStatus::Disabled => ManagedWebhookRegistrationStatus::Disabled,
        RegistrationStatus::Missing => ManagedWebhookRegistrationStatus::Missing,
      },
      callback_url: registration.callback_url,
    })
  }
}

pub(super) struct UnavailableWebhookVerifier;

#[async_trait::async_trait]
impl WebhookDeliveryVerifier for UnavailableWebhookVerifier {
  async fn verify(
    &self,
    _delivery: VerifyWebhookDelivery,
  ) -> Result<AuthenticatedWebhookEvent, WebhookVerificationError> {
    Err(WebhookVerificationError::Unavailable)
  }
}

#[async_trait::async_trait]
impl WebhookManagementProvider for UnavailableWebhookVerifier {
  async fn validate_configuration(
    &self,
    _adapter_id: &str,
    _adapter_sha256: &str,
  ) -> Result<(), WebhookVerificationError> {
    Err(WebhookVerificationError::Unavailable)
  }

  async fn validate_managed_operation(
    &self,
    _adapter_id: &str,
    _adapter_sha256: &str,
    _operation: ManagedWebhookOperation,
  ) -> Result<(), WebhookVerificationError> {
    Err(WebhookVerificationError::Unavailable)
  }

  async fn manage_registration(
    &self,
    _request: ManagedWebhookRegistrationRequest,
  ) -> Result<ManagedWebhookRegistration, WebhookVerificationError> {
    Err(WebhookVerificationError::Unavailable)
  }
}
