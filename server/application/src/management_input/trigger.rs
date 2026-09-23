use super::*;

impl ManagementInputFactory {
  /// Creates a typed manual Trigger-definition command.
  pub fn create_manual_trigger_definition(
    &self,
    input: ManualTriggerDefinitionInput,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CreateTriggerDefinitionCommand, ManagementInputError> {
    if !input.definition.is_object() {
      return Err(ManagementInputError::Invalid("manual trigger definition"));
    }
    Ok(CreateTriggerDefinitionCommand {
      id: identifier(input.id, "trigger id")?,
      version: TriggerVersion::INITIAL,
      configuration_id: parse(&input.configuration_id, "build configuration id")?,
      configuration_version: version(input.configuration_version, "build configuration version")?,
      kind: octacity_server_store::TriggerKind::Manual,
      enabled: input.enabled,
      definition: input.definition,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      created_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed scheduled Trigger-definition command.
  pub fn create_scheduled_trigger_definition(
    &self,
    input: ScheduledTriggerDefinitionInput,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CreateScheduleCommand, ManagementInputError> {
    let schedule = decode::<octacity_server_store::ScheduleDefinition>(input.schedule, "schedule")?;
    schedule
      .validate()
      .map_err(|_| ManagementInputError::Invalid("schedule"))?;
    Ok(CreateScheduleCommand {
      id: identifier(input.id, "trigger id")?,
      version: TriggerVersion::INITIAL,
      configuration_id: parse(&input.configuration_id, "build configuration id")?,
      configuration_version: version(input.configuration_version, "build configuration version")?,
      enabled: input.enabled,
      schedule,
      build: decode::<ScheduledBuildDefinition>(input.build, "scheduled build definition")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      created_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed unmanaged webhook configuration command.
  pub fn create_unmanaged_webhook(
    &self,
    input: UnmanagedWebhookInput,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CreateUnmanagedWebhookCommand, ManagementInputError> {
    Ok(CreateUnmanagedWebhookCommand {
      integration_id: identifier(input.integration_id, "integration id")?,
      trigger_id: identifier(input.trigger_id, "trigger id")?,
      configuration_id: parse(&input.configuration_id, "build configuration id")?,
      configuration_version: version(input.configuration_version, "build configuration version")?,
      enabled: input.enabled,
      adapter_id: input.adapter_id,
      adapter_sha256: input.adapter_sha256,
      verification_material_handle: input.verification_material_handle,
      verification_headers: input.verification_headers,
      repository_id: parse(&input.repository_id, "repository id")?,
      event_kind: octacity_server_store::TriggerEventKind::new(input.event_kind)
        .map_err(|_| ManagementInputError::Invalid("webhook event kind"))?,
      parameters: input.parameters,
      priority: input.priority,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      created_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed managed webhook creation command.
  pub fn create_managed_webhook(
    &self,
    input: ManagedWebhookInput,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CreateManagedWebhookCommand, ManagementInputError> {
    let webhook = input.webhook;
    Ok(CreateManagedWebhookCommand {
      integration_id: identifier(webhook.integration_id, "integration id")?,
      trigger_id: identifier(webhook.trigger_id, "trigger id")?,
      configuration_id: parse(&webhook.configuration_id, "build configuration id")?,
      configuration_version: version(webhook.configuration_version, "build configuration version")?,
      enabled: webhook.enabled,
      adapter_id: webhook.adapter_id,
      adapter_sha256: webhook.adapter_sha256,
      verification_material_handle: webhook.verification_material_handle,
      verification_headers: webhook.verification_headers,
      administration_credential_handle: input.administration_credential_handle,
      repository_id: parse(&webhook.repository_id, "repository id")?,
      event_kind: octacity_server_store::TriggerEventKind::new(webhook.event_kind)
        .map_err(|_| ManagementInputError::Invalid("webhook event kind"))?,
      parameters: webhook.parameters,
      priority: webhook.priority,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      created_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed remote-registration observation command.
  pub fn observe_managed_webhook(
    &self,
    integration_id: &str,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<ManageWebhookRegistrationCommand, ManagementInputError> {
    self.manage_webhook_registration(
      integration_id,
      ManagedWebhookOperation::Observe,
      idempotency_key,
      now_unix_ms,
    )
  }

  /// Creates a typed remote-registration rotation command.
  pub fn rotate_managed_webhook(
    &self,
    integration_id: &str,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<ManageWebhookRegistrationCommand, ManagementInputError> {
    self.manage_webhook_registration(
      integration_id,
      ManagedWebhookOperation::Rotate,
      idempotency_key,
      now_unix_ms,
    )
  }

  /// Creates a typed remote-registration deletion command.
  pub fn delete_managed_webhook(
    &self,
    integration_id: &str,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<ManageWebhookRegistrationCommand, ManagementInputError> {
    self.manage_webhook_registration(
      integration_id,
      ManagedWebhookOperation::Delete,
      idempotency_key,
      now_unix_ms,
    )
  }

  fn manage_webhook_registration(
    &self,
    integration_id: &str,
    operation: ManagedWebhookOperation,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<ManageWebhookRegistrationCommand, ManagementInputError> {
    Ok(ManageWebhookRegistrationCommand {
      integration_id: parse(integration_id, "integration id")?,
      operation,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      observed_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed query for one exact durable schedule.
  pub fn get_schedule(&self, trigger_id: &str, version_value: u64) -> Result<GetScheduleQuery, ManagementInputError> {
    Ok(GetScheduleQuery {
      trigger_id: parse(trigger_id, "trigger id")?,
      version: version(version_value, "trigger version")?,
    })
  }

  /// Creates a typed manual-Trigger acceptance command.
  pub fn accept_manual_trigger(
    &self,
    input: ManualTriggerInput,
    now_unix_ms: i64,
  ) -> Result<AcceptManualTriggerCommand, ManagementInputError> {
    let now = timestamp(now_unix_ms)?;
    Ok(AcceptManualTriggerCommand {
      trigger: ManualTriggerCommand {
        trigger: TriggerDefinitionRef {
          id: parse::<TriggerId>(&input.trigger_id, "trigger id")?,
          version: version::<TriggerVersion>(input.trigger_version, "trigger version")?,
        },
        target: TriggerTarget {
          configuration_id: parse::<BuildConfigurationId>(&input.configuration_id, "build configuration id")?,
          configuration_version: version::<BuildConfigurationVersion>(
            input.configuration_version,
            "build configuration version",
          )?,
        },
        deduplication_identity: parse::<TriggerIdentity>(&input.deduplication_identity, "deduplication identity")?,
        source: decode::<ManualSourceSelection>(input.source, "manual source")?,
        parameters: input.parameters,
        priority: input.priority,
        observed_at: now,
      },
      accepted_at: now,
    })
  }
}
