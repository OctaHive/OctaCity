use super::*;

#[derive(FromRow)]
pub(super) struct DeliveryClaimRow {
  pub(super) id: uuid::Uuid,
  pub(super) integration_id: uuid::Uuid,
  pub(super) state: String,
  pub(super) headers: Json<Value>,
  pub(super) body: Vec<u8>,
  pub(super) received_at_millis: i64,
  pub(super) verification_attempts: i32,
  pub(super) trigger_attempts: i32,
  pub(super) normalized_event: Option<Json<Value>>,
}

#[derive(FromRow)]
pub(super) struct DeliveryDiagnosticRow {
  pub(super) integration_id: uuid::Uuid,
  pub(super) state: String,
  pub(super) verification_attempts: i32,
  pub(super) trigger_attempts: i32,
  pub(super) provider_delivery_id: Option<String>,
  pub(super) failure_code: Option<String>,
  pub(super) diagnostic: Option<String>,
  pub(super) next_attempt_at_millis: Option<i64>,
}

#[derive(FromRow)]
pub(super) struct ManagedOperationClaimRow {
  pub(super) integration_id: uuid::Uuid,
  pub(super) operation: String,
  pub(super) idempotency_key: String,
  pub(super) attempts: i32,
  pub(super) trigger_id: uuid::Uuid,
  pub(super) trigger_version: i64,
  pub(super) build_configuration_id: uuid::Uuid,
  pub(super) build_configuration_version: i64,
  pub(super) enabled: bool,
  pub(super) trigger_kind: String,
  pub(super) definition: Json<Value>,
  pub(super) administration_credential_handle: String,
  pub(super) remote_registration: Option<Json<Value>>,
}

pub(super) fn decode_delivery_claim(
  row: DeliveryClaimRow,
  request: &ClaimWebhookDeliveries,
) -> Result<WebhookDeliveryClaim, StoreError> {
  let work = match row.state.as_str() {
    "pending_verification" | "retry_scheduled" => WebhookDeliveryWork::Verify {
      headers: serde_json::from_value::<BTreeMap<String, String>>(row.headers.0)
        .map_err(|_| StoreError::Unavailable)?,
      body: row.body,
    },
    "pending_trigger" => WebhookDeliveryWork::Trigger {
      event: serde_json::from_value(row.normalized_event.ok_or(StoreError::Unavailable)?.0)
        .map_err(|_| StoreError::Unavailable)?,
    },
    _ => return Err(StoreError::Unavailable),
  };
  Ok(WebhookDeliveryClaim {
    delivery_id: WebhookDeliveryId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    integration_id: IntegrationId::from_uuid(row.integration_id).map_err(|_| StoreError::Unavailable)?,
    received_at: timestamp(row.received_at_millis)?,
    verification_attempt: u16::try_from(row.verification_attempts).map_err(|_| StoreError::Unavailable)?,
    trigger_attempt: u16::try_from(row.trigger_attempts).map_err(|_| StoreError::Unavailable)?,
    work,
    owner: request.owner.clone(),
    claim_expires_at: request.claim_expires_at,
  })
}

pub(super) fn decode_managed_operation_claim(
  row: ManagedOperationClaimRow,
  request: &ClaimManagedWebhookOperations,
) -> Result<ManagedWebhookOperationClaim, StoreError> {
  if row.trigger_kind != TriggerKind::External.as_str() {
    return Err(StoreError::Unavailable);
  }
  let definition = ManagedWebhookDefinition {
    delivery: serde_json::from_value(row.definition.0).map_err(|_| StoreError::Unavailable)?,
    administration_credential_handle: row.administration_credential_handle,
  };
  definition.validate().map_err(|_| StoreError::Unavailable)?;
  let registration = row
    .remote_registration
    .map(|value| serde_json::from_value::<ManagedWebhookRegistration>(value.0))
    .transpose()
    .map_err(|_| StoreError::Unavailable)?;
  Ok(ManagedWebhookOperationClaim {
    integration: ManagedWebhookRecord {
      integration_id: IntegrationId::from_uuid(row.integration_id).map_err(|_| StoreError::Unavailable)?,
      trigger: TriggerDefinitionRef {
        id: TriggerId::from_uuid(row.trigger_id).map_err(|_| StoreError::Unavailable)?,
        version: TriggerVersion::new(u64::try_from(row.trigger_version).map_err(|_| StoreError::Unavailable)?)
          .map_err(|_| StoreError::Unavailable)?,
      },
      target: TriggerTarget {
        configuration_id: octacity_server_domain::BuildConfigurationId::from_uuid(row.build_configuration_id)
          .map_err(|_| StoreError::Unavailable)?,
        configuration_version: octacity_server_domain::BuildConfigurationVersion::new(
          u64::try_from(row.build_configuration_version).map_err(|_| StoreError::Unavailable)?,
        )
        .map_err(|_| StoreError::Unavailable)?,
      },
      enabled: row.enabled,
      definition,
      registration,
    },
    operation: parse_managed_operation(&row.operation)?,
    idempotency_key: IdempotencyKey::new(row.idempotency_key).map_err(|_| StoreError::Unavailable)?,
    attempt: u16::try_from(row.attempts).map_err(|_| StoreError::Unavailable)?,
    owner: request.owner.clone(),
    claim_expires_at: request.claim_expires_at,
  })
}

pub(super) const fn managed_operation(operation: ManagedWebhookOperation) -> &'static str {
  match operation {
    ManagedWebhookOperation::Create => "create",
    ManagedWebhookOperation::Observe => "observe",
    ManagedWebhookOperation::Rotate => "rotate",
    ManagedWebhookOperation::Delete => "delete",
  }
}

fn parse_managed_operation(value: &str) -> Result<ManagedWebhookOperation, StoreError> {
  match value {
    "create" => Ok(ManagedWebhookOperation::Create),
    "observe" => Ok(ManagedWebhookOperation::Observe),
    "rotate" => Ok(ManagedWebhookOperation::Rotate),
    "delete" => Ok(ManagedWebhookOperation::Delete),
    _ => Err(StoreError::Unavailable),
  }
}

pub(super) const fn failure_code(code: WebhookFailureCode) -> &'static str {
  match code {
    WebhookFailureCode::AuthenticationFailed => "authentication_failed",
    WebhookFailureCode::InvalidConfiguration => "invalid_configuration",
    WebhookFailureCode::Unsupported => "unsupported",
    WebhookFailureCode::Permanent => "permanent",
    WebhookFailureCode::Cancelled => "cancelled",
    WebhookFailureCode::Unavailable => "unavailable",
    WebhookFailureCode::InvalidResponse => "invalid_response",
    WebhookFailureCode::NormalizedEventMismatch => "normalized_event_mismatch",
    WebhookFailureCode::DeliveryIdentityConflict => "delivery_identity_conflict",
    WebhookFailureCode::TriggerEvaluationFailed => "trigger_evaluation_failed",
  }
}

pub(super) fn parse_failure_code(value: &str) -> Result<WebhookFailureCode, StoreError> {
  match value {
    "authentication_failed" => Ok(WebhookFailureCode::AuthenticationFailed),
    "invalid_configuration" => Ok(WebhookFailureCode::InvalidConfiguration),
    "unsupported" => Ok(WebhookFailureCode::Unsupported),
    "permanent" => Ok(WebhookFailureCode::Permanent),
    "cancelled" => Ok(WebhookFailureCode::Cancelled),
    "unavailable" => Ok(WebhookFailureCode::Unavailable),
    "invalid_response" => Ok(WebhookFailureCode::InvalidResponse),
    "normalized_event_mismatch" => Ok(WebhookFailureCode::NormalizedEventMismatch),
    "delivery_identity_conflict" => Ok(WebhookFailureCode::DeliveryIdentityConflict),
    "trigger_evaluation_failed" => Ok(WebhookFailureCode::TriggerEvaluationFailed),
    _ => Err(StoreError::Unavailable),
  }
}

pub(super) fn delivery_state(value: &str) -> Result<WebhookDeliveryState, StoreError> {
  match value {
    "pending_verification" => Ok(WebhookDeliveryState::PendingVerification),
    "retry_scheduled" => Ok(WebhookDeliveryState::RetryScheduled),
    "pending_trigger" => Ok(WebhookDeliveryState::PendingTrigger),
    "completed" => Ok(WebhookDeliveryState::Completed),
    "suppressed" => Ok(WebhookDeliveryState::Suppressed),
    "dead_letter" => Ok(WebhookDeliveryState::DeadLetter),
    _ => Err(StoreError::Unavailable),
  }
}

pub(super) fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}
