use octacity_server_domain::{EntityKind, Timestamp, TriggerId, TriggerVersion};
use octacity_server_store::{
  ClaimDueSchedules, CompleteScheduleClaim, CreateSchedule, DueScheduleClaim, MissedRunPolicy, MutationDisposition,
  ScheduleDefinition, ScheduleRecord, StoreError, StoreOperation, TriggerDefinitionMutationOutcome,
  TriggerDefinitionRef, TriggerTarget,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, types::Json};

use crate::{
  database::{classify, number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

#[derive(Serialize)]
struct CreateFingerprint<'a> {
  version: TriggerVersion,
  configuration_id: octacity_server_domain::BuildConfigurationId,
  configuration_version: octacity_server_domain::BuildConfigurationVersion,
  enabled: bool,
  definition: &'a Value,
  schedule: &'a ScheduleDefinition,
}

#[derive(FromRow)]
struct ScheduleRow {
  trigger_id: uuid::Uuid,
  trigger_version: i64,
  build_configuration_id: uuid::Uuid,
  build_configuration_version: i64,
  enabled: bool,
  definition: Json<Value>,
  expression: String,
  timezone: String,
  missed_run_policy: Json<MissedRunPolicy>,
  next_occurrence_at_millis: i64,
}

pub(crate) async fn create(
  pool: &PgPool,
  request: CreateSchedule,
) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
  request.validate()?;
  let trigger = &request.trigger;
  let identity = MutationIdentity::new(
    MutationKind::CreateSchedule,
    trigger.idempotency_key.to_string(),
    trigger.created_at,
    EntityKind::Trigger,
    &CreateFingerprint {
      version: trigger.version,
      configuration_id: trigger.configuration_id,
      configuration_version: trigger.configuration_version,
      enabled: trigger.enabled,
      definition: &trigger.definition,
      schedule: &request.schedule,
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

  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, $2, $3, $4, 'scheduled', $5, $6, to_timestamp($7::double precision / 1000.0))",
  )
  .bind(trigger.id.as_uuid())
  .bind(number(trigger.version.get(), StoreOperation::CreateSchedule)?)
  .bind(trigger.configuration_id.as_uuid())
  .bind(number(
    trigger.configuration_version.get(),
    StoreOperation::CreateSchedule,
  )?)
  .bind(trigger.enabled)
  .bind(Json(&trigger.definition))
  .bind(trigger.created_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;

  sqlx::query(
    "INSERT INTO schedules \
       (trigger_id, trigger_version, expression, timezone, next_occurrence_at, missed_run_policy) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0), $6)",
  )
  .bind(trigger.id.as_uuid())
  .bind(number(trigger.version.get(), StoreOperation::CreateSchedule)?)
  .bind(&request.schedule.expression)
  .bind(&request.schedule.timezone)
  .bind(request.next_occurrence_at.unix_millis())
  .bind(Json(&request.schedule.missed_run_policy))
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;

  let outcome = TriggerDefinitionMutationOutcome {
    disposition: MutationDisposition::Applied,
    trigger_id: trigger.id,
    version: trigger.version,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "management_api",
      actor_identity: None,
      target_identity: trigger.id.to_string(),
      safe_metadata: json!({"version": trigger.version.get(), "kind": "scheduled"}),
      outbox_payload: json!({
        "trigger_id": trigger.id,
        "version": trigger.version,
        "next_occurrence_at": request.next_occurrence_at,
      }),
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
) -> Result<ScheduleRecord, StoreError> {
  let row = sqlx::query_as::<_, ScheduleRow>(
    "SELECT schedule.trigger_id, schedule.trigger_version, trigger.build_configuration_id, \
     trigger.build_configuration_version, trigger.enabled, trigger.definition, schedule.expression, schedule.timezone, \
     schedule.missed_run_policy, FLOOR(EXTRACT(EPOCH FROM schedule.next_occurrence_at) * 1000)::BIGINT \
       AS next_occurrence_at_millis \
     FROM schedules AS schedule JOIN triggers AS trigger \
       ON trigger.id = schedule.trigger_id AND trigger.version = schedule.trigger_version \
     WHERE schedule.trigger_id = $1 AND schedule.trigger_version = $2",
  )
  .bind(trigger_id.as_uuid())
  .bind(number(version.get(), StoreOperation::ReadSchedule)?)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Trigger,
  })?;
  decode(row)
}

pub(crate) async fn claim(pool: &PgPool, request: ClaimDueSchedules) -> Result<Vec<DueScheduleClaim>, StoreError> {
  let rows = sqlx::query_as::<_, ScheduleRow>(
    "WITH due AS (\
       SELECT schedule.trigger_id, schedule.trigger_version FROM schedules AS schedule \
       JOIN triggers AS trigger ON trigger.id = schedule.trigger_id AND trigger.version = schedule.trigger_version \
       WHERE trigger.enabled AND next_occurrence_at <= to_timestamp($1::double precision / 1000.0) \
         AND (claim_expires_at IS NULL OR claim_expires_at <= to_timestamp($1::double precision / 1000.0)) \
       ORDER BY next_occurrence_at, trigger_id, trigger_version \
       FOR UPDATE SKIP LOCKED LIMIT $2\
     ), claimed AS (\
       UPDATE schedules AS schedule SET claim_owner = $3, \
         claim_expires_at = to_timestamp($4::double precision / 1000.0) \
       FROM due WHERE schedule.trigger_id = due.trigger_id AND schedule.trigger_version = due.trigger_version \
       RETURNING schedule.trigger_id, schedule.trigger_version\
     ) SELECT schedule.trigger_id, schedule.trigger_version, trigger.build_configuration_id, \
       trigger.build_configuration_version, trigger.enabled, trigger.definition, schedule.expression, \
       schedule.timezone, schedule.missed_run_policy, \
       FLOOR(EXTRACT(EPOCH FROM schedule.next_occurrence_at) * 1000)::BIGINT AS next_occurrence_at_millis \
     FROM schedules AS schedule JOIN triggers AS trigger \
       ON trigger.id = schedule.trigger_id AND trigger.version = schedule.trigger_version \
     JOIN claimed USING (trigger_id, trigger_version)",
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
    .map(|row| {
      Ok(DueScheduleClaim {
        schedule: decode(row)?,
        owner: request.owner.clone(),
        claim_expires_at: request.claim_expires_at,
      })
    })
    .collect()
}

pub(crate) async fn complete(pool: &PgPool, request: CompleteScheduleClaim) -> Result<MutationDisposition, StoreError> {
  if request.next_occurrence_at <= request.expected_next_occurrence_at {
    return Err(StoreError::InvalidInput {
      operation: StoreOperation::CompleteScheduleClaim,
      source: octacity_server_store::StoreInputError::InvalidWorkerClaim,
    });
  }
  let result = sqlx::query(
    "UPDATE schedules SET next_occurrence_at = to_timestamp($1::double precision / 1000.0), \
       claim_owner = NULL, claim_expires_at = NULL \
     WHERE trigger_id = $2 AND trigger_version = $3 AND next_occurrence_at = to_timestamp($4::double precision / 1000.0) \
       AND claim_owner = $5 AND claim_expires_at > to_timestamp($6::double precision / 1000.0)",
  )
  .bind(request.next_occurrence_at.unix_millis())
  .bind(request.trigger.id.as_uuid())
  .bind(number(request.trigger.version.get(), StoreOperation::CompleteScheduleClaim)?)
  .bind(request.expected_next_occurrence_at.unix_millis())
  .bind(request.owner.as_str())
  .bind(request.completed_at.unix_millis())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if result.rows_affected() == 1 {
    Ok(MutationDisposition::Applied)
  } else {
    Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    })
  }
}

fn decode(row: ScheduleRow) -> Result<ScheduleRecord, StoreError> {
  let trigger_id = TriggerId::from_uuid(row.trigger_id).map_err(|_| StoreError::Unavailable)?;
  let trigger_version = u64::try_from(row.trigger_version)
    .ok()
    .and_then(|value| TriggerVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)?;
  let configuration_id = octacity_server_domain::BuildConfigurationId::from_uuid(row.build_configuration_id)
    .map_err(|_| StoreError::Unavailable)?;
  let configuration_version = u64::try_from(row.build_configuration_version)
    .ok()
    .and_then(|value| octacity_server_domain::BuildConfigurationVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)?;
  let schedule = ScheduleDefinition {
    expression: row.expression,
    timezone: row.timezone,
    missed_run_policy: row.missed_run_policy.0,
  };
  schedule.validate().map_err(|_| StoreError::Unavailable)?;
  Ok(ScheduleRecord {
    trigger: TriggerDefinitionRef {
      id: trigger_id,
      version: trigger_version,
    },
    target: TriggerTarget {
      configuration_id,
      configuration_version,
    },
    enabled: row.enabled,
    definition: row.definition.0,
    schedule,
    next_occurrence_at: Timestamp::from_unix_millis(row.next_occurrence_at_millis)
      .map_err(|_| StoreError::Unavailable)?,
  })
}
