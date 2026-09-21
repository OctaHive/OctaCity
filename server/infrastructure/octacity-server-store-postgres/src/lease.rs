use octacity_server_domain::Timestamp;
use octacity_server_store::{LeaseAccess, LeaseFence, StoreError, StoreOperation};
use sha2::{Digest as _, Sha256};
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

use crate::database::{number, unavailable};

const CURRENT_STATES: [&str; 3] = ["active", "cancellation_requested", "drain_requested"];

#[derive(FromRow)]
pub(crate) struct LeaseRow {
  pub(crate) job_id: Uuid,
  pub(crate) attempt_id: Uuid,
  pub(crate) attempt_number: i64,
  agent_id: Uuid,
  epoch: i64,
  fence_hash: Vec<u8>,
  pub(crate) state: String,
  registration_revoked: bool,
  registration_expires_at_millis: i64,
  expired: bool,
  expires_at_millis: i64,
}

pub(crate) async fn load(
  transaction: &mut Transaction<'_, Postgres>,
  access: LeaseAccess,
  operation: StoreOperation,
) -> Result<LeaseRow, StoreError> {
  let epoch = number(access.registration_epoch.get(), operation)?;
  let row: LeaseRow = sqlx::query_as(
    "SELECT lease.job_id, job.attempt_id, attempt.attempt_number, agent.id AS agent_id, lease.registration_epoch AS epoch, \
            lease.fence_hash, lease.state, \
            registration.revoked_at IS NOT NULL AS registration_revoked, \
            FLOOR(EXTRACT(EPOCH FROM registration.expires_at) * 1000)::BIGINT AS registration_expires_at_millis, \
            lease.expires_at <= now() AS expired, \
            FLOOR(EXTRACT(EPOCH FROM lease.expires_at) * 1000)::BIGINT AS expires_at_millis \
     FROM leases AS lease \
     JOIN jobs AS job ON job.id = lease.job_id \
     JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     JOIN agent_registrations AS registration ON registration.id = lease.registration_id \
     JOIN agents AS agent ON agent.id = registration.agent_id \
     WHERE lease.id = $1 \
     FOR UPDATE OF lease, job",
  )
  .bind(access.lease_id.as_uuid())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::Fenced { lease: access.lease_id })?;
  if row.agent_id != access.agent_id.as_uuid() || row.epoch != epoch || row.fence_hash != fence_hash(access.fence) {
    return Err(StoreError::Fenced { lease: access.lease_id });
  }
  Ok(row)
}

pub(crate) fn require_current(row: &LeaseRow, access: LeaseAccess, observed_at: Timestamp) -> Result<(), StoreError> {
  if row.expired || row.state == "expired" || observed_at.unix_millis() >= row.expires_at_millis {
    return Err(StoreError::Expired { lease: access.lease_id });
  }
  if row.registration_revoked || observed_at.unix_millis() >= row.registration_expires_at_millis {
    return Err(StoreError::Fenced { lease: access.lease_id });
  }
  if !CURRENT_STATES.contains(&row.state.as_str()) {
    return Err(StoreError::Fenced { lease: access.lease_id });
  }
  Ok(())
}

pub(crate) fn fence_hash(fence: LeaseFence) -> Vec<u8> {
  Sha256::digest(fence.expose()).to_vec()
}
