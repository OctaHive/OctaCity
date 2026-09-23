use super::*;

/// Application service for webhook integration configuration and managed registration.
pub struct WebhookManagementService {
  configuration_store: Arc<dyn WebhookConfigurationStore>,
  registrations: Arc<dyn ManagedWebhookRegistrationStore>,
  operations: Arc<dyn ManagedWebhookOperationStore>,
  provider: Arc<dyn WebhookManagementProvider>,
  callback_origin: Option<WebhookCallbackOrigin>,
}

impl WebhookManagementService {
  /// Creates the management service; `None` keeps routes stable while webhook ingress is disabled.
  pub fn new(
    configuration_store: Arc<dyn WebhookConfigurationStore>,
    registrations: Arc<dyn ManagedWebhookRegistrationStore>,
    operations: Arc<dyn ManagedWebhookOperationStore>,
    provider: Arc<dyn WebhookManagementProvider>,
    callback_origin: Option<WebhookCallbackOrigin>,
  ) -> Self {
    Self {
      configuration_store,
      registrations,
      operations,
      provider,
      callback_origin,
    }
  }

  async fn create(
    &self,
    command: CreateUnmanagedWebhookCommand,
  ) -> Result<UnmanagedWebhookProjection, ApplicationError> {
    let Some(callback_origin) = &self.callback_origin else {
      return Err(ApplicationError::unavailable());
    };
    let definition = validated_webhook_definition(WebhookDefinitionInput {
      adapter_id: &command.adapter_id,
      adapter_sha256: &command.adapter_sha256,
      verification_material_handle: &command.verification_material_handle,
      verification_headers: &command.verification_headers,
      repository_id: command.repository_id,
      event_kind: &command.event_kind,
      parameters: &command.parameters,
      priority: command.priority,
    })?;
    let required_headers = definition.verification_headers.iter().cloned().collect();
    self
      .provider
      .validate_configuration(&command.adapter_id, &command.adapter_sha256)
      .await
      .map_err(|error| match error.classification() {
        WebhookVerificationFailure::Unavailable | WebhookVerificationFailure::Cancelled => {
          ApplicationError::unavailable()
        }
        _ => ApplicationError::invalid(),
      })?;
    let outcome = self
      .configuration_store
      .create_unmanaged_webhook(CreateUnmanagedWebhook {
        integration_id: command.integration_id,
        trigger: CreateTriggerDefinition {
          id: command.trigger_id,
          version: TriggerVersion::INITIAL,
          configuration_id: command.configuration_id,
          configuration_version: command.configuration_version,
          kind: TriggerKind::External,
          enabled: command.enabled,
          definition: json!({}),
          idempotency_key: command.idempotency_key.clone(),
          created_at: command.created_at,
        },
        definition,
        idempotency_key: command.idempotency_key,
        created_at: command.created_at,
      })
      .await?;
    Ok(UnmanagedWebhookProjection {
      disposition: outcome.disposition.into(),
      integration_id: outcome.integration_id,
      trigger: outcome.trigger,
      callback_url: callback_origin.callback_url(outcome.integration_id),
      verification: WebhookVerificationRequirements {
        adapter_id: command.adapter_id,
        adapter_sha256: command.adapter_sha256,
        required_headers,
        protected_material_required: true,
      },
    })
  }

  async fn create_managed(
    &self,
    command: CreateManagedWebhookCommand,
  ) -> Result<ManagedWebhookProjection, ApplicationError> {
    let callback_origin = self
      .callback_origin
      .as_ref()
      .ok_or_else(ApplicationError::unavailable)?;
    let delivery = validated_webhook_definition(WebhookDefinitionInput {
      adapter_id: &command.adapter_id,
      adapter_sha256: &command.adapter_sha256,
      verification_material_handle: &command.verification_material_handle,
      verification_headers: &command.verification_headers,
      repository_id: command.repository_id,
      event_kind: &command.event_kind,
      parameters: &command.parameters,
      priority: command.priority,
    })?;
    let definition = ManagedWebhookDefinition {
      delivery,
      administration_credential_handle: command.administration_credential_handle.clone(),
    };
    definition.validate().map_err(|_| ApplicationError::invalid())?;
    self
      .provider
      .validate_managed_operation(
        &command.adapter_id,
        &command.adapter_sha256,
        ManagedWebhookOperation::Create,
      )
      .await
      .map_err(managed_provider_error)?;
    let reservation = self
      .configuration_store
      .create_managed_webhook(CreateManagedWebhook {
        integration_id: command.integration_id,
        trigger: CreateTriggerDefinition {
          id: command.trigger_id,
          version: TriggerVersion::INITIAL,
          configuration_id: command.configuration_id,
          configuration_version: command.configuration_version,
          kind: TriggerKind::External,
          enabled: command.enabled,
          definition: json!({}),
          idempotency_key: command.idempotency_key.clone(),
          created_at: command.created_at,
        },
        definition,
        idempotency_key: command.idempotency_key.clone(),
        created_at: command.created_at,
      })
      .await?;
    let callback_url = callback_origin.callback_url(reservation.integration_id);
    if let Some(registration) = &reservation.registration {
      ensure_callback(&registration.callback_url, &callback_url)?;
    }
    Ok(ManagedWebhookProjection {
      disposition: reservation.disposition.into(),
      integration_id: reservation.integration_id,
      trigger: reservation.trigger,
      callback_url,
      registration: reservation.registration.map(Into::into),
    })
  }

  async fn manage(
    &self,
    command: ManageWebhookRegistrationCommand,
  ) -> Result<ManagedWebhookProjection, ApplicationError> {
    if command.operation == ManagedWebhookOperation::Create {
      return Err(ApplicationError::invalid());
    }
    let callback_origin = self
      .callback_origin
      .as_ref()
      .ok_or_else(ApplicationError::unavailable)?;
    let integration = self.registrations.managed_webhook(command.integration_id).await?;
    self
      .provider
      .validate_managed_operation(
        &integration.definition.delivery.adapter_id,
        &integration.definition.delivery.adapter_sha256,
        command.operation,
      )
      .await
      .map_err(managed_provider_error)?;
    let callback_url = callback_origin.callback_url(integration.integration_id);
    let disposition = self
      .operations
      .enqueue_managed_webhook_operation(EnqueueManagedWebhookOperation {
        integration_id: command.integration_id,
        operation: command.operation,
        idempotency_key: command.idempotency_key.clone(),
        requested_at: command.observed_at,
      })
      .await?;
    if let Some(registration) = &integration.registration {
      ensure_callback(&registration.callback_url, &callback_url)?;
    }
    Ok(ManagedWebhookProjection {
      disposition: disposition.into(),
      integration_id: integration.integration_id,
      trigger: integration.trigger,
      callback_url,
      registration: integration.registration.map(Into::into),
    })
  }
}

#[async_trait]
impl CommandHandler<CreateUnmanagedWebhookCommand> for WebhookManagementService {
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: CreateUnmanagedWebhookCommand,
  ) -> Result<UnmanagedWebhookProjection, Self::Error> {
    self.create(command).await
  }
}

#[async_trait]
impl CommandHandler<CreateManagedWebhookCommand> for WebhookManagementService {
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: CreateManagedWebhookCommand,
  ) -> Result<ManagedWebhookProjection, Self::Error> {
    self.create_managed(command).await
  }
}

#[async_trait]
impl CommandHandler<ManageWebhookRegistrationCommand> for WebhookManagementService {
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: ManageWebhookRegistrationCommand,
  ) -> Result<ManagedWebhookProjection, Self::Error> {
    self.manage(command).await
  }
}
