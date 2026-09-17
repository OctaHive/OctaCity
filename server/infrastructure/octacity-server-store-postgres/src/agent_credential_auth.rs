use octacity_server_domain::{AgentId, PoolId, PoolVersion};
use octacity_server_store::{
  AuthenticateAgentRegistration, AuthenticatedAgentRegistration, CredentialDigest, RegistrationEpoch, StoreError,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::database::unavailable;

pub(crate) async fn execute(
  pool: &PgPool,
  request: AuthenticateAgentRegistration,
) -> Result<AuthenticatedAgentRegistration, StoreError> {
  let row: Option<RegistrationRow> = sqlx::query_as(
    "SELECT registration.agent_id, registration.epoch, registration.credential_hash, \
       registration.revoked_at IS NOT NULL AS revoked, \
       (extract(epoch FROM registration.expires_at) * 1000)::bigint AS expires_at, \
       agent.pool_id, agent.pool_version \
     FROM agent_registrations AS registration \
     JOIN agents AS agent ON agent.id = registration.agent_id \
     WHERE registration.id = $1",
  )
  .bind(request.credential_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  let row = row.ok_or(StoreError::CredentialRejected)?;
  let persisted_digest = CredentialDigest::from_bytes(
    row
      .credential_hash
      .as_slice()
      .try_into()
      .map_err(|_| StoreError::Unavailable)?,
  );
  if row.revoked
    || row.expires_at <= request.authenticated_at.unix_millis()
    || !persisted_digest.matches(request.credential.digest())
  {
    return Err(StoreError::CredentialRejected);
  }
  Ok(AuthenticatedAgentRegistration {
    agent_id: AgentId::from_uuid(row.agent_id).map_err(|_| StoreError::Unavailable)?,
    registration_epoch: RegistrationEpoch::new(u64::try_from(row.epoch).map_err(|_| StoreError::Unavailable)?)
      .map_err(|_| StoreError::Unavailable)?,
    pool_id: PoolId::from_uuid(row.pool_id).map_err(|_| StoreError::Unavailable)?,
    pool_version: PoolVersion::new(u64::try_from(row.pool_version).map_err(|_| StoreError::Unavailable)?)
      .map_err(|_| StoreError::Unavailable)?,
    expires_at: octacity_server_domain::Timestamp::from_unix_millis(row.expires_at)
      .map_err(|_| StoreError::Unavailable)?,
  })
}

#[derive(sqlx::FromRow)]
struct RegistrationRow {
  agent_id: Uuid,
  epoch: i64,
  credential_hash: Vec<u8>,
  revoked: bool,
  expires_at: i64,
  pool_id: Uuid,
  pool_version: i64,
}
