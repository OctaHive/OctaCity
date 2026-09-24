use octacity_server_cache::{CacheNamespacePolicy, CacheSessionEvent, CacheSessionState};
use octacity_server_domain::{EntityKind, ProjectId, Timestamp};
use octacity_server_store::{
  AuthorizeCacheSession, BeginCacheSession, BeginCacheSessionOutcome, CacheAuthorization, CacheAuthorizationOutcome,
  MutationDisposition, RevokeCacheSession, StoreError, StoreOperation, cache_scope_id,
};
use sqlx::{PgPool, types::Json};
use uuid::Uuid;

use super::records::{
  AuthorizationRow, CacheSessionRow, add_seconds, current_lease_state, digest, id, invalid_cache, load_by_request,
  load_policy, number, parse_state, positive_attempt, retention_seconds, timestamp, unsigned,
};
use crate::{database::unavailable, lease};

pub(crate) async fn begin(pool: &PgPool, request: BeginCacheSession) -> Result<BeginCacheSessionOutcome, StoreError> {
  request.validate()?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let lease_row = lease::load(&mut transaction, request.lease, StoreOperation::BeginCacheSession).await?;
  lease::require_current(&lease_row, request.lease, request.created_at)?;
  if lease_row.job_id != request.job_id.as_uuid() || positive_attempt(lease_row.attempt_number)? != request.attempt {
    return Err(StoreError::Fenced {
      lease: request.lease.lease_id,
    });
  }

  let policy = load_policy(&mut transaction, request.job_id, lease_row.build_id, &request).await?;
  if let Some(existing) = load_by_request(
    &mut transaction,
    request.lease.lease_id,
    request.idempotency_key.as_str(),
  )
  .await?
  {
    if existing.namespace != policy.namespace.as_str()
      || existing.permissions.0 != policy.permissions
      || existing.job_id != request.job_id.as_uuid()
    {
      return Err(StoreError::Conflict {
        entity: EntityKind::CacheSession,
      });
    }
    transaction.commit().await.map_err(unavailable)?;
    return Ok(BeginCacheSessionOutcome {
      session: existing.try_into()?,
      disposition: MutationDisposition::Replayed,
    });
  }

  let retention_until = add_seconds(request.created_at, policy.retention_seconds)?;
  let expires_at = Timestamp::from_unix_millis(request.expires_at.unix_millis().min(lease_row.expires_at_millis))
    .map_err(|_| invalid_cache())?;
  let scope_id = cache_scope_id(id::<ProjectId>(lease_row.project_id)?, &policy.namespace);
  let permissions = Json(policy.permissions);
  let quota_bytes = number(policy.quota_bytes)?;
  let epoch = number(request.lease.registration_epoch.get())?;
  let row: CacheSessionRow = sqlx::query_as(
    "INSERT INTO cache_sessions (
       id, project_id, build_id, job_id, registration_id, lease_id, namespace, permissions,
       credential_hash, quota_bytes, state, created_at, expires_at, revoked_at,
       request_identity, scope_id, agent_id, registration_epoch, retention_until
     ) VALUES (
       $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'active',
       to_timestamp($11::double precision / 1000.0), to_timestamp($12::double precision / 1000.0), NULL,
       $13, $14, $15, $16, to_timestamp($17::double precision / 1000.0)
     )
     RETURNING id, project_id, build_id, job_id, lease_id, namespace, permissions, quota_bytes, state,
       scope_id, agent_id, registration_epoch,
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis,
       FLOOR(EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT AS expires_at_millis,
       FLOOR(EXTRACT(EPOCH FROM retention_until) * 1000)::BIGINT AS retention_until_millis,
       NULL::BIGINT AS revoked_at_millis,
       TRUE AS lease_current",
  )
  .bind(request.session_id.as_uuid())
  .bind(lease_row.project_id)
  .bind(lease_row.build_id)
  .bind(request.job_id.as_uuid())
  .bind(lease_row.registration_id)
  .bind(request.lease.lease_id.as_uuid())
  .bind(policy.namespace.as_str())
  .bind(permissions)
  .bind(request.credential_digest.as_bytes().to_vec())
  .bind(quota_bytes)
  .bind(request.created_at.unix_millis())
  .bind(expires_at.unix_millis())
  .bind(request.idempotency_key.as_str())
  .bind(scope_id)
  .bind(lease_row.agent_id)
  .bind(epoch)
  .bind(retention_until.unix_millis())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(BeginCacheSessionOutcome {
    session: row.try_into()?,
    disposition: MutationDisposition::Applied,
  })
}

pub(crate) async fn revoke(pool: &PgPool, request: RevokeCacheSession) -> Result<MutationDisposition, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let lease_row = lease::load(&mut transaction, request.lease, StoreOperation::RevokeCacheSession).await?;
  lease::require_current(&lease_row, request.lease, request.revoked_at)?;
  let row: (Uuid, String, Option<i64>) = sqlx::query_as(
    "SELECT job_id, state, FLOOR(EXTRACT(EPOCH FROM revoked_at) * 1000)::BIGINT
     FROM cache_sessions WHERE id = $1 AND lease_id = $2 FOR UPDATE",
  )
  .bind(request.session_id.as_uuid())
  .bind(request.lease.lease_id.as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::CacheSession,
  })?;
  if row.0 != request.job_id.as_uuid() {
    return Err(StoreError::Fenced {
      lease: request.lease.lease_id,
    });
  }
  let state = parse_state(&row.1)?;
  if state == CacheSessionState::Revoked {
    transaction.commit().await.map_err(unavailable)?;
    return Ok(MutationDisposition::Replayed);
  }
  state
    .transition(CacheSessionEvent::Revoke)
    .map_err(|_| StoreError::Conflict {
      entity: EntityKind::CacheSession,
    })?;
  sqlx::query(
    "UPDATE cache_sessions SET state = 'revoked', revoked_at = to_timestamp($2::double precision / 1000.0)
     WHERE id = $1",
  )
  .bind(request.session_id.as_uuid())
  .bind(request.revoked_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(MutationDisposition::Applied)
}

pub(crate) async fn authorize(
  pool: &PgPool,
  request: AuthorizeCacheSession,
) -> Result<CacheAuthorizationOutcome, StoreError> {
  let row: Option<AuthorizationRow> = sqlx::query_as(
    "SELECT session.project_id, session.namespace, session.permissions, session.credential_hash,
            session.quota_bytes, session.state,
            FLOOR(EXTRACT(EPOCH FROM session.created_at) * 1000)::BIGINT AS created_at_millis,
            FLOOR(EXTRACT(EPOCH FROM session.expires_at) * 1000)::BIGINT AS expires_at_millis,
            FLOOR(EXTRACT(EPOCH FROM session.retention_until) * 1000)::BIGINT AS retention_until_millis,
            lease.state AS lease_state,
            FLOOR(EXTRACT(EPOCH FROM lease.expires_at) * 1000)::BIGINT AS lease_expires_at_millis,
            registration.revoked_at IS NOT NULL AS registration_revoked,
            FLOOR(EXTRACT(EPOCH FROM registration.expires_at) * 1000)::BIGINT AS registration_expires_at_millis
     FROM cache_sessions AS session
     JOIN leases AS lease ON lease.id = session.lease_id AND lease.registration_id = session.registration_id
     JOIN agent_registrations AS registration ON registration.id = session.registration_id
     WHERE session.id = $1",
  )
  .bind(request.session_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  let Some(row) = row else {
    return Ok(CacheAuthorizationOutcome::Rejected);
  };
  let digest = digest(&row.credential_hash)?;
  let permissions = row.permissions.0;
  let now = request.observed_at.unix_millis();
  let allowed = row.state == "active"
    && row.namespace == request.namespace.as_str()
    && permissions.allows(request.operation)
    && digest.matches(request.credential_digest)
    && now < row.expires_at_millis
    && current_lease_state(&row.lease_state)
    && now < row.lease_expires_at_millis
    && !row.registration_revoked
    && now < row.registration_expires_at_millis;
  if !allowed {
    return Ok(CacheAuthorizationOutcome::Rejected);
  }
  Ok(CacheAuthorizationOutcome::Authorized(CacheAuthorization {
    project_id: id(row.project_id)?,
    policy: CacheNamespacePolicy {
      namespace: request.namespace,
      permissions,
      quota_bytes: unsigned(row.quota_bytes)?,
      retention_seconds: retention_seconds(timestamp(row.created_at_millis)?, row.retention_until_millis)?,
    },
    retention_until: timestamp(row.retention_until_millis)?,
  }))
}
