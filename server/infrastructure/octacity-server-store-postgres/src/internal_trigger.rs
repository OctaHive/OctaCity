use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, BuildId, EntityKind, ImmutableRevision, Timestamp, TriggerId,
  TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_store::{
  ClaimInternalTriggerEvents, CompleteInternalTriggerEvent, InternalTriggerEventClaim, InternalTriggerMatch,
  MAX_INTERNAL_TRIGGER_MATCHES_PER_EVENT, MutationDisposition, NormalizedTriggerOccurrence, StoreError,
  StoreInputError, StoreOperation, TerminalBuildEvent, TriggerCausality, TriggerCause, TriggerDefinitionRef,
  TriggerMetadata, TriggerTarget,
};
use sqlx::{FromRow, PgPool, types::Json};
use uuid::Uuid;

use crate::database::unavailable;

#[derive(FromRow)]
struct ClaimedEventRow {
  id: Uuid,
  topic: String,
  aggregate_identity: String,
  occurred_at_millis: i64,
}

#[derive(FromRow)]
struct OccurrenceRow {
  id: Uuid,
  trigger_id: Uuid,
  trigger_version: i64,
  build_configuration_id: Uuid,
  build_configuration_version: i64,
  deduplication_identity: String,
  cause: Json<TriggerCause>,
  causality: Json<TriggerCausality>,
  provider_metadata: Json<TriggerMetadata>,
  source_time_millis: i64,
}

#[derive(FromRow)]
struct MatchRow {
  trigger_id: Uuid,
  trigger_version: i64,
  build_configuration_id: Uuid,
  build_configuration_version: i64,
  definition: Json<serde_json::Value>,
}

#[derive(FromRow)]
struct SourceBuildRow {
  id: Uuid,
  build_configuration_id: Uuid,
  build_configuration_version: i64,
  immutable_revision: String,
}

struct SourceBuild {
  id: BuildId,
  target: TriggerTarget,
  revision: ImmutableRevision,
}

pub(crate) async fn claim(
  pool: &PgPool,
  request: ClaimInternalTriggerEvents,
) -> Result<Vec<InternalTriggerEventClaim>, StoreError> {
  let rows = sqlx::query_as::<_, ClaimedEventRow>(
    "WITH available AS (\
       SELECT id FROM outbox_entries \
       WHERE published_at IS NULL \
         AND available_at <= to_timestamp($1::double precision / 1000.0) \
         AND (claim_expires_at IS NULL OR claim_expires_at <= to_timestamp($1::double precision / 1000.0)) \
         AND (\
           (topic = 'job.completed' AND payload ->> 'build_state' IN ('succeeded', 'failed', 'cancelled')) \
           OR (topic = 'build.cancellation-requested' AND payload ->> 'build_state' = 'cancelled')\
         ) \
       ORDER BY available_at, id FOR UPDATE SKIP LOCKED LIMIT $2\
     ), claimed AS (\
       UPDATE outbox_entries AS entry SET claim_owner = $3, \
         claim_expires_at = to_timestamp($4::double precision / 1000.0), \
         attempt_count = attempt_count + 1 \
       FROM available WHERE entry.id = available.id \
       RETURNING entry.id, entry.topic, entry.aggregate_identity, entry.created_at\
     ) SELECT id, topic, aggregate_identity, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS occurred_at_millis \
     FROM claimed ORDER BY created_at, id",
  )
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;

  let mut claims = Vec::with_capacity(rows.len());
  for row in rows {
    claims.push(load_claim(pool, &request, row).await?);
  }
  Ok(claims)
}

async fn load_claim(
  pool: &PgPool,
  request: &ClaimInternalTriggerEvents,
  event: ClaimedEventRow,
) -> Result<InternalTriggerEventClaim, StoreError> {
  let build = source_build(pool, &event).await?;
  let source_occurrence = source_occurrence(pool, build.id).await?;
  let trigger_ancestry = ancestry(pool, source_occurrence.id).await?;
  octacity_server_store::validate_internal_ancestry(&source_occurrence, &trigger_ancestry)
    .map_err(|_| StoreError::Unavailable)?;
  let event_kind = event_kind(pool, build.id).await?;
  let matches = matching_triggers(pool, build.target, &event_kind, event.occurred_at_millis).await?;
  Ok(InternalTriggerEventClaim {
    event_identity: TriggerIdentity::new(event.id.to_string()).map_err(|_| StoreError::Unavailable)?,
    source_build_id: build.id,
    source_revision: build.revision,
    source_target: build.target,
    source_occurrence,
    trigger_ancestry,
    event_kind,
    occurred_at: timestamp(event.occurred_at_millis)?,
    matches,
    owner: request.owner.clone(),
    claim_expires_at: request.claim_expires_at,
  })
}

async fn source_build(pool: &PgPool, event: &ClaimedEventRow) -> Result<SourceBuild, StoreError> {
  let row: Option<SourceBuildRow> = match event.topic.as_str() {
    "job.completed" => sqlx::query_as(
      "SELECT build.id, build.build_configuration_id, build.build_configuration_version, build.immutable_revision \
       FROM jobs AS job JOIN attempts AS attempt ON attempt.id = job.attempt_id \
       JOIN builds AS build ON build.id = attempt.build_id WHERE job.id::text = $1",
    )
    .bind(&event.aggregate_identity)
    .fetch_optional(pool)
    .await
    .map_err(unavailable)?,
    "build.cancellation-requested" => sqlx::query_as(
      "SELECT id, build_configuration_id, build_configuration_version, immutable_revision \
       FROM builds WHERE id::text = $1",
    )
    .bind(&event.aggregate_identity)
    .fetch_optional(pool)
    .await
    .map_err(unavailable)?,
    _ => None,
  };
  let row = row.ok_or(StoreError::NotFound {
    entity: EntityKind::Build,
  })?;
  Ok(SourceBuild {
    id: BuildId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    target: TriggerTarget {
      configuration_id: BuildConfigurationId::from_uuid(row.build_configuration_id)
        .map_err(|_| StoreError::Unavailable)?,
      configuration_version: positive_configuration_version(row.build_configuration_version)?,
    },
    revision: ImmutableRevision::new(row.immutable_revision).map_err(|_| StoreError::Unavailable)?,
  })
}

async fn source_occurrence(pool: &PgPool, build_id: BuildId) -> Result<NormalizedTriggerOccurrence, StoreError> {
  let row = sqlx::query_as::<_, OccurrenceRow>(
    "SELECT occurrence.id, occurrence.trigger_id, occurrence.trigger_version, \
       occurrence.build_configuration_id, occurrence.build_configuration_version, \
       occurrence.deduplication_identity, occurrence.cause, occurrence.causality, \
       occurrence.provider_metadata, \
       FLOOR(EXTRACT(EPOCH FROM occurrence.source_time) * 1000)::BIGINT AS source_time_millis \
     FROM builds AS build JOIN trigger_occurrences AS occurrence ON occurrence.id = build.trigger_occurrence_id \
     WHERE build.id = $1",
  )
  .bind(build_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Build,
  })?;
  decode_occurrence(row)
}

async fn ancestry(
  pool: &PgPool,
  source_occurrence_id: TriggerOccurrenceId,
) -> Result<Vec<TriggerDefinitionRef>, StoreError> {
  let rows: Vec<(Uuid, i64)> = sqlx::query_as(
    "WITH RECURSIVE lineage AS (\
       SELECT id, trigger_id, trigger_version, \
         NULLIF(causality ->> 'parent_occurrence_id', '')::uuid AS parent_id, \
         (causality ->> 'depth')::integer AS depth \
       FROM trigger_occurrences WHERE id = $1 \
       UNION ALL \
       SELECT parent.id, parent.trigger_id, parent.trigger_version, \
         NULLIF(parent.causality ->> 'parent_occurrence_id', '')::uuid, \
         (parent.causality ->> 'depth')::integer \
       FROM trigger_occurrences AS parent JOIN lineage ON parent.id = lineage.parent_id\
     ) SELECT trigger_id, trigger_version FROM lineage ORDER BY depth",
  )
  .bind(source_occurrence_id.as_uuid())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  rows
    .into_iter()
    .map(|(id, version)| {
      Ok(TriggerDefinitionRef {
        id: TriggerId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
        version: positive_version(version)?,
      })
    })
    .collect()
}

async fn event_kind(pool: &PgPool, build_id: BuildId) -> Result<TerminalBuildEvent, StoreError> {
  let state: String = sqlx::query_scalar("SELECT state FROM builds WHERE id = $1")
    .bind(build_id.as_uuid())
    .fetch_one(pool)
    .await
    .map_err(unavailable)?;
  TerminalBuildEvent::from_build_state(&state).ok_or(StoreError::Unavailable)
}

async fn matching_triggers(
  pool: &PgPool,
  source: TriggerTarget,
  event_kind: &TerminalBuildEvent,
  occurred_at_millis: i64,
) -> Result<Vec<InternalTriggerMatch>, StoreError> {
  let rows = sqlx::query_as::<_, MatchRow>(
    "SELECT trigger_id, trigger_version, build_configuration_id, build_configuration_version, definition FROM (\
       SELECT DISTINCT ON (id) id AS trigger_id, version AS trigger_version, build_configuration_id, \
         build_configuration_version, enabled, definition FROM triggers \
       WHERE kind = 'internal' AND created_at <= to_timestamp($2::double precision / 1000.0) \
       ORDER BY id, version DESC\
     ) AS active \
     WHERE enabled AND definition ->> 'event_kind' = $1 \
       AND definition #>> '{upstream,configuration_id}' = $3 \
       AND definition #>> '{upstream,configuration_version}' = $4 \
     ORDER BY trigger_id LIMIT $5",
  )
  .bind(event_kind.as_str())
  .bind(occurred_at_millis)
  .bind(source.configuration_id.to_string())
  .bind(source.configuration_version.get().to_string())
  .bind(i64::try_from(MAX_INTERNAL_TRIGGER_MATCHES_PER_EVENT + 1).map_err(|_| StoreError::Unavailable)?)
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  if rows.len() > MAX_INTERNAL_TRIGGER_MATCHES_PER_EVENT {
    return Err(StoreError::InvalidInput {
      operation: StoreOperation::ClaimInternalTriggerEvents,
      source: StoreInputError::TooManyInternalTriggerMatches,
    });
  }
  rows.into_iter().map(decode_match).collect()
}

pub(crate) async fn complete(
  pool: &PgPool,
  request: CompleteInternalTriggerEvent,
) -> Result<MutationDisposition, StoreError> {
  let event_id = Uuid::parse_str(request.event_identity.as_str()).map_err(|_| StoreError::InvalidInput {
    operation: StoreOperation::CompleteInternalTriggerEvent,
    source: StoreInputError::InvalidWorkerClaim,
  })?;
  let result = sqlx::query(
    "UPDATE outbox_entries SET published_at = to_timestamp($1::double precision / 1000.0), \
       claim_owner = NULL, claim_expires_at = NULL \
     WHERE id = $2 AND published_at IS NULL AND claim_owner = $3 \
       AND claim_expires_at > to_timestamp($1::double precision / 1000.0)",
  )
  .bind(request.completed_at.unix_millis())
  .bind(event_id)
  .bind(request.owner.as_str())
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

fn decode_occurrence(row: OccurrenceRow) -> Result<NormalizedTriggerOccurrence, StoreError> {
  let occurrence = NormalizedTriggerOccurrence {
    id: TriggerOccurrenceId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    trigger: TriggerDefinitionRef {
      id: TriggerId::from_uuid(row.trigger_id).map_err(|_| StoreError::Unavailable)?,
      version: positive_version(row.trigger_version)?,
    },
    target: TriggerTarget {
      configuration_id: BuildConfigurationId::from_uuid(row.build_configuration_id)
        .map_err(|_| StoreError::Unavailable)?,
      configuration_version: positive_configuration_version(row.build_configuration_version)?,
    },
    deduplication_identity: TriggerIdentity::new(row.deduplication_identity).map_err(|_| StoreError::Unavailable)?,
    cause: row.cause.0,
    causality: row.causality.0,
    provider_metadata: row.provider_metadata.0,
    source_time: timestamp(row.source_time_millis)?,
  };
  occurrence.validate().map_err(|_| StoreError::Unavailable)?;
  Ok(occurrence)
}

fn decode_match(row: MatchRow) -> Result<InternalTriggerMatch, StoreError> {
  Ok(InternalTriggerMatch {
    trigger: TriggerDefinitionRef {
      id: TriggerId::from_uuid(row.trigger_id).map_err(|_| StoreError::Unavailable)?,
      version: positive_version(row.trigger_version)?,
    },
    target: TriggerTarget {
      configuration_id: BuildConfigurationId::from_uuid(row.build_configuration_id)
        .map_err(|_| StoreError::Unavailable)?,
      configuration_version: positive_configuration_version(row.build_configuration_version)?,
    },
    definition: row.definition.0,
  })
}

fn positive_version(value: i64) -> Result<TriggerVersion, StoreError> {
  u64::try_from(value)
    .ok()
    .and_then(|value| TriggerVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)
}

fn positive_configuration_version(value: i64) -> Result<BuildConfigurationVersion, StoreError> {
  u64::try_from(value)
    .ok()
    .and_then(|value| BuildConfigurationVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)
}

fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}
