use std::collections::BTreeSet;

use octacity_server_domain::{AttemptId, BuildId, EntityKind, JobId, TriggerOccurrenceId};
use octacity_server_job::JobSpecSigner;
use octacity_server_store::{
  AcceptTrigger, AcceptTriggerOutcome, MutationDisposition, StoreError, StoreOperation, SuppressTrigger,
  SuppressTriggerOutcome, TriggerAcceptanceProbe, TriggerEvaluationOutcome,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

use crate::{
  database::{classify, number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

pub(crate) async fn execute(
  pool: &PgPool,
  signer: &JobSpecSigner,
  request: AcceptTrigger,
) -> Result<AcceptTriggerOutcome, StoreError> {
  request.validate()?;
  let identity = MutationIdentity::with_digest(
    MutationKind::AcceptTrigger,
    request.trigger.id.to_string(),
    request.accepted_at,
    EntityKind::Trigger,
    request.intent_digest.as_bytes(),
  );
  let trigger_version = number(request.trigger.trigger.version.get(), StoreOperation::AcceptTrigger)?;
  let configuration_version = number(request.build.configuration_version.get(), StoreOperation::AcceptTrigger)?;
  let pipeline_version = number(request.build.pipeline_version.get(), StoreOperation::AcceptTrigger)?;
  let repository_version = number(request.build.repository_version.get(), StoreOperation::AcceptTrigger)?;
  let attempt_number = number(request.attempt_number.get(), StoreOperation::AcceptTrigger)?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_accepted(outcome),
  };

  if let Some(existing) = occurrence_replay(&mut transaction, &request.trigger, trigger_version).await? {
    if existing.digest == identity.request_digest && existing.state == "accepted" {
      let outcome = replay_accepted(existing.outcome)?;
      if outcome.trigger_occurrence_id != existing.occurrence_id {
        return Err(StoreError::Unavailable);
      }
      crate::mutation::commit(
        transaction,
        &identity,
        accepted_facts(&request, &outcome),
        encode_outcome(&StoredOutcome::from(&outcome))?,
      )
      .await?;
      return Ok(outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    });
  }

  require_references(
    &mut transaction,
    &request,
    trigger_version,
    configuration_version,
    pipeline_version,
    repository_version,
  )
  .await?;
  insert_occurrence(
    &mut transaction,
    &request,
    trigger_version,
    configuration_version,
    &identity.request_digest,
  )
  .await?;
  insert_build(
    &mut transaction,
    &request,
    configuration_version,
    pipeline_version,
    repository_version,
  )
  .await?;
  crate::attempt_materialization::insert_attempt(
    &mut transaction,
    request.attempt_id,
    request.build.id,
    attempt_number,
    None,
    octacity_server_orchestrator::AttemptState::Running,
    request.accepted_at,
  )
  .await?;
  crate::attempt_materialization::insert_jobs(&mut transaction, request.attempt_id, &request.jobs, request.accepted_at)
    .await?;
  crate::attempt_materialization::insert_dependencies(&mut transaction, request.attempt_id, &request.jobs).await?;
  crate::attempt_materialization::enqueue_roots(
    &mut transaction,
    signer,
    request.attempt_id,
    &request.jobs,
    request.accepted_at,
  )
  .await?;

  let outcome = outcome(&request, MutationDisposition::Applied);
  crate::mutation::commit(
    transaction,
    &identity,
    accepted_facts(&request, &outcome),
    encode_outcome(&StoredOutcome::from(&outcome))?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn replay_evaluation(
  pool: &PgPool,
  request: TriggerAcceptanceProbe,
) -> Result<Option<TriggerEvaluationOutcome>, StoreError> {
  let trigger_version = number(request.trigger.trigger.version.get(), StoreOperation::AcceptTrigger)?;
  let record: Option<(Uuid, String, Vec<u8>, Json<Value>)> = sqlx::query_as(
    "SELECT occurrence.id, occurrence.state, record.request_digest, record.outcome \
     FROM trigger_occurrences AS occurrence \
     JOIN idempotency_records AS record \
       ON record.scope = CASE occurrence.state \
            WHEN 'accepted' THEN $1 WHEN 'suppressed' THEN $2 ELSE '' END \
      AND record.idempotency_key = occurrence.id::text \
     WHERE occurrence.id = $3 \
        OR (occurrence.trigger_id = $4 AND occurrence.trigger_version = $5 \
            AND occurrence.deduplication_identity = $6) \
     ORDER BY (occurrence.id = $3) DESC \
     LIMIT 1",
  )
  .bind(MutationKind::AcceptTrigger.scope())
  .bind(MutationKind::SuppressTrigger.scope())
  .bind(request.trigger.id.as_uuid())
  .bind(request.trigger.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(request.trigger.deduplication_identity.as_str())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  let Some((occurrence_id, state, digest, Json(outcome))) = record else {
    return Ok(None);
  };
  if digest != request.intent_digest.as_bytes() {
    return Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    });
  }
  let occurrence_id = TriggerOccurrenceId::from_uuid(occurrence_id).map_err(|_| StoreError::Unavailable)?;
  match state.as_str() {
    "accepted" => replay_accepted(outcome).and_then(|outcome| {
      (outcome.trigger_occurrence_id == occurrence_id)
        .then_some(TriggerEvaluationOutcome::Accepted(outcome))
        .ok_or(StoreError::Unavailable)
    }),
    "suppressed" => replay_suppressed(outcome).and_then(|outcome| {
      (outcome.trigger_occurrence_id == occurrence_id)
        .then_some(TriggerEvaluationOutcome::Suppressed(outcome))
        .ok_or(StoreError::Unavailable)
    }),
    _ => Err(StoreError::Unavailable),
  }
  .map(Some)
}

pub(crate) async fn suppress(pool: &PgPool, request: SuppressTrigger) -> Result<SuppressTriggerOutcome, StoreError> {
  request.validate()?;
  let identity = MutationIdentity::with_digest(
    MutationKind::SuppressTrigger,
    request.trigger.id.to_string(),
    request.suppressed_at,
    EntityKind::Trigger,
    request.intent_digest.as_bytes(),
  );
  let trigger_version = number(request.trigger.trigger.version.get(), StoreOperation::AcceptTrigger)?;
  let configuration_version = number(
    request.trigger.target.configuration_version.get(),
    StoreOperation::AcceptTrigger,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_suppressed(outcome),
  };
  if let Some(existing) = occurrence_replay(&mut transaction, &request.trigger, trigger_version).await? {
    if existing.digest == identity.request_digest && existing.state == "suppressed" {
      let outcome = replay_suppressed(existing.outcome)?;
      if outcome.trigger_occurrence_id != existing.occurrence_id {
        return Err(StoreError::Unavailable);
      }
      crate::mutation::commit(
        transaction,
        &identity,
        suppressed_facts(&request, outcome.trigger_occurrence_id),
        encode_outcome(&StoredSuppressedOutcome::from(outcome))?,
      )
      .await?;
      return Ok(outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    });
  }
  require_trigger_reference(&mut transaction, &request, trigger_version, configuration_version).await?;
  insert_suppressed_occurrence(
    &mut transaction,
    &request,
    trigger_version,
    configuration_version,
    &identity.request_digest,
  )
  .await?;
  let outcome = SuppressTriggerOutcome {
    disposition: MutationDisposition::Applied,
    trigger_occurrence_id: request.trigger.id,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    suppressed_facts(&request, request.trigger.id),
    encode_outcome(&StoredSuppressedOutcome::from(outcome))?,
  )
  .await?;
  Ok(outcome)
}

async fn require_references(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
  trigger_version: i64,
  configuration_version: i64,
  pipeline_version: i64,
  repository_version: i64,
) -> Result<(), StoreError> {
  let references_exist: bool = sqlx::query_scalar(
    "SELECT EXISTS (\
       SELECT 1 \
       FROM triggers AS trigger \
       JOIN build_configuration_versions AS version \
         ON version.build_configuration_id = trigger.build_configuration_id \
        AND version.version = trigger.build_configuration_version \
       JOIN build_configurations AS configuration ON configuration.id = version.build_configuration_id \
       WHERE trigger.id = $1 AND trigger.version = $2 \
         AND configuration.id = $3 AND version.version = $4 \
         AND configuration.project_id = $5 \
         AND version.pipeline_id = $6 AND version.pipeline_version = $7 \
         AND version.repository_id = $8 AND version.repository_version = $9 \
         AND trigger.kind = $10 AND trigger.enabled AND version.enabled\
     )",
  )
  .bind(request.trigger.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(request.build.configuration_id.as_uuid())
  .bind(configuration_version)
  .bind(request.build.project_id.as_uuid())
  .bind(request.build.pipeline_id.as_uuid())
  .bind(pipeline_version)
  .bind(request.build.repository_id.as_uuid())
  .bind(repository_version)
  .bind(request.trigger.cause.kind().as_str())
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if !references_exist {
    return Err(StoreError::NotFound {
      entity: EntityKind::Trigger,
    });
  }

  let allowed_pools: BTreeSet<_> = request
    .jobs
    .iter()
    .flat_map(|job| job.allowed_pools.iter().map(|pool| pool.as_uuid()))
    .collect();
  let pool_ids: Vec<_> = allowed_pools.iter().copied().collect();
  for pool_id in &allowed_pools {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
      .bind(format!("octacity.agent-pool.{pool_id}"))
      .execute(&mut **transaction)
      .await
      .map_err(unavailable)?;
  }
  let pool_count: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT id) FROM pools WHERE id = ANY($1)")
    .bind(&pool_ids)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if usize::try_from(pool_count).ok() != Some(allowed_pools.len()) {
    return Err(StoreError::NotFound {
      entity: EntityKind::Pool,
    });
  }
  Ok(())
}

async fn insert_occurrence(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
  trigger_version: i64,
  configuration_version: i64,
  request_digest: &[u8; 32],
) -> Result<(), StoreError> {
  let inserted = sqlx::query(
    "INSERT INTO trigger_occurrences \
       (id, trigger_id, trigger_version, build_configuration_id, build_configuration_version, kind, \
        deduplication_identity, cause, causality, provider_metadata, source_time, state, build_id, request_digest, \
        created_at, updated_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, to_timestamp($11::double precision / 1000.0), \
             'accepted', NULL, $12, to_timestamp($13::double precision / 1000.0), \
             to_timestamp($13::double precision / 1000.0)) \
     ON CONFLICT DO NOTHING",
  )
  .bind(request.trigger.id.as_uuid())
  .bind(request.trigger.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(request.trigger.target.configuration_id.as_uuid())
  .bind(configuration_version)
  .bind(request.trigger.cause.kind().as_str())
  .bind(request.trigger.deduplication_identity.as_str())
  .bind(Json(request.trigger.cause.clone()))
  .bind(Json(request.trigger.causality))
  .bind(Json(request.trigger.provider_metadata.clone()))
  .bind(request.trigger.source_time.unix_millis())
  .bind(request_digest.as_slice())
  .bind(request.accepted_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;
  if inserted.rows_affected() == 1 {
    return Ok(());
  }
  Err(StoreError::Conflict {
    entity: EntityKind::Trigger,
  })
}

async fn require_trigger_reference(
  transaction: &mut Transaction<'_, Postgres>,
  request: &SuppressTrigger,
  trigger_version: i64,
  configuration_version: i64,
) -> Result<(), StoreError> {
  let exists: bool = sqlx::query_scalar(
    "SELECT EXISTS (\
       SELECT 1 FROM triggers \
       WHERE id = $1 AND version = $2 AND enabled AND kind = $3 \
         AND build_configuration_id = $4 AND build_configuration_version = $5\
     )",
  )
  .bind(request.trigger.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(request.trigger.cause.kind().as_str())
  .bind(request.trigger.target.configuration_id.as_uuid())
  .bind(configuration_version)
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if exists {
    Ok(())
  } else {
    Err(StoreError::NotFound {
      entity: EntityKind::Trigger,
    })
  }
}

async fn insert_suppressed_occurrence(
  transaction: &mut Transaction<'_, Postgres>,
  request: &SuppressTrigger,
  trigger_version: i64,
  configuration_version: i64,
  request_digest: &[u8; 32],
) -> Result<(), StoreError> {
  let inserted = sqlx::query(
    "INSERT INTO trigger_occurrences \
       (id, trigger_id, trigger_version, build_configuration_id, build_configuration_version, kind, \
        deduplication_identity, cause, causality, provider_metadata, source_time, state, build_id, request_digest, \
        created_at, updated_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, to_timestamp($11::double precision / 1000.0), \
             'suppressed', NULL, $12, to_timestamp($13::double precision / 1000.0), \
             to_timestamp($13::double precision / 1000.0)) \
     ON CONFLICT DO NOTHING",
  )
  .bind(request.trigger.id.as_uuid())
  .bind(request.trigger.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(request.trigger.target.configuration_id.as_uuid())
  .bind(configuration_version)
  .bind(request.trigger.cause.kind().as_str())
  .bind(request.trigger.deduplication_identity.as_str())
  .bind(Json(request.trigger.cause.clone()))
  .bind(Json(request.trigger.causality))
  .bind(Json(request.trigger.provider_metadata.clone()))
  .bind(request.trigger.source_time.unix_millis())
  .bind(request_digest.as_slice())
  .bind(request.suppressed_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;
  if inserted.rows_affected() == 1 {
    Ok(())
  } else {
    Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    })
  }
}

async fn insert_build(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
  configuration_version: i64,
  pipeline_version: i64,
  repository_version: i64,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO builds \
       (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
        repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
        effective_policy_snapshot, project_job_concurrency_limit, priority, state, version, created_at, updated_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 'running', 1, \
             to_timestamp($15::double precision / 1000.0), to_timestamp($15::double precision / 1000.0))",
  )
  .bind(request.build.id.as_uuid())
  .bind(request.build.project_id.as_uuid())
  .bind(request.build.configuration_id.as_uuid())
  .bind(configuration_version)
  .bind(request.build.pipeline_id.as_uuid())
  .bind(pipeline_version)
  .bind(request.build.repository_id.as_uuid())
  .bind(repository_version)
  .bind(request.trigger.id.as_uuid())
  .bind(request.build.immutable_revision.as_str())
  .bind(Json(request.build.input_snapshot.clone()))
  .bind(Json(request.build.effective_policy_snapshot.clone()))
  .bind(i64::from(request.build.project_job_concurrency_limit))
  .bind(request.build.priority)
  .bind(request.accepted_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Build))?;

  sqlx::query(
    "UPDATE trigger_occurrences SET build_id = $1, \
       updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
  )
  .bind(request.build.id.as_uuid())
  .bind(request.accepted_at.unix_millis())
  .bind(request.trigger.id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;
  Ok(())
}

struct ExistingEvaluation {
  occurrence_id: TriggerOccurrenceId,
  state: String,
  digest: Vec<u8>,
  outcome: Value,
}

async fn occurrence_replay(
  transaction: &mut Transaction<'_, Postgres>,
  trigger: &octacity_server_store::NormalizedTriggerOccurrence,
  trigger_version: i64,
) -> Result<Option<ExistingEvaluation>, StoreError> {
  let record: Option<(Uuid, String, Vec<u8>, Json<Value>)> = sqlx::query_as(
    "SELECT occurrence.id, occurrence.state, record.request_digest, record.outcome \
     FROM trigger_occurrences AS occurrence \
     JOIN idempotency_records AS record \
       ON record.scope = CASE occurrence.state \
            WHEN 'accepted' THEN $1 WHEN 'suppressed' THEN $2 ELSE '' END \
      AND record.idempotency_key = occurrence.id::text \
     WHERE occurrence.id = $3 \
        OR (occurrence.trigger_id = $4 AND occurrence.trigger_version = $5 \
            AND occurrence.deduplication_identity = $6) \
     ORDER BY CASE WHEN occurrence.id = $3 THEN 0 ELSE 1 END \
     LIMIT 1 \
     FOR UPDATE OF occurrence, record",
  )
  .bind(MutationKind::AcceptTrigger.scope())
  .bind(MutationKind::SuppressTrigger.scope())
  .bind(trigger.id.as_uuid())
  .bind(trigger.trigger.id.as_uuid())
  .bind(trigger_version)
  .bind(trigger.deduplication_identity.as_str())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?;
  record
    .map(|(occurrence_id, state, digest, Json(outcome))| {
      Ok(ExistingEvaluation {
        occurrence_id: TriggerOccurrenceId::from_uuid(occurrence_id).map_err(|_| StoreError::Unavailable)?,
        state,
        digest,
        outcome,
      })
    })
    .transpose()
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  trigger_occurrence_id: TriggerOccurrenceId,
  build_id: BuildId,
  attempt_id: AttemptId,
  ready_jobs: Vec<JobId>,
}

impl From<&AcceptTriggerOutcome> for StoredOutcome {
  fn from(outcome: &AcceptTriggerOutcome) -> Self {
    Self {
      trigger_occurrence_id: outcome.trigger_occurrence_id,
      build_id: outcome.build_id,
      attempt_id: outcome.attempt_id,
      ready_jobs: outcome.ready_jobs.clone(),
    }
  }
}

fn replay_accepted(value: Value) -> Result<AcceptTriggerOutcome, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(AcceptTriggerOutcome {
    disposition: MutationDisposition::Replayed,
    trigger_occurrence_id: stored.trigger_occurrence_id,
    build_id: stored.build_id,
    attempt_id: stored.attempt_id,
    ready_jobs: stored.ready_jobs,
  })
}

fn outcome(request: &AcceptTrigger, disposition: MutationDisposition) -> AcceptTriggerOutcome {
  AcceptTriggerOutcome {
    disposition,
    trigger_occurrence_id: request.trigger.id,
    build_id: request.build.id,
    attempt_id: request.attempt_id,
    ready_jobs: request
      .jobs
      .iter()
      .filter(|job| job.dependencies.is_empty())
      .map(|job| job.id)
      .collect(),
  }
}

#[derive(Deserialize, Serialize)]
struct StoredSuppressedOutcome {
  trigger_occurrence_id: TriggerOccurrenceId,
}

impl From<SuppressTriggerOutcome> for StoredSuppressedOutcome {
  fn from(outcome: SuppressTriggerOutcome) -> Self {
    Self {
      trigger_occurrence_id: outcome.trigger_occurrence_id,
    }
  }
}

fn replay_suppressed(value: Value) -> Result<SuppressTriggerOutcome, StoreError> {
  let stored: StoredSuppressedOutcome = decode_outcome(value)?;
  Ok(SuppressTriggerOutcome {
    disposition: MutationDisposition::Replayed,
    trigger_occurrence_id: stored.trigger_occurrence_id,
  })
}

fn accepted_facts(request: &AcceptTrigger, outcome: &AcceptTriggerOutcome) -> MutationFacts {
  let (actor_kind, actor_identity) = trigger_actor(&request.trigger);
  MutationFacts {
    actor_kind,
    actor_identity,
    target_identity: outcome.build_id.to_string(),
    safe_metadata: json!({
      "attempt_id": outcome.attempt_id,
      "ready_job_count": outcome.ready_jobs.len(),
      "trigger_occurrence_id": outcome.trigger_occurrence_id,
    }),
    outbox_payload: json!({
      "attempt_id": outcome.attempt_id,
      "build_id": outcome.build_id,
      "schema_version": 1,
      "trigger_occurrence_id": outcome.trigger_occurrence_id,
    }),
  }
}

fn suppressed_facts(request: &SuppressTrigger, occurrence_id: TriggerOccurrenceId) -> MutationFacts {
  let (actor_kind, actor_identity) = trigger_actor(&request.trigger);
  MutationFacts {
    actor_kind,
    actor_identity,
    target_identity: occurrence_id.to_string(),
    safe_metadata: json!({
      "reason": "configuration_disabled",
      "trigger_occurrence_id": occurrence_id,
    }),
    outbox_payload: json!({
      "reason": "configuration_disabled",
      "schema_version": 1,
      "trigger_occurrence_id": occurrence_id,
    }),
  }
}

fn trigger_actor(trigger: &octacity_server_store::NormalizedTriggerOccurrence) -> (&'static str, Option<String>) {
  match &trigger.cause {
    octacity_server_store::TriggerCause::Manual {} => ("unauthenticated_management", None),
    _ => ("trigger", Some(trigger.trigger.id.to_string())),
  }
}

#[cfg(test)]
mod tests {
  use octacity_server_domain::{
    BuildConfigurationId, Timestamp, TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
  };
  use octacity_server_store::{
    NormalizedTriggerOccurrence, TriggerCause, TriggerDefinitionRef, TriggerMetadata, TriggerTarget,
  };

  use super::trigger_actor;

  #[test]
  fn manual_trigger_is_audited_as_the_unauthenticated_management_actor() {
    let trigger = NormalizedTriggerOccurrence::root(
      TriggerOccurrenceId::from_uuid(uuid::Uuid::from_u128(1)).unwrap(),
      TriggerDefinitionRef {
        id: TriggerId::from_uuid(uuid::Uuid::from_u128(2)).unwrap(),
        version: TriggerVersion::INITIAL,
      },
      TriggerTarget {
        configuration_id: BuildConfigurationId::from_uuid(uuid::Uuid::from_u128(3)).unwrap(),
        configuration_version: octacity_server_domain::BuildConfigurationVersion::INITIAL,
      },
      TriggerIdentity::new("manual:audit").unwrap(),
      TriggerCause::Manual {},
      TriggerMetadata::default(),
      Timestamp::from_unix_millis(1).unwrap(),
    )
    .unwrap();

    assert_eq!(trigger_actor(&trigger), ("unauthenticated_management", None));
  }
}
