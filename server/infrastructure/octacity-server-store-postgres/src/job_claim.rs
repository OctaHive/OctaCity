use octacity_protocol::{AgentInventory, HostSnapshot, SignedEnvelope};
use octacity_server_domain::{AttemptNumber, EntityKind, JobId};
use octacity_server_job::{JobRequirements, JobSpecTemplate};
use octacity_server_store::{JobClaim, JobClaimOutcome, LeaseGrant, StoreError, StoreInputError, StoreOperation};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

use crate::{
  database::{classify, number, unavailable},
  lease::fence_hash,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

pub(crate) async fn execute(pool: &PgPool, request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
  request.validate()?;
  let epoch = number(request.registration_epoch.get(), StoreOperation::ClaimReadyJob)?;
  let digest_input = json!({
    "agent_id": request.agent_id,
    "fence": request.fence.expose(),
    "lease_id": request.lease_id,
    "pool_id": request.pool_id,
    "registration_epoch": request.registration_epoch.get(),
    "snapshot": request.snapshot,
  });
  let identity = MutationIdentity::new(
    MutationKind::ClaimReadyJob,
    request.lease_id.to_string(),
    request.claimed_at,
    EntityKind::Lease,
    &digest_input,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(request, outcome),
  };
  sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
    .bind(format!("octacity.agent-pool.{}", request.pool_id))
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;

  let registration: Option<RegistrationRow> = sqlx::query_as(
    "SELECT registration.id AS registration_id, pool.version AS pool_version, pool.fairness_policy, registration.inventory \
     FROM agent_registrations AS registration \
     JOIN agents AS agent ON agent.id = registration.agent_id \
     JOIN LATERAL (SELECT version, enabled, drain_state, concurrency_limit, fairness_policy FROM pools \
       WHERE id = agent.pool_id ORDER BY version DESC LIMIT 1) AS pool ON true \
     WHERE agent.id = $1 AND registration.epoch = $2 AND agent.pool_id = $3 AND agent.state = 'online' \
       AND registration.revoked_at IS NULL \
       AND registration.expires_at > to_timestamp($4::double precision / 1000.0) \
       AND pool.enabled AND pool.drain_state = 'accepting' \
       AND (SELECT COUNT(*) FROM leases WHERE pool_id = agent.pool_id \
         AND state IN ('active', 'cancellation_requested', 'drain_requested')) < pool.concurrency_limit \
       AND NOT EXISTS (SELECT 1 FROM leases AS owned \
         JOIN agent_registrations AS owner_registration ON owner_registration.id = owned.registration_id \
         WHERE owner_registration.agent_id = agent.id \
           AND owned.state IN ('active', 'cancellation_requested', 'drain_requested')) \
     ORDER BY registration.registered_at DESC \
     LIMIT 1 \
     FOR UPDATE OF registration, agent",
  )
  .bind(request.agent_id.as_uuid())
  .bind(epoch)
  .bind(request.pool_id.as_uuid())
  .bind(request.claimed_at.unix_millis())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let Some(registration) = registration else {
    transaction.rollback().await.map_err(unavailable)?;
    return Ok(JobClaimOutcome::Empty);
  };
  request
    .snapshot
    .validate(&registration.inventory.0.host_capacity)
    .map_err(|_| StoreError::InvalidInput {
      operation: StoreOperation::ClaimReadyJob,
      source: StoreInputError::InvalidAgentInventory,
    })?;
  if request.snapshot.active_job.is_some() {
    return Err(StoreError::InvalidInput {
      operation: StoreOperation::ClaimReadyJob,
      source: StoreInputError::InvalidAgentInventory,
    });
  }

  let candidate = select_candidate(
    &mut transaction,
    request.pool_id.as_uuid(),
    &registration.fairness_policy,
    &registration.inventory.0,
    &request.snapshot,
  )
  .await?;
  let Some(candidate) = candidate else {
    transaction.rollback().await.map_err(unavailable)?;
    return Ok(JobClaimOutcome::Empty);
  };
  let job_id = candidate.job_id;

  sqlx::query("DELETE FROM ready_queue_entries WHERE job_id = $1")
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
  let fence_hash = fence_hash(request.fence);
  sqlx::query(
    "INSERT INTO leases \
       (id, job_id, pool_id, pool_version, registration_id, registration_epoch, fence_hash, state, version, \
        leased_at, expires_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', 1, to_timestamp($8::double precision / 1000.0), \
             to_timestamp($9::double precision / 1000.0))",
  )
  .bind(request.lease_id.as_uuid())
  .bind(job_id)
  .bind(request.pool_id.as_uuid())
  .bind(registration.pool_version)
  .bind(registration.registration_id)
  .bind(epoch)
  .bind(fence_hash.as_slice())
  .bind(request.claimed_at.unix_millis())
  .bind(request.expires_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Lease))?;
  sqlx::query(
    "UPDATE jobs SET state = 'leased', version = version + 1, \
       updated_at = to_timestamp($1::double precision / 1000.0) WHERE id = $2",
  )
  .bind(request.claimed_at.unix_millis())
  .bind(job_id)
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;

  let job_id = JobId::from_uuid(job_id).map_err(|_| StoreError::Unavailable)?;
  let attempt = AttemptNumber::new(u64::try_from(candidate.attempt_number).map_err(|_| StoreError::Unavailable)?)
    .map_err(|_| StoreError::Unavailable)?;
  let grant = grant(&request, job_id, attempt, candidate.signed_job_spec.0);
  crate::mutation::commit(
    transaction,
    &identity,
    facts(&request, &grant),
    encode_outcome(&StoredOutcome {
      job_id,
      attempt,
      signed_job_spec: grant.signed_job_spec.clone(),
      claimed_at: grant.claimed_at,
      expires_at: grant.expires_at,
    })?,
  )
  .await?;
  Ok(JobClaimOutcome::Claimed(Box::new(grant)))
}

#[derive(FromRow)]
struct RegistrationRow {
  registration_id: Uuid,
  pool_version: i64,
  fairness_policy: String,
  inventory: Json<AgentInventory>,
}

#[derive(FromRow)]
struct CandidateRow {
  job_id: Uuid,
  project_id: Uuid,
  build_configuration_id: Uuid,
  build_configuration_version: i64,
  project_concurrency_limit: i64,
  configuration_concurrency_limit: i64,
  priority: i64,
  fairness_rank: i64,
  enqueue_order: i64,
  attempt_number: i64,
  requirements: Json<JobRequirements>,
  job_spec_template: Json<JobSpecTemplate>,
  signed_job_spec: Json<SignedEnvelope>,
}

async fn select_candidate(
  transaction: &mut Transaction<'_, Postgres>,
  pool_id: Uuid,
  fairness_policy: &str,
  inventory: &AgentInventory,
  snapshot: &HostSnapshot,
) -> Result<Option<CandidateRow>, StoreError> {
  const PAGE_SIZE: i64 = 64;
  let mut after_priority = None;
  let mut after_fairness_rank = None;
  let mut after_enqueue_order = None;
  let mut after_job_id = None;
  loop {
    let candidates: Vec<CandidateRow> = sqlx::query_as(
      "SELECT queue.job_id, queue.project_id, queue.build_configuration_id, queue.build_configuration_version, \
              build.project_job_concurrency_limit AS project_concurrency_limit, \
              configuration.job_concurrency_limit AS configuration_concurrency_limit, queue.priority, \
              CASE WHEN $6 = 'configuration_fair' THEN configuration_load.active_jobs ELSE 0 END AS fairness_rank, \
              queue.enqueue_order, attempt.attempt_number, \
              job.requirements, job.job_spec_template, job.signed_job_spec \
       FROM ready_queue_entries AS queue \
       JOIN jobs AS job ON job.id = queue.job_id \
       JOIN attempts AS attempt ON attempt.id = job.attempt_id \
       JOIN builds AS build ON build.id = attempt.build_id \
       JOIN build_configuration_versions AS configuration \
         ON configuration.build_configuration_id = queue.build_configuration_id \
        AND configuration.version = queue.build_configuration_version \
       JOIN LATERAL (SELECT COUNT(*)::bigint AS active_jobs FROM leases AS active_lease \
         JOIN jobs AS active_job ON active_job.id = active_lease.job_id \
         JOIN attempts AS active_attempt ON active_attempt.id = active_job.attempt_id \
         JOIN builds AS active_build ON active_build.id = active_attempt.build_id \
         WHERE active_lease.state IN ('active', 'cancellation_requested', 'drain_requested') \
           AND active_build.project_id = queue.project_id) AS project_load ON true \
       JOIN LATERAL (SELECT COUNT(*)::bigint AS active_jobs FROM leases AS active_lease \
         JOIN jobs AS active_job ON active_job.id = active_lease.job_id \
         JOIN attempts AS active_attempt ON active_attempt.id = active_job.attempt_id \
         JOIN builds AS active_build ON active_build.id = active_attempt.build_id \
         WHERE active_lease.state IN ('active', 'cancellation_requested', 'drain_requested') \
           AND active_build.build_configuration_id = queue.build_configuration_id \
           AND active_build.build_configuration_version = queue.build_configuration_version) AS configuration_load ON true \
       WHERE $1 = ANY(queue.allowed_pool_ids) AND job.state = 'ready' AND job.signed_job_spec IS NOT NULL \
         AND project_load.active_jobs < build.project_job_concurrency_limit \
         AND configuration_load.active_jobs < configuration.job_concurrency_limit \
         AND ($2::bigint IS NULL OR queue.priority < $2 \
           OR (queue.priority = $2 AND CASE WHEN $6 = 'configuration_fair' THEN configuration_load.active_jobs ELSE 0 END > $3) \
           OR (queue.priority = $2 AND CASE WHEN $6 = 'configuration_fair' THEN configuration_load.active_jobs ELSE 0 END = $3 AND queue.enqueue_order > $4) \
           OR (queue.priority = $2 AND CASE WHEN $6 = 'configuration_fair' THEN configuration_load.active_jobs ELSE 0 END = $3 AND queue.enqueue_order = $4 AND queue.job_id > $5)) \
       ORDER BY queue.priority DESC, fairness_rank, queue.enqueue_order, queue.job_id \
       LIMIT $7",
    )
    .bind(pool_id)
    .bind(after_priority)
    .bind(after_fairness_rank)
    .bind(after_enqueue_order)
    .bind(after_job_id)
    .bind(fairness_policy)
    .bind(PAGE_SIZE)
    .fetch_all(&mut **transaction)
    .await
    .map_err(unavailable)?;
    let page_len = candidates.len();
    let Some(last) = candidates.last() else {
      return Ok(None);
    };
    let next_cursor = (last.priority, last.fairness_rank, last.enqueue_order, last.job_id);
    for candidate in candidates {
      if !octacity_server_scheduler::is_compatible(
        inventory,
        snapshot,
        &candidate.requirements.0,
        &candidate.job_spec_template.0,
      ) {
        continue;
      }
      let locked: Option<Uuid> =
        sqlx::query_scalar("SELECT job_id FROM ready_queue_entries WHERE job_id = $1 FOR UPDATE SKIP LOCKED")
          .bind(candidate.job_id)
          .fetch_optional(&mut **transaction)
          .await
          .map_err(unavailable)?;
      if locked.is_some()
        && try_lock_concurrency_scopes(transaction, candidate.project_id, candidate.build_configuration_id).await?
        && within_concurrency_limits(transaction, &candidate).await?
      {
        return Ok(Some(candidate));
      }
    }
    if page_len < usize::try_from(PAGE_SIZE).expect("positive page size fits usize") {
      return Ok(None);
    }
    after_priority = Some(next_cursor.0);
    after_fairness_rank = Some(next_cursor.1);
    after_enqueue_order = Some(next_cursor.2);
    after_job_id = Some(next_cursor.3);
  }
}

/// Acquires only the concurrency scopes that can conflict with this candidate.
///
/// Every claim takes Project before Build Configuration. Try-locking preserves
/// skip-locked placement: a busy scope cannot block an unrelated ready Job.
async fn try_lock_concurrency_scopes(
  transaction: &mut Transaction<'_, Postgres>,
  project_id: Uuid,
  configuration_id: Uuid,
) -> Result<bool, StoreError> {
  for identity in [
    format!("octacity.project-concurrency.{project_id}"),
    format!("octacity.configuration-concurrency.{configuration_id}"),
  ] {
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1, 0))")
      .bind(identity)
      .fetch_one(&mut **transaction)
      .await
      .map_err(unavailable)?;
    if !acquired {
      return Ok(false);
    }
  }
  Ok(true)
}

/// Rechecks mutable lease counts after the candidate's scopes are serialized.
async fn within_concurrency_limits(
  transaction: &mut Transaction<'_, Postgres>,
  candidate: &CandidateRow,
) -> Result<bool, StoreError> {
  let project_limit = u32::try_from(candidate.project_concurrency_limit).map_err(|_| StoreError::Unavailable)?;
  let configuration_limit =
    u32::try_from(candidate.configuration_concurrency_limit).map_err(|_| StoreError::Unavailable)?;
  if project_limit == 0 || configuration_limit == 0 {
    return Err(StoreError::Unavailable);
  }
  let (project_jobs, configuration_jobs): (i64, i64) = sqlx::query_as(
    "SELECT \
       (SELECT COUNT(*) FROM leases AS active_lease \
        JOIN jobs AS active_job ON active_job.id = active_lease.job_id \
        JOIN attempts AS active_attempt ON active_attempt.id = active_job.attempt_id \
        JOIN builds AS active_build ON active_build.id = active_attempt.build_id \
        WHERE active_lease.state IN ('active', 'cancellation_requested', 'drain_requested') \
          AND active_build.project_id = $1), \
       (SELECT COUNT(*) FROM leases AS active_lease \
        JOIN jobs AS active_job ON active_job.id = active_lease.job_id \
        JOIN attempts AS active_attempt ON active_attempt.id = active_job.attempt_id \
        JOIN builds AS active_build ON active_build.id = active_attempt.build_id \
        WHERE active_lease.state IN ('active', 'cancellation_requested', 'drain_requested') \
          AND active_build.build_configuration_id = $2 \
          AND active_build.build_configuration_version = $3)",
  )
  .bind(candidate.project_id)
  .bind(candidate.build_configuration_id)
  .bind(candidate.build_configuration_version)
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(project_jobs < i64::from(project_limit) && configuration_jobs < i64::from(configuration_limit))
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  job_id: JobId,
  attempt: AttemptNumber,
  signed_job_spec: SignedEnvelope,
  claimed_at: octacity_server_domain::Timestamp,
  expires_at: octacity_server_domain::Timestamp,
}

fn replay(request: JobClaim, value: Value) -> Result<JobClaimOutcome, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  let mut grant = grant(&request, stored.job_id, stored.attempt, stored.signed_job_spec);
  grant.claimed_at = stored.claimed_at;
  grant.expires_at = stored.expires_at;
  Ok(JobClaimOutcome::Claimed(Box::new(grant)))
}

fn grant(request: &JobClaim, job_id: JobId, attempt: AttemptNumber, signed_job_spec: SignedEnvelope) -> LeaseGrant {
  LeaseGrant {
    lease_id: request.lease_id,
    fence: request.fence,
    job_id,
    attempt,
    agent_id: request.agent_id,
    registration_epoch: request.registration_epoch,
    pool_id: request.pool_id,
    claimed_at: request.claimed_at,
    expires_at: request.expires_at,
    signed_job_spec,
  }
}

fn facts(request: &JobClaim, grant: &LeaseGrant) -> MutationFacts {
  MutationFacts {
    actor_kind: "agent",
    actor_identity: Some(request.agent_id.to_string()),
    target_identity: grant.job_id.to_string(),
    safe_metadata: json!({
      "lease_id": request.lease_id,
      "pool_id": request.pool_id,
      "registration_epoch": request.registration_epoch.get(),
    }),
    outbox_payload: json!({
      "agent_id": request.agent_id,
      "job_id": grant.job_id,
      "lease_id": request.lease_id,
      "pool_id": request.pool_id,
      "schema_version": 1,
    }),
  }
}
