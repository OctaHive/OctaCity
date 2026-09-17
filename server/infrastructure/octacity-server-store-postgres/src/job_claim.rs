use octacity_server_domain::{EntityKind, JobId};
use octacity_server_store::{JobClaim, JobClaimOutcome, LeaseGrant, StoreError, StoreOperation};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool};
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
    "expires_at": request.expires_at.unix_millis(),
    "fence": request.fence.expose(),
    "lease_id": request.lease_id,
    "pool_id": request.pool_id,
    "registration_epoch": request.registration_epoch.get(),
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

  let registration: Option<RegistrationRow> = sqlx::query_as(
    "SELECT registration.id AS registration_id, agent.pool_version \
     FROM agent_registrations AS registration \
     JOIN agents AS agent ON agent.id = registration.agent_id \
     JOIN pools AS pool ON pool.id = agent.pool_id AND pool.version = agent.pool_version \
     WHERE agent.id = $1 AND registration.epoch = $2 AND agent.pool_id = $3 \
       AND registration.revoked_at IS NULL \
       AND registration.expires_at > to_timestamp($4::double precision / 1000.0) \
       AND pool.enabled AND pool.drain_state = 'accepting' \
     ORDER BY registration.registered_at DESC \
     LIMIT 1 \
     FOR UPDATE OF registration, agent, pool",
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

  let job_id: Option<Uuid> = sqlx::query_scalar(
    "SELECT job_id \
     FROM ready_queue_entries \
     WHERE $1 = ANY(allowed_pool_ids) \
     ORDER BY priority DESC, enqueue_order, job_id \
     LIMIT 1 \
     FOR UPDATE SKIP LOCKED",
  )
  .bind(request.pool_id.as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let Some(job_id) = job_id else {
    transaction.rollback().await.map_err(unavailable)?;
    return Ok(JobClaimOutcome::Empty);
  };

  sqlx::query("DELETE FROM ready_queue_entries WHERE job_id = $1")
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
  let fence_hash = fence_hash(request.fence);
  sqlx::query(
    "INSERT INTO leases \
       (id, job_id, pool_id, pool_version, registration_id, fence_hash, state, version, leased_at, expires_at) \
     VALUES ($1, $2, $3, $4, $5, $6, 'active', 1, to_timestamp($7::double precision / 1000.0), \
             to_timestamp($8::double precision / 1000.0))",
  )
  .bind(request.lease_id.as_uuid())
  .bind(job_id)
  .bind(request.pool_id.as_uuid())
  .bind(registration.pool_version)
  .bind(registration.registration_id)
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
  let grant = grant(request, job_id);
  crate::mutation::commit(
    transaction,
    &identity,
    facts(&request, grant),
    encode_outcome(&StoredOutcome { job_id })?,
  )
  .await?;
  Ok(JobClaimOutcome::Claimed(grant))
}

#[derive(FromRow)]
struct RegistrationRow {
  registration_id: Uuid,
  pool_version: i64,
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  job_id: JobId,
}

fn replay(request: JobClaim, value: Value) -> Result<JobClaimOutcome, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(JobClaimOutcome::Claimed(grant(request, stored.job_id)))
}

fn grant(request: JobClaim, job_id: JobId) -> LeaseGrant {
  LeaseGrant {
    lease_id: request.lease_id,
    fence: request.fence,
    job_id,
    agent_id: request.agent_id,
    registration_epoch: request.registration_epoch,
    pool_id: request.pool_id,
    expires_at: request.expires_at,
  }
}

fn facts(request: &JobClaim, grant: LeaseGrant) -> MutationFacts {
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
