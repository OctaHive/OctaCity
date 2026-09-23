use std::collections::BTreeMap;

use octacity_server_domain::{EntityKind, IntegrationId, Timestamp, TriggerId, TriggerIdentity, TriggerVersion};
use octacity_server_store::{
  ClaimManagedWebhookOperations, ClaimWebhookDeliveries, CompleteWebhookDelivery, CreateManagedWebhook,
  CreateUnmanagedWebhook, EnqueueManagedWebhookOperation, EnqueueWebhookDelivery, FailManagedWebhookOperation,
  FailWebhookDelivery, IdempotencyKey, ManagedWebhookDefinition, ManagedWebhookMutationOutcome,
  ManagedWebhookOperation, ManagedWebhookOperationClaim, ManagedWebhookRecord, ManagedWebhookRegistration,
  MutationDisposition, RecordManagedWebhookRegistration, RecordWebhookEvent, RecordWebhookEventOutcome, StoreError,
  StoreOperation, SuppressWebhookDelivery, TriggerDefinitionRef, TriggerKind, TriggerTarget,
  UnmanagedWebhookMutationOutcome, WebhookDeliveryClaim, WebhookDeliveryDiagnostic, WebhookDeliveryId,
  WebhookDeliveryState, WebhookDeliveryWork, WebhookFailureCode, WebhookIntegrationRecord,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, types::Json};

use crate::{
  database::{classify, number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

mod decoding;
mod delivery;

pub(crate) use delivery::{
  claim_deliveries, complete_delivery, delivery_diagnostic, enqueue_delivery, fail_delivery, record_event,
  suppress_delivery,
};

use decoding::{
  DeliveryClaimRow, DeliveryDiagnosticRow, ManagedOperationClaimRow, decode_delivery_claim,
  decode_managed_operation_claim, delivery_state, failure_code, managed_operation, parse_failure_code, timestamp,
};

#[derive(Serialize)]
struct CreateFingerprint<'a> {
  trigger_version: octacity_server_domain::TriggerVersion,
  configuration_id: octacity_server_domain::BuildConfigurationId,
  configuration_version: octacity_server_domain::BuildConfigurationVersion,
  enabled: bool,
  definition: &'a octacity_server_store::UnmanagedWebhookDefinition,
}

#[derive(Serialize)]
struct CreateManagedFingerprint<'a> {
  trigger_version: octacity_server_domain::TriggerVersion,
  configuration_id: octacity_server_domain::BuildConfigurationId,
  configuration_version: octacity_server_domain::BuildConfigurationVersion,
  enabled: bool,
  definition: &'a ManagedWebhookDefinition,
}

#[derive(Serialize)]
struct RegistrationFingerprint<'a> {
  integration_id: IntegrationId,
  registration: &'a ManagedWebhookRegistration,
}

pub(crate) async fn create(
  pool: &sqlx::PgPool,
  request: CreateUnmanagedWebhook,
) -> Result<UnmanagedWebhookMutationOutcome, StoreError> {
  request.validate()?;
  let identity = MutationIdentity::new(
    MutationKind::CreateUnmanagedWebhook,
    request.idempotency_key.to_string(),
    request.created_at,
    EntityKind::Integration,
    &CreateFingerprint {
      trigger_version: request.trigger.version,
      configuration_id: request.trigger.configuration_id,
      configuration_version: request.trigger.configuration_version,
      enabled: request.trigger.enabled,
      definition: &request.definition,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => {
      let mut outcome: UnmanagedWebhookMutationOutcome = decode_outcome(outcome)?;
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
  };
  let trigger_version = number(request.trigger.version.get(), StoreOperation::CreateUnmanagedWebhook)?;
  let configuration_version = number(
    request.trigger.configuration_version.get(),
    StoreOperation::CreateUnmanagedWebhook,
  )?;
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, $2, $3, $4, 'external', $5, $6, to_timestamp($7::double precision / 1000.0))",
  )
  .bind(request.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(request.trigger.configuration_id.as_uuid())
  .bind(configuration_version)
  .bind(request.trigger.enabled)
  .bind(Json(&request.trigger.definition))
  .bind(request.created_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;
  sqlx::query(
    "INSERT INTO webhook_integrations (id, trigger_id, trigger_version, definition, created_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0))",
  )
  .bind(request.integration_id.as_uuid())
  .bind(request.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(Json(&request.definition))
  .bind(request.created_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Integration))?;
  let outcome = UnmanagedWebhookMutationOutcome {
    disposition: MutationDisposition::Applied,
    integration_id: request.integration_id,
    trigger: TriggerDefinitionRef {
      id: request.trigger.id,
      version: request.trigger.version,
    },
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "management_api",
      actor_identity: None,
      target_identity: request.integration_id.to_string(),
      safe_metadata: json!({
        "trigger_id": request.trigger.id,
        "trigger_version": request.trigger.version,
        "adapter_id": request.definition.adapter_id,
      }),
      outbox_payload: json!({
        "integration_id": request.integration_id,
        "trigger_id": request.trigger.id,
        "trigger_version": request.trigger.version,
      }),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn read(
  pool: &sqlx::PgPool,
  integration_id: IntegrationId,
) -> Result<WebhookIntegrationRecord, StoreError> {
  let row = sqlx::query_as::<_, (uuid::Uuid, i64, uuid::Uuid, i64, bool, String, Json<Value>)>(
    "SELECT integration.trigger_id, integration.trigger_version, trigger.build_configuration_id, \
       trigger.build_configuration_version, trigger.enabled, trigger.kind, integration.definition \
     FROM webhook_integrations integration \
     JOIN triggers trigger ON trigger.id = integration.trigger_id AND trigger.version = integration.trigger_version \
     WHERE integration.id = $1",
  )
  .bind(integration_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Integration,
  })?;
  if row.5 != TriggerKind::External.as_str() {
    return Err(StoreError::Unavailable);
  }
  let definition: octacity_server_store::UnmanagedWebhookDefinition =
    serde_json::from_value(row.6.0).map_err(|_| StoreError::Unavailable)?;
  definition.validate().map_err(|_| StoreError::Unavailable)?;
  Ok(WebhookIntegrationRecord {
    integration_id,
    trigger: TriggerDefinitionRef {
      id: TriggerId::from_uuid(row.0).map_err(|_| StoreError::Unavailable)?,
      version: TriggerVersion::new(u64::try_from(row.1).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
    },
    target: TriggerTarget {
      configuration_id: octacity_server_domain::BuildConfigurationId::from_uuid(row.2)
        .map_err(|_| StoreError::Unavailable)?,
      configuration_version: octacity_server_domain::BuildConfigurationVersion::new(
        u64::try_from(row.3).map_err(|_| StoreError::Unavailable)?,
      )
      .map_err(|_| StoreError::Unavailable)?,
    },
    enabled: row.4,
    definition,
  })
}

pub(crate) async fn create_managed(
  pool: &sqlx::PgPool,
  request: CreateManagedWebhook,
) -> Result<ManagedWebhookMutationOutcome, StoreError> {
  request.validate()?;
  let identity = MutationIdentity::new(
    MutationKind::CreateManagedWebhook,
    request.idempotency_key.to_string(),
    request.created_at,
    EntityKind::Integration,
    &CreateManagedFingerprint {
      trigger_version: request.trigger.version,
      configuration_id: request.trigger.configuration_id,
      configuration_version: request.trigger.configuration_version,
      enabled: request.trigger.enabled,
      definition: &request.definition,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => {
      let mut outcome: ManagedWebhookMutationOutcome = decode_outcome(outcome)?;
      outcome.disposition = MutationDisposition::Replayed;
      outcome.registration = read_managed(pool, outcome.integration_id).await?.registration;
      return Ok(outcome);
    }
  };
  let trigger_version = number(request.trigger.version.get(), StoreOperation::CreateManagedWebhook)?;
  let configuration_version = number(
    request.trigger.configuration_version.get(),
    StoreOperation::CreateManagedWebhook,
  )?;
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, $2, $3, $4, 'external', $5, $6, to_timestamp($7::double precision / 1000.0))",
  )
  .bind(request.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(request.trigger.configuration_id.as_uuid())
  .bind(configuration_version)
  .bind(request.trigger.enabled)
  .bind(Json(&request.trigger.definition))
  .bind(request.created_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;
  sqlx::query(
    "INSERT INTO webhook_integrations \
       (id, trigger_id, trigger_version, definition, created_at, management_mode, administration_credential_handle) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0), 'managed', $6)",
  )
  .bind(request.integration_id.as_uuid())
  .bind(request.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(Json(&request.definition.delivery))
  .bind(request.created_at.unix_millis())
  .bind(&request.definition.administration_credential_handle)
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Integration))?;
  sqlx::query(
    "INSERT INTO managed_webhook_operations \
       (integration_id, operation, idempotency_key, requested_at, next_attempt_at) \
     VALUES ($1, 'create', $2, to_timestamp($3::double precision / 1000.0), \
       to_timestamp($3::double precision / 1000.0))",
  )
  .bind(request.integration_id.as_uuid())
  .bind(request.idempotency_key.as_str())
  .bind(request.created_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Integration))?;
  let outcome = ManagedWebhookMutationOutcome {
    disposition: MutationDisposition::Applied,
    integration_id: request.integration_id,
    trigger: TriggerDefinitionRef {
      id: request.trigger.id,
      version: request.trigger.version,
    },
    registration: None,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "management_api",
      actor_identity: None,
      target_identity: request.integration_id.to_string(),
      safe_metadata: json!({
        "trigger_id": request.trigger.id,
        "trigger_version": request.trigger.version,
        "adapter_id": request.definition.delivery.adapter_id,
        "management_mode": "managed",
      }),
      outbox_payload: json!({
        "integration_id": request.integration_id,
        "trigger_id": request.trigger.id,
        "trigger_version": request.trigger.version,
        "registration_state": "pending",
      }),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn read_managed(
  pool: &sqlx::PgPool,
  integration_id: IntegrationId,
) -> Result<ManagedWebhookRecord, StoreError> {
  let row = sqlx::query_as::<
    _,
    (
      uuid::Uuid,
      i64,
      uuid::Uuid,
      i64,
      bool,
      String,
      Json<Value>,
      String,
      Option<Json<Value>>,
    ),
  >(
    "SELECT integration.trigger_id, integration.trigger_version, trigger.build_configuration_id, \
       trigger.build_configuration_version, trigger.enabled, trigger.kind, integration.definition, \
       integration.administration_credential_handle, integration.remote_registration \
     FROM webhook_integrations integration \
     JOIN triggers trigger ON trigger.id = integration.trigger_id AND trigger.version = integration.trigger_version \
     WHERE integration.id = $1 AND integration.management_mode = 'managed'",
  )
  .bind(integration_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Integration,
  })?;
  if row.5 != TriggerKind::External.as_str() {
    return Err(StoreError::Unavailable);
  }
  let definition = ManagedWebhookDefinition {
    delivery: serde_json::from_value(row.6.0).map_err(|_| StoreError::Unavailable)?,
    administration_credential_handle: row.7,
  };
  definition.validate().map_err(|_| StoreError::Unavailable)?;
  let registration = row
    .8
    .map(|value| serde_json::from_value::<ManagedWebhookRegistration>(value.0))
    .transpose()
    .map_err(|_| StoreError::Unavailable)?;
  if registration.as_ref().is_some_and(|value| value.validate().is_err()) {
    return Err(StoreError::Unavailable);
  }
  Ok(ManagedWebhookRecord {
    integration_id,
    trigger: TriggerDefinitionRef {
      id: TriggerId::from_uuid(row.0).map_err(|_| StoreError::Unavailable)?,
      version: TriggerVersion::new(u64::try_from(row.1).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
    },
    target: TriggerTarget {
      configuration_id: octacity_server_domain::BuildConfigurationId::from_uuid(row.2)
        .map_err(|_| StoreError::Unavailable)?,
      configuration_version: octacity_server_domain::BuildConfigurationVersion::new(
        u64::try_from(row.3).map_err(|_| StoreError::Unavailable)?,
      )
      .map_err(|_| StoreError::Unavailable)?,
    },
    enabled: row.4,
    definition,
    registration,
  })
}

pub(crate) async fn record_managed(
  pool: &sqlx::PgPool,
  request: RecordManagedWebhookRegistration,
) -> Result<ManagedWebhookMutationOutcome, StoreError> {
  request.validate()?;
  let kind = match request.operation {
    ManagedWebhookOperation::Create => MutationKind::CompleteManagedWebhookCreate,
    ManagedWebhookOperation::Observe => MutationKind::ObserveManagedWebhook,
    ManagedWebhookOperation::Rotate => MutationKind::RotateManagedWebhook,
    ManagedWebhookOperation::Delete => MutationKind::DeleteManagedWebhook,
  };
  let identity = MutationIdentity::new(
    kind,
    request.idempotency_key.to_string(),
    request.observed_at,
    EntityKind::Integration,
    &RegistrationFingerprint {
      integration_id: request.integration_id,
      registration: &request.registration,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => {
      let mut outcome: ManagedWebhookMutationOutcome = decode_outcome(outcome)?;
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
  };
  let trigger = sqlx::query_as::<_, (uuid::Uuid, i64)>(
    "SELECT trigger_id, trigger_version FROM webhook_integrations \
     WHERE id = $1 AND management_mode = 'managed' FOR UPDATE",
  )
  .bind(request.integration_id.as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Integration,
  })?;
  let operation_completed = sqlx::query(
    "UPDATE managed_webhook_operations SET state = 'completed', next_attempt_at = NULL, \
       failure_code = NULL, diagnostic = NULL, completed_at = to_timestamp($1::double precision / 1000.0), \
       claim_owner = NULL, claim_expires_at = NULL \
     WHERE integration_id = $2 AND operation = $3 AND idempotency_key = $4 \
       AND state IN ('pending', 'retry_scheduled') \
       AND claim_owner = $5",
  )
  .bind(request.observed_at.unix_millis())
  .bind(request.integration_id.as_uuid())
  .bind(managed_operation(request.operation))
  .bind(request.idempotency_key.as_str())
  .bind(request.owner.as_str())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if operation_completed.rows_affected() != 1 {
    return Err(StoreError::Conflict {
      entity: EntityKind::Integration,
    });
  }
  sqlx::query(
    "UPDATE webhook_integrations SET remote_registration = $1, \
       registration_observed_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
  )
  .bind(Json(&request.registration))
  .bind(request.observed_at.unix_millis())
  .bind(request.integration_id.as_uuid())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if request.operation == ManagedWebhookOperation::Delete {
    sqlx::query("UPDATE triggers SET enabled = FALSE WHERE id = $1 AND version = $2")
      .bind(trigger.0)
      .bind(trigger.1)
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
  }
  let outcome = ManagedWebhookMutationOutcome {
    disposition: MutationDisposition::Applied,
    integration_id: request.integration_id,
    trigger: TriggerDefinitionRef {
      id: TriggerId::from_uuid(trigger.0).map_err(|_| StoreError::Unavailable)?,
      version: TriggerVersion::new(u64::try_from(trigger.1).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
    },
    registration: Some(request.registration.clone()),
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "management_api",
      actor_identity: None,
      target_identity: request.integration_id.to_string(),
      safe_metadata: json!({
        "operation": request.operation,
        "registration_status": request.registration.status,
      }),
      outbox_payload: json!({
        "integration_id": request.integration_id,
        "operation": request.operation,
        "registration_status": request.registration.status,
      }),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn enqueue_managed_operation(
  pool: &sqlx::PgPool,
  request: EnqueueManagedWebhookOperation,
) -> Result<MutationDisposition, StoreError> {
  let result = sqlx::query(
    "INSERT INTO managed_webhook_operations \
       (integration_id, operation, idempotency_key, requested_at, next_attempt_at) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0), \
       to_timestamp($4::double precision / 1000.0)) \
     ON CONFLICT (integration_id, operation, idempotency_key) DO NOTHING",
  )
  .bind(request.integration_id.as_uuid())
  .bind(managed_operation(request.operation))
  .bind(request.idempotency_key.as_str())
  .bind(request.requested_at.unix_millis())
  .execute(pool)
  .await
  .map_err(|error| classify(error, EntityKind::Integration))?;
  Ok(if result.rows_affected() == 1 {
    MutationDisposition::Applied
  } else {
    MutationDisposition::Replayed
  })
}

pub(crate) async fn claim_managed_operations(
  pool: &sqlx::PgPool,
  request: ClaimManagedWebhookOperations,
) -> Result<Vec<ManagedWebhookOperationClaim>, StoreError> {
  let rows = sqlx::query_as::<_, ManagedOperationClaimRow>(
    "WITH available AS (\
       SELECT integration_id, operation, idempotency_key FROM managed_webhook_operations \
       WHERE state IN ('pending', 'retry_scheduled') \
         AND next_attempt_at <= to_timestamp($1::double precision / 1000.0) \
         AND (claim_expires_at IS NULL OR claim_expires_at <= to_timestamp($1::double precision / 1000.0)) \
       ORDER BY next_attempt_at, requested_at, integration_id FOR UPDATE SKIP LOCKED LIMIT $2\
     ), claimed AS (\
       UPDATE managed_webhook_operations AS work SET claim_owner = $3, \
         claim_expires_at = to_timestamp($4::double precision / 1000.0), attempts = work.attempts + 1 \
       FROM available WHERE work.integration_id = available.integration_id \
         AND work.operation = available.operation AND work.idempotency_key = available.idempotency_key \
       RETURNING work.*\
     ) SELECT claimed.integration_id, claimed.operation, claimed.idempotency_key, claimed.attempts, \
       integration.trigger_id, integration.trigger_version, trigger.build_configuration_id, \
       trigger.build_configuration_version, trigger.enabled, trigger.kind AS trigger_kind, \
       integration.definition, integration.administration_credential_handle, integration.remote_registration \
     FROM claimed JOIN webhook_integrations integration ON integration.id = claimed.integration_id \
     JOIN triggers trigger ON trigger.id = integration.trigger_id AND trigger.version = integration.trigger_version \
     ORDER BY claimed.next_attempt_at, claimed.requested_at, claimed.integration_id",
  )
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  rows
    .into_iter()
    .map(|row| decode_managed_operation_claim(row, &request))
    .collect()
}

pub(crate) async fn fail_managed_operation(
  pool: &sqlx::PgPool,
  request: FailManagedWebhookOperation,
) -> Result<(), StoreError> {
  request.validate()?;
  let state = if request.retry_at.is_some() {
    "retry_scheduled"
  } else {
    "dead_letter"
  };
  let result = sqlx::query(
    "UPDATE managed_webhook_operations SET state = $1, failure_code = $2, diagnostic = $3, \
       next_attempt_at = CASE WHEN $4::bigint IS NULL THEN NULL \
         ELSE to_timestamp($4::double precision / 1000.0) END, \
       completed_at = CASE WHEN $4::bigint IS NULL THEN to_timestamp($5::double precision / 1000.0) ELSE NULL END, \
       claim_owner = NULL, claim_expires_at = NULL \
     WHERE integration_id = $6 AND operation = $7 AND idempotency_key = $8 \
       AND state IN ('pending', 'retry_scheduled') \
       AND claim_owner = $9",
  )
  .bind(state)
  .bind(failure_code(request.code))
  .bind(request.diagnostic)
  .bind(request.retry_at.map(Timestamp::unix_millis))
  .bind(request.failed_at.unix_millis())
  .bind(request.integration_id.as_uuid())
  .bind(managed_operation(request.operation))
  .bind(request.idempotency_key.as_str())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if result.rows_affected() == 1 {
    Ok(())
  } else {
    Err(StoreError::Conflict {
      entity: EntityKind::Integration,
    })
  }
}
