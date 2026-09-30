use std::sync::Arc;

use axum::{
  Json,
  extract::{Extension, Path, State, rejection::JsonRejection},
  http::{HeaderMap, StatusCode},
  response::IntoResponse,
};
use octacity_server_application::{
  ManagedWebhookInput, ManagedWebhookProjection as ApplicationManagedWebhookProjection,
  ManagedWebhookRegistrationStatus as ApplicationManagedWebhookRegistrationStatus, UnmanagedWebhookInput,
};
use uuid::Uuid;

use super::{
  ApiError, ManagementApplication, authorized_handler_error, body, idempotency_key, invalid_input,
  mutation_disposition, now_unix_ms,
};
use crate::v1::{
  CreateManagedWebhookRequest, CreateUnmanagedWebhookRequest, ManagedWebhookRegistrationResource,
  ManagedWebhookRegistrationStatus, ManagedWebhookResource, TriggerDefinitionResource, UnmanagedWebhookResource,
  WebhookVerificationRequirements,
};

pub(super) async fn create_unmanaged_webhook(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  headers: HeaderMap,
  payload: Result<Json<CreateUnmanagedWebhookRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_unmanaged_webhook(
      UnmanagedWebhookInput {
        integration_id: Uuid::new_v4(),
        trigger_id: Uuid::new_v4(),
        configuration_id: body.configuration_id,
        configuration_version: body.configuration_version,
        enabled: body.enabled,
        adapter_id: body.adapter_id,
        adapter_sha256: body.adapter_sha256,
        verification_material_handle: body.verification_material_handle,
        verification_headers: body.verification_headers,
        repository_id: body.repository_id,
        event_kind: body.event_kind,
        parameters: body.parameters,
        priority: body.priority,
      },
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .definitions
    .create_unmanaged_webhook
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((
    StatusCode::CREATED,
    Json(UnmanagedWebhookResource {
      disposition: mutation_disposition(outcome.disposition),
      integration_id: outcome.integration_id.to_string(),
      trigger: TriggerDefinitionResource {
        id: outcome.trigger.id.to_string(),
        version: outcome.trigger.version.get(),
      },
      callback_url: outcome.callback_url,
      verification: WebhookVerificationRequirements {
        adapter_id: outcome.verification.adapter_id,
        adapter_sha256: outcome.verification.adapter_sha256,
        required_headers: outcome.verification.required_headers,
        protected_material_required: outcome.verification.protected_material_required,
      },
    }),
  ))
}

pub(super) async fn create_managed_webhook(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  headers: HeaderMap,
  payload: Result<Json<CreateManagedWebhookRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_managed_webhook(
      ManagedWebhookInput {
        webhook: UnmanagedWebhookInput {
          integration_id: Uuid::new_v4(),
          trigger_id: Uuid::new_v4(),
          configuration_id: body.configuration_id,
          configuration_version: body.configuration_version,
          enabled: body.enabled,
          adapter_id: body.adapter_id,
          adapter_sha256: body.adapter_sha256,
          verification_material_handle: body.verification_material_handle,
          verification_headers: body.verification_headers,
          repository_id: body.repository_id,
          event_kind: body.event_kind,
          parameters: body.parameters,
          priority: body.priority,
        },
        administration_credential_handle: body.administration_credential_handle,
      },
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .definitions
    .create_managed_webhook
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::CREATED, Json(managed_webhook_resource(outcome))))
}

pub(super) async fn observe_managed_webhook(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(integration_id): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
  let key = idempotency_key(&headers, &request_id)?;
  let now = now_unix_ms(&request_id)?;
  let command = application
    .inputs
    .observe_managed_webhook(&integration_id, key.as_str(), now)
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .definitions
    .observe_managed_webhook
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(managed_webhook_resource(outcome))))
}

pub(super) async fn rotate_managed_webhook(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(integration_id): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
  let key = idempotency_key(&headers, &request_id)?;
  let now = now_unix_ms(&request_id)?;
  let command = application
    .inputs
    .rotate_managed_webhook(&integration_id, key.as_str(), now)
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .definitions
    .rotate_managed_webhook
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(managed_webhook_resource(outcome))))
}

pub(super) async fn delete_managed_webhook(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(integration_id): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
  let key = idempotency_key(&headers, &request_id)?;
  let now = now_unix_ms(&request_id)?;
  let command = application
    .inputs
    .delete_managed_webhook(&integration_id, key.as_str(), now)
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .definitions
    .delete_managed_webhook
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(managed_webhook_resource(outcome))))
}

fn managed_webhook_resource(outcome: ApplicationManagedWebhookProjection) -> ManagedWebhookResource {
  ManagedWebhookResource {
    disposition: mutation_disposition(outcome.disposition),
    integration_id: outcome.integration_id.to_string(),
    trigger: TriggerDefinitionResource {
      id: outcome.trigger.id.to_string(),
      version: outcome.trigger.version.get(),
    },
    callback_url: outcome.callback_url,
    registration: outcome
      .registration
      .map(|registration| ManagedWebhookRegistrationResource {
        registration_id: registration.registration_id,
        status: match registration.status {
          ApplicationManagedWebhookRegistrationStatus::Active => ManagedWebhookRegistrationStatus::Active,
          ApplicationManagedWebhookRegistrationStatus::Disabled => ManagedWebhookRegistrationStatus::Disabled,
          ApplicationManagedWebhookRegistrationStatus::Missing => ManagedWebhookRegistrationStatus::Missing,
        },
        callback_url: registration.callback_url,
      }),
  }
}
