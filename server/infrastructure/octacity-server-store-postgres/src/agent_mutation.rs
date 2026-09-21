use octacity_server_domain::{AgentId, EntityKind, PoolId};
use octacity_server_scheduler::PoolDrainState;
use octacity_server_store::{
  AgentDrainMode, AgentPlatform, AgentPoolDefinition, DrainAgent, DrainAgentOutcome, MutationDisposition,
  PoolAdmissionPolicy, PoolFairnessPolicy, ReassignAgentPool, ReassignAgentPoolOutcome, StoreError,
};
use serde::Serialize;
use serde_json::json;
use sqlx::{Postgres, Transaction};

use crate::{
  agent_row::AgentRow,
  database::{number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

#[derive(Serialize)]
struct Fingerprint {
  agent_id: AgentId,
  expected_version: octacity_server_domain::AgentVersion,
  target_pool_id: PoolId,
}

#[derive(Serialize)]
struct DrainFingerprint {
  agent_id: AgentId,
  expected_version: octacity_server_domain::AgentVersion,
  mode: AgentDrainMode,
}

pub(crate) async fn drain(pool: &sqlx::PgPool, request: DrainAgent) -> Result<DrainAgentOutcome, StoreError> {
  let identity = MutationIdentity::new(
    MutationKind::DrainAgent,
    request.idempotency_key.to_string(),
    request.requested_at,
    EntityKind::Agent,
    &DrainFingerprint {
      agent_id: request.agent_id,
      expected_version: request.expected_version,
      mode: request.mode,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => {
      let mut outcome: DrainAgentOutcome = decode_outcome(outcome)?;
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
  };
  let current: Option<(i64, String)> = sqlx::query_as("SELECT version, state FROM agents WHERE id = $1 FOR UPDATE")
    .bind(request.agent_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(unavailable)?;
  let Some((version, status)) = current else {
    return Err(StoreError::NotFound {
      entity: EntityKind::Agent,
    });
  };
  if version
    != number(
      request.expected_version.get(),
      octacity_server_store::StoreOperation::DrainAgent,
    )?
    || status == "draining"
  {
    return Err(StoreError::Conflict {
      entity: EntityKind::Agent,
    });
  }

  let lease_state = match request.mode {
    AgentDrainMode::Graceful => "drain_requested",
    AgentDrainMode::Forced => "cancellation_requested",
  };
  sqlx::query(
    "UPDATE leases AS lease SET state = $1, version = version + 1 \
     FROM agent_registrations AS registration \
     WHERE lease.registration_id = registration.id AND registration.agent_id = $2 \
       AND lease.state IN ('active', 'drain_requested')",
  )
  .bind(lease_state)
  .bind(request.agent_id.as_uuid())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let row = sqlx::query_as::<_, AgentRow>(
    "UPDATE agents SET state = 'draining', version = version + 1, \
       updated_at = to_timestamp($1::double precision / 1000.0) WHERE id = $2 \
     RETURNING id, name, version, pool_id, pool_version, inventory, state, \
       FLOOR(EXTRACT(EPOCH FROM last_seen_at) * 1000)::BIGINT AS last_seen_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis",
  )
  .bind(request.requested_at.unix_millis())
  .bind(request.agent_id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let outcome = DrainAgentOutcome {
    disposition: MutationDisposition::Applied,
    agent: row.try_into()?,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "unauthenticated_management",
      actor_identity: None,
      target_identity: request.agent_id.to_string(),
      safe_metadata: json!({"mode": request.mode}),
      outbox_payload: json!({"agent_id": request.agent_id, "mode": request.mode}),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn reassign(
  pool: &sqlx::PgPool,
  request: ReassignAgentPool,
) -> Result<ReassignAgentPoolOutcome, StoreError> {
  let identity = MutationIdentity::new(
    MutationKind::ReassignAgentPool,
    request.idempotency_key.to_string(),
    request.reassigned_at,
    EntityKind::Agent,
    &Fingerprint {
      agent_id: request.agent_id,
      expected_version: request.expected_version,
      target_pool_id: request.target_pool_id,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => {
      let mut outcome: ReassignAgentPoolOutcome = decode_outcome(outcome)?;
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
  };

  let source_pool_id = source_pool(&mut transaction, request.agent_id).await?;
  lock_pools(&mut transaction, source_pool_id, request.target_pool_id).await?;
  let agent = lock_agent(&mut transaction, request.agent_id).await?;
  if agent.pool_id != source_pool_id.as_uuid()
    || agent.version
      != number(
        request.expected_version.get(),
        octacity_server_store::StoreOperation::ReassignAgentPool,
      )?
    || source_pool_id == request.target_pool_id
  {
    return Err(StoreError::Conflict {
      entity: EntityKind::Agent,
    });
  }
  let active_lease: bool = sqlx::query_scalar(
    "SELECT EXISTS (SELECT 1 FROM leases AS lease \
       JOIN agent_registrations AS registration ON registration.id = lease.registration_id \
       WHERE registration.agent_id = $1 \
         AND lease.state IN ('active', 'cancellation_requested', 'drain_requested'))",
  )
  .bind(request.agent_id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if active_lease {
    return Err(StoreError::Conflict {
      entity: EntityKind::Lease,
    });
  }
  let target = current_pool(&mut transaction, request.target_pool_id).await?;
  ensure_compatible(&mut transaction, request.target_pool_id, &target.definition, &agent).await?;

  sqlx::query(
    "UPDATE agent_registrations SET revoked_at = to_timestamp($1::double precision / 1000.0) \
     WHERE agent_id = $2 AND revoked_at IS NULL",
  )
  .bind(request.reassigned_at.unix_millis())
  .bind(request.agent_id.as_uuid())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let updated = sqlx::query_as::<_, AgentRow>(
    "UPDATE agents SET pool_id = $1, pool_version = $2, state = 'offline', version = version + 1, \
       updated_at = to_timestamp($3::double precision / 1000.0) WHERE id = $4 \
     RETURNING id, name, version, pool_id, pool_version, inventory, state, \
       FLOOR(EXTRACT(EPOCH FROM last_seen_at) * 1000)::BIGINT AS last_seen_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis",
  )
  .bind(request.target_pool_id.as_uuid())
  .bind(number(
    target.version.get(),
    octacity_server_store::StoreOperation::ReassignAgentPool,
  )?)
  .bind(request.reassigned_at.unix_millis())
  .bind(request.agent_id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let outcome = ReassignAgentPoolOutcome {
    disposition: MutationDisposition::Applied,
    agent: updated.try_into()?,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "unauthenticated_management",
      actor_identity: None,
      target_identity: request.agent_id.to_string(),
      safe_metadata: json!({"source_pool_id": source_pool_id, "target_pool_id": request.target_pool_id}),
      outbox_payload: json!({"agent_id": request.agent_id, "pool_id": request.target_pool_id, "pool_version": target.version}),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

struct LockedAgent {
  pool_id: uuid::Uuid,
  version: i64,
  platform: Option<AgentPlatform>,
}

struct CurrentPool {
  version: octacity_server_domain::PoolVersion,
  definition: AgentPoolDefinition,
}

async fn source_pool(transaction: &mut Transaction<'_, Postgres>, agent_id: AgentId) -> Result<PoolId, StoreError> {
  let id: uuid::Uuid = sqlx::query_scalar("SELECT pool_id FROM agents WHERE id = $1")
    .bind(agent_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Agent,
    })?;
  PoolId::from_uuid(id).map_err(|_| StoreError::Unavailable)
}

async fn lock_pools(
  transaction: &mut Transaction<'_, Postgres>,
  left: PoolId,
  right: PoolId,
) -> Result<(), StoreError> {
  let mut ids = vec![left, right];
  ids.sort_unstable();
  ids.dedup();
  for id in ids {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
      .bind(format!("octacity.agent-pool.{id}"))
      .execute(&mut **transaction)
      .await
      .map_err(unavailable)?;
  }
  Ok(())
}

async fn lock_agent(transaction: &mut Transaction<'_, Postgres>, agent_id: AgentId) -> Result<LockedAgent, StoreError> {
  let row: (uuid::Uuid, i64, Option<String>, Option<String>) = sqlx::query_as(
    "SELECT pool_id, version, platform_operating_system, platform_architecture FROM agents WHERE id = $1 FOR UPDATE",
  )
  .bind(agent_id.as_uuid())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Agent,
  })?;
  let platform = match (row.2, row.3) {
    (Some(os), Some(architecture)) => Some(AgentPlatform::new(os, architecture).map_err(|_| StoreError::Unavailable)?),
    (None, None) => None,
    _ => return Err(StoreError::Unavailable),
  };
  Ok(LockedAgent {
    pool_id: row.0,
    version: row.1,
    platform,
  })
}

async fn current_pool(transaction: &mut Transaction<'_, Postgres>, pool_id: PoolId) -> Result<CurrentPool, StoreError> {
  let row: (i64, bool, String, serde_json::Value, i64, String, i64) = sqlx::query_as(
    "SELECT version, enabled, drain_state, admission_policy, concurrency_limit, fairness_policy, static_capacity_limit \
     FROM pools WHERE id = $1 ORDER BY version DESC LIMIT 1 FOR UPDATE",
  )
  .bind(pool_id.as_uuid())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Pool,
  })?;
  let version = octacity_server_domain::PoolVersion::new(u64::try_from(row.0).map_err(|_| StoreError::Unavailable)?)
    .map_err(|_| StoreError::Unavailable)?;
  let admission_policy = serde_json::from_value(row.3).map_err(|_| StoreError::Unavailable)?;
  let drain_state = match row.2.as_str() {
    "accepting" => PoolDrainState::Accepting,
    "graceful_drain" => PoolDrainState::GracefulDrain,
    "forced_drain" => PoolDrainState::ForcedDrain,
    "drained" => PoolDrainState::Drained,
    _ => return Err(StoreError::Unavailable),
  };
  Ok(CurrentPool {
    version,
    definition: AgentPoolDefinition {
      enabled: row.1,
      drain_state,
      admission_policy,
      concurrency_limit: u32::try_from(row.4).map_err(|_| StoreError::Unavailable)?,
      fairness_policy: match row.5.as_str() {
        "priority_fifo" => PoolFairnessPolicy::PriorityFifo,
        "configuration_fair" => PoolFairnessPolicy::ConfigurationFair,
        _ => return Err(StoreError::Unavailable),
      },
      static_capacity_limit: u32::try_from(row.6).map_err(|_| StoreError::Unavailable)?,
    },
  })
}

async fn ensure_compatible(
  transaction: &mut Transaction<'_, Postgres>,
  target_pool_id: PoolId,
  definition: &AgentPoolDefinition,
  agent: &LockedAgent,
) -> Result<(), StoreError> {
  let accepts_platform = match &definition.admission_policy {
    PoolAdmissionPolicy::Any => true,
    PoolAdmissionPolicy::Allowlist { platforms } => agent
      .platform
      .as_ref()
      .is_some_and(|platform| platforms.contains(platform)),
  };
  let assigned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE pool_id = $1")
    .bind(target_pool_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if !definition.enabled
    || definition.drain_state != PoolDrainState::Accepting
    || !accepts_platform
    || assigned >= i64::from(definition.static_capacity_limit)
  {
    return Err(StoreError::Conflict {
      entity: EntityKind::Pool,
    });
  }
  Ok(())
}
