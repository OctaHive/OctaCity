use octacity_server_domain::{EntityKind, Timestamp, TriggerId, TriggerVersion};
use octacity_server_store::{
  CreateInternalTriggerDefinition, InternalTriggerDefinitionPage, InternalTriggerDefinitionRecord,
  ListInternalTriggerDefinitions, MutationDisposition, PublishInternalTriggerVersion, StoreError, StoreInputError,
  StoreOperation, TriggerDefinitionMutationOutcome, TriggerDefinitionRef, TriggerTarget,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, types::Json};

use crate::{
  database::{classify, number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

#[derive(Serialize)]
struct PublishFingerprint<'a> {
  id: TriggerId,
  expected_current_version: TriggerVersion,
  target: TriggerTarget,
  enabled: bool,
  definition: &'a Value,
}

#[derive(FromRow)]
struct DefinitionRow {
  trigger_id: uuid::Uuid,
  trigger_version: i64,
  build_configuration_id: uuid::Uuid,
  build_configuration_version: i64,
  enabled: bool,
  definition: Json<Value>,
  created_at_millis: i64,
}

pub(crate) async fn create(
  pool: &PgPool,
  request: CreateInternalTriggerDefinition,
) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
  request.validate()?;
  crate::definition_mutation::create_trigger(pool, request.trigger).await
}

pub(crate) async fn publish(
  pool: &PgPool,
  request: PublishInternalTriggerVersion,
) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
  request.validate()?;
  let identity = MutationIdentity::new(
    MutationKind::PublishInternalTriggerVersion,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Trigger,
    &PublishFingerprint {
      id: request.id,
      expected_current_version: request.expected_current_version,
      target: request.target,
      enabled: request.enabled,
      definition: &request.definition,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => {
      let mut outcome: TriggerDefinitionMutationOutcome = decode_outcome(outcome)?;
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
  };

  let current: Option<(i64, i64)> = sqlx::query_as(
    "SELECT version, FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT \
     FROM triggers WHERE id = $1 AND kind = 'internal' ORDER BY version DESC LIMIT 1 FOR UPDATE",
  )
  .bind(request.id.as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let Some((current_version, current_created_at)) = current else {
    return Err(StoreError::NotFound {
      entity: EntityKind::Trigger,
    });
  };
  if current_version
    != number(
      request.expected_current_version.get(),
      StoreOperation::PublishInternalTriggerVersion,
    )?
  {
    return Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    });
  }
  if request.published_at.unix_millis() <= current_created_at {
    return Err(StoreError::InvalidInput {
      operation: StoreOperation::PublishInternalTriggerVersion,
      source: StoreInputError::InvalidNormalizedTrigger,
    });
  }
  let next_version = request
    .expected_current_version
    .next()
    .map_err(|_| StoreError::Unavailable)?;
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, $2, $3, $4, 'internal', $5, $6, to_timestamp($7::double precision / 1000.0))",
  )
  .bind(request.id.as_uuid())
  .bind(number(
    next_version.get(),
    StoreOperation::PublishInternalTriggerVersion,
  )?)
  .bind(request.target.configuration_id.as_uuid())
  .bind(number(
    request.target.configuration_version.get(),
    StoreOperation::PublishInternalTriggerVersion,
  )?)
  .bind(request.enabled)
  .bind(Json(&request.definition))
  .bind(request.published_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;

  let outcome = TriggerDefinitionMutationOutcome {
    disposition: MutationDisposition::Applied,
    trigger_id: request.id,
    version: next_version,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "management_api",
      actor_identity: None,
      target_identity: request.id.to_string(),
      safe_metadata: json!({"version": next_version.get(), "kind": "internal", "enabled": request.enabled}),
      outbox_payload: json!({"trigger_id": request.id, "version": next_version}),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn read(
  pool: &PgPool,
  trigger_id: TriggerId,
  version: TriggerVersion,
) -> Result<InternalTriggerDefinitionRecord, StoreError> {
  let row = sqlx::query_as::<_, DefinitionRow>(
    "SELECT id AS trigger_id, version AS trigger_version, build_configuration_id, \
       build_configuration_version, enabled, definition, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis \
     FROM triggers WHERE id = $1 AND version = $2 AND kind = 'internal'",
  )
  .bind(trigger_id.as_uuid())
  .bind(number(version.get(), StoreOperation::ReadInternalTriggerDefinition)?)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Trigger,
  })?;
  decode(row)
}

pub(crate) async fn list(
  pool: &PgPool,
  request: ListInternalTriggerDefinitions,
) -> Result<InternalTriggerDefinitionPage, StoreError> {
  let mut rows = sqlx::query_as::<_, DefinitionRow>(
    "SELECT trigger_id, trigger_version, build_configuration_id, build_configuration_version, enabled, definition, \
       created_at_millis FROM (\
       SELECT DISTINCT ON (id) id AS trigger_id, version AS trigger_version, build_configuration_id, \
         build_configuration_version, enabled, definition, \
         FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis \
       FROM triggers WHERE kind = 'internal' AND ($1::uuid IS NULL OR id > $1) \
       ORDER BY id, version DESC\
     ) AS current ORDER BY trigger_id LIMIT $2",
  )
  .bind(request.after.map(TriggerId::as_uuid))
  .bind(i64::from(request.limit.get()) + 1)
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let has_more = rows.len() > usize::from(request.limit.get());
  if has_more {
    rows.pop();
  }
  let items = rows.into_iter().map(decode).collect::<Result<Vec<_>, _>>()?;
  let next_after = has_more.then(|| items.last().expect("non-empty bounded page").trigger.id);
  Ok(InternalTriggerDefinitionPage { items, next_after })
}

fn decode(row: DefinitionRow) -> Result<InternalTriggerDefinitionRecord, StoreError> {
  Ok(InternalTriggerDefinitionRecord {
    trigger: TriggerDefinitionRef {
      id: TriggerId::from_uuid(row.trigger_id).map_err(|_| StoreError::Unavailable)?,
      version: positive_version(row.trigger_version)?,
    },
    target: TriggerTarget {
      configuration_id: octacity_server_domain::BuildConfigurationId::from_uuid(row.build_configuration_id)
        .map_err(|_| StoreError::Unavailable)?,
      configuration_version: u64::try_from(row.build_configuration_version)
        .ok()
        .and_then(|value| octacity_server_domain::BuildConfigurationVersion::new(value).ok())
        .ok_or(StoreError::Unavailable)?,
    },
    enabled: row.enabled,
    definition: row.definition.0,
    created_at: Timestamp::from_unix_millis(row.created_at_millis).map_err(|_| StoreError::Unavailable)?,
  })
}

fn positive_version(value: i64) -> Result<TriggerVersion, StoreError> {
  u64::try_from(value)
    .ok()
    .and_then(|value| TriggerVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)
}
