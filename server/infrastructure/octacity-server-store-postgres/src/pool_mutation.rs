use octacity_server_domain::{EntityKind, PoolId, PoolName, PoolVersion};
use octacity_server_store::{
  AgentPoolDefinition, AgentPoolMutationOutcome, CreateAgentPool, DeleteAgentPool, DeleteAgentPoolOutcome,
  MutationDisposition, PublishAgentPoolVersion, PublishedAgentPool, StoreError, StoreOperation,
  validate_pool_drain_transition,
};
use serde::Serialize;
use serde_json::json;
use sqlx::{Postgres, Transaction, types::Json};

use crate::{
  database::{number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
  pool_row::PoolRow,
};

#[derive(Serialize)]
struct CreateFingerprint<'a> {
  name: &'a PoolName,
  definition: &'a AgentPoolDefinition,
}

#[derive(Serialize)]
struct PublishFingerprint<'a> {
  id: PoolId,
  expected_current_version: PoolVersion,
  definition: &'a AgentPoolDefinition,
}

#[derive(Serialize)]
struct DeleteFingerprint {
  id: PoolId,
  expected_current_version: PoolVersion,
}

pub(crate) async fn create(
  pool: &sqlx::PgPool,
  request: CreateAgentPool,
) -> Result<AgentPoolMutationOutcome, StoreError> {
  request
    .definition
    .validate()
    .map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::CreateAgentPool,
      source,
    })?;
  let identity = MutationIdentity::new(
    MutationKind::CreateAgentPool,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Pool,
    &CreateFingerprint {
      name: &request.name,
      definition: &request.definition,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_pool(outcome),
  };
  lock_pool_catalog(&mut transaction).await?;
  let name_exists: bool = sqlx::query_scalar(
    "SELECT EXISTS (SELECT 1 FROM (SELECT DISTINCT ON (id) id, name FROM pools ORDER BY id, version DESC) AS current WHERE name = $1)",
  )
  .bind(request.name.as_str())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if name_exists {
    return Err(StoreError::Conflict {
      entity: EntityKind::Pool,
    });
  }
  let row = insert_version(
    &mut transaction,
    request.id,
    &request.name,
    PoolVersion::INITIAL,
    &request.definition,
    request.published_at.unix_millis(),
    StoreOperation::CreateAgentPool,
  )
  .await
  .map_err(classify_create)?;
  let outcome = AgentPoolMutationOutcome {
    disposition: MutationDisposition::Applied,
    pool: row.try_into()?,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    pool_facts(&outcome.pool),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn publish(
  pool: &sqlx::PgPool,
  request: PublishAgentPoolVersion,
) -> Result<AgentPoolMutationOutcome, StoreError> {
  request
    .definition
    .validate()
    .map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::PublishAgentPoolVersion,
      source,
    })?;
  let identity = MutationIdentity::new(
    MutationKind::PublishAgentPoolVersion,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Pool,
    &PublishFingerprint {
      id: request.id,
      expected_current_version: request.expected_current_version,
      definition: &request.definition,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_pool(outcome),
  };
  lock_pool(&mut transaction, request.id).await?;
  let current = current(&mut transaction, request.id).await?;
  if current.version != request.expected_current_version || request.published_at < current.published_at {
    return Err(StoreError::Conflict {
      entity: EntityKind::Pool,
    });
  }
  let active_leases: bool = sqlx::query_scalar(
    "SELECT EXISTS (SELECT 1 FROM leases WHERE pool_id = $1 AND state IN ('active', 'cancellation_requested', 'drain_requested'))",
  )
  .bind(request.id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  validate_pool_drain_transition(
    current.definition.drain_state,
    request.definition.drain_state,
    active_leases,
  )?;
  apply_lease_directive(&mut transaction, request.id, request.definition.drain_state).await?;
  let next_version = current.version.next().map_err(|_| StoreError::Unavailable)?;
  let row = insert_version(
    &mut transaction,
    request.id,
    &current.name,
    next_version,
    &request.definition,
    request.published_at.unix_millis(),
    StoreOperation::PublishAgentPoolVersion,
  )
  .await
  .map_err(|_| StoreError::Conflict {
    entity: EntityKind::Pool,
  })?;
  let outcome = AgentPoolMutationOutcome {
    disposition: MutationDisposition::Applied,
    pool: row.try_into()?,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    pool_facts(&outcome.pool),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

async fn apply_lease_directive(
  transaction: &mut Transaction<'_, Postgres>,
  pool_id: PoolId,
  state: octacity_server_scheduler::PoolDrainState,
) -> Result<(), StoreError> {
  let requested = match state {
    octacity_server_scheduler::PoolDrainState::GracefulDrain => Some("drain_requested"),
    octacity_server_scheduler::PoolDrainState::ForcedDrain => Some("cancellation_requested"),
    octacity_server_scheduler::PoolDrainState::Accepting | octacity_server_scheduler::PoolDrainState::Drained => None,
  };
  if let Some(requested) = requested {
    sqlx::query(
      "UPDATE leases SET state = $1, version = version + 1 WHERE pool_id = $2 \
       AND state IN ('active', 'drain_requested')",
    )
    .bind(requested)
    .bind(pool_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  }
  Ok(())
}

pub(crate) async fn delete(
  pool: &sqlx::PgPool,
  request: DeleteAgentPool,
) -> Result<DeleteAgentPoolOutcome, StoreError> {
  let identity = MutationIdentity::new(
    MutationKind::DeleteAgentPool,
    request.idempotency_key.to_string(),
    request.deleted_at,
    EntityKind::Pool,
    &DeleteFingerprint {
      id: request.id,
      expected_current_version: request.expected_current_version,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_delete(outcome),
  };
  lock_pool(&mut transaction, request.id).await?;
  let current = current(&mut transaction, request.id).await?;
  if current.version != request.expected_current_version || request.deleted_at < current.published_at {
    return Err(StoreError::Conflict {
      entity: EntityKind::Pool,
    });
  }
  let referenced: bool = sqlx::query_scalar(
    "SELECT \
       EXISTS (SELECT 1 FROM build_configuration_versions WHERE $1 = ANY(allowed_pool_ids)) OR \
       EXISTS (SELECT 1 FROM agents WHERE pool_id = $1) OR \
       EXISTS (SELECT 1 FROM leases WHERE pool_id = $1 AND state IN ('active', 'cancellation_requested', 'drain_requested')) OR \
       EXISTS (SELECT 1 FROM ready_queue_entries WHERE $1 = ANY(allowed_pool_ids)) OR \
       EXISTS (SELECT 1 FROM jobs WHERE state = 'ready' AND $1 = ANY(allowed_pool_ids)) OR \
       EXISTS (SELECT 1 FROM agent_enrollment_credentials WHERE pool_id = $1)",
  )
  .bind(request.id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if referenced {
    return Err(StoreError::Conflict {
      entity: EntityKind::Pool,
    });
  }
  sqlx::query("DELETE FROM pools WHERE id = $1")
    .bind(request.id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(|_| StoreError::Conflict {
      entity: EntityKind::Pool,
    })?;
  let outcome = DeleteAgentPoolOutcome {
    disposition: MutationDisposition::Applied,
    pool_id: request.id,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "unauthenticated_management",
      actor_identity: None,
      target_identity: request.id.to_string(),
      safe_metadata: json!({"version": request.expected_current_version.get()}),
      outbox_payload: json!({"pool_id": request.id}),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

async fn insert_version(
  transaction: &mut Transaction<'_, Postgres>,
  id: PoolId,
  name: &PoolName,
  version: PoolVersion,
  definition: &AgentPoolDefinition,
  published_at_millis: i64,
  operation: StoreOperation,
) -> Result<PoolRow, sqlx::Error> {
  sqlx::query_as::<_, PoolRow>(
    "INSERT INTO pools (id, version, name, enabled, drain_state, admission_policy, concurrency_limit, fairness_policy, static_capacity_limit, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::double precision / 1000.0)) \
     RETURNING id, name, version, enabled, drain_state, admission_policy, concurrency_limit, fairness_policy, static_capacity_limit, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS published_at_millis",
  )
  .bind(id.as_uuid())
  .bind(number(version.get(), operation).map_err(|_| sqlx::Error::Protocol("pool version exceeds PostgreSQL range".to_owned()))?)
  .bind(name.as_str())
  .bind(definition.enabled)
  .bind(drain_state(definition.drain_state))
  .bind(Json(&definition.admission_policy))
  .bind(i64::from(definition.concurrency_limit))
  .bind(match definition.fairness_policy {
    octacity_server_store::PoolFairnessPolicy::PriorityFifo => "priority_fifo",
    octacity_server_store::PoolFairnessPolicy::ConfigurationFair => "configuration_fair",
  })
  .bind(i64::from(definition.static_capacity_limit))
  .bind(published_at_millis)
  .fetch_one(&mut **transaction)
  .await
}

async fn current(
  transaction: &mut Transaction<'_, Postgres>,
  pool_id: PoolId,
) -> Result<PublishedAgentPool, StoreError> {
  sqlx::query_as::<_, PoolRow>(
    "SELECT id, name, version, enabled, drain_state, admission_policy, concurrency_limit, fairness_policy, static_capacity_limit, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS published_at_millis \
     FROM pools WHERE id = $1 ORDER BY version DESC LIMIT 1 FOR UPDATE",
  )
  .bind(pool_id.as_uuid())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Pool,
  })?
  .try_into()
}

async fn lock_pool_catalog(transaction: &mut Transaction<'_, Postgres>) -> Result<(), StoreError> {
  sqlx::query("SELECT pg_advisory_xact_lock(hashtext('octacity.agent-pools.catalog'))")
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  Ok(())
}

async fn lock_pool(transaction: &mut Transaction<'_, Postgres>, pool_id: PoolId) -> Result<(), StoreError> {
  sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
    .bind(format!("octacity.agent-pool.{pool_id}"))
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  Ok(())
}

fn drain_state(state: octacity_server_scheduler::PoolDrainState) -> &'static str {
  match state {
    octacity_server_scheduler::PoolDrainState::Accepting => "accepting",
    octacity_server_scheduler::PoolDrainState::GracefulDrain => "graceful_drain",
    octacity_server_scheduler::PoolDrainState::ForcedDrain => "forced_drain",
    octacity_server_scheduler::PoolDrainState::Drained => "drained",
  }
}

fn pool_facts(pool: &PublishedAgentPool) -> MutationFacts {
  MutationFacts {
    actor_kind: "unauthenticated_management",
    actor_identity: None,
    target_identity: pool.id.to_string(),
    safe_metadata: json!({"version": pool.version.get(), "enabled": pool.definition.enabled, "drain_state": pool.definition.drain_state}),
    outbox_payload: json!({"pool_id": pool.id, "version": pool.version.get()}),
  }
}

fn replay_pool(outcome: serde_json::Value) -> Result<AgentPoolMutationOutcome, StoreError> {
  let mut outcome: AgentPoolMutationOutcome = decode_outcome(outcome)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

fn replay_delete(outcome: serde_json::Value) -> Result<DeleteAgentPoolOutcome, StoreError> {
  let mut outcome: DeleteAgentPoolOutcome = decode_outcome(outcome)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

fn classify_create(error: sqlx::Error) -> StoreError {
  if let sqlx::Error::Database(database) = &error
    && database.is_unique_violation()
  {
    return StoreError::Duplicate {
      entity: EntityKind::Pool,
    };
  }
  unavailable(error)
}
