use octacity_server_domain::{AgentId, AttemptId, BuildId, EntityKind, JobId, LeaseId};
use octacity_server_store::{
  AgentCurrentExecution, AgentCurrentLeaseState, AgentDetail, AgentPage, EnrolledAgent, ListAgents, StoreError,
};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::{agent_row::AgentRow, database::unavailable, read_visibility::sql_read_visibility};

async fn read_from(connection: &mut PgConnection, agent_id: AgentId) -> Result<EnrolledAgent, StoreError> {
  sqlx::query_as::<_, AgentRow>(
    "SELECT id, name, version, pool_id, pool_version, inventory, state, \
       FLOOR(EXTRACT(EPOCH FROM last_seen_at) * 1000)::BIGINT AS last_seen_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM agents WHERE id = $1",
  )
  .bind(agent_id.as_uuid())
  .fetch_optional(connection)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Agent,
  })?
  .try_into()
}

#[derive(FromRow)]
struct CurrentExecutionRow {
  lease_id: Uuid,
  build_id: Uuid,
  attempt_id: Uuid,
  job_id: Uuid,
  lease_state: String,
}

pub(crate) async fn read_detail(pool: &sqlx::PgPool, agent_id: AgentId) -> Result<AgentDetail, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  let agent = read_from(&mut transaction, agent_id).await?;
  let current_execution = read_current_execution(&mut transaction, agent_id).await?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(AgentDetail {
    agent,
    current_execution,
  })
}

pub(crate) async fn read_current_execution(
  connection: &mut PgConnection,
  agent_id: AgentId,
) -> Result<Option<AgentCurrentExecution>, StoreError> {
  let current = sqlx::query_as::<_, CurrentExecutionRow>(
    "SELECT lease.id AS lease_id, attempt.build_id, job.attempt_id, lease.job_id, lease.state AS lease_state \
     FROM leases AS lease \
     JOIN jobs AS job ON job.id = lease.job_id \
     JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     JOIN agent_registrations AS registration ON registration.id = lease.registration_id \
     WHERE registration.agent_id = $1 \
       AND registration.epoch = lease.registration_epoch \
       AND registration.revoked_at IS NULL AND registration.expires_at > now() \
       AND lease.expires_at > now() \
       AND lease.state IN ('active', 'cancellation_requested', 'drain_requested') \
     ORDER BY lease.leased_at DESC, lease.id DESC LIMIT 1",
  )
  .bind(agent_id.as_uuid())
  .fetch_optional(connection)
  .await
  .map_err(unavailable)?;
  current.map(current_execution).transpose()
}

fn current_execution(row: CurrentExecutionRow) -> Result<AgentCurrentExecution, StoreError> {
  Ok(AgentCurrentExecution {
    lease_id: LeaseId::from_uuid(row.lease_id).map_err(|_| StoreError::Unavailable)?,
    build_id: BuildId::from_uuid(row.build_id).map_err(|_| StoreError::Unavailable)?,
    attempt_id: AttemptId::from_uuid(row.attempt_id).map_err(|_| StoreError::Unavailable)?,
    job_id: JobId::from_uuid(row.job_id).map_err(|_| StoreError::Unavailable)?,
    lease_state: match row.lease_state.as_str() {
      "active" => AgentCurrentLeaseState::Active,
      "cancellation_requested" => AgentCurrentLeaseState::CancellationRequested,
      "drain_requested" => AgentCurrentLeaseState::DrainRequested,
      _ => return Err(StoreError::Unavailable),
    },
  })
}

pub(crate) async fn list(pool: &sqlx::PgPool, request: ListAgents) -> Result<AgentPage, StoreError> {
  if request.visibility().kind() == octacity_server_store::ReadVisibilityKind::None {
    return Ok(AgentPage {
      agents: Vec::new(),
      next_cursor: None,
    });
  }
  let visibility = sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
  let rows = sqlx::query_as::<_, AgentRow>(
    "SELECT id, name, version, pool_id, pool_version, inventory, state, \
       FLOOR(EXTRACT(EPOCH FROM last_seen_at) * 1000)::BIGINT AS last_seen_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM agents \
     WHERE ($1 OR id = ANY($2::uuid[])) \
       AND ($3::uuid IS NULL OR pool_id = $3) \
       AND ($4::uuid IS NULL OR id > $4) \
     ORDER BY id LIMIT $5",
  )
  .bind(visibility.all)
  .bind(visibility.identities)
  .bind(request.pool_id().map(octacity_server_domain::PoolId::as_uuid))
  .bind(request.after().map(AgentId::as_uuid))
  .bind(i64::from(request.limit().get()) + 1)
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let limit = usize::from(request.limit().get());
  let mut agents = rows
    .into_iter()
    .map(EnrolledAgent::try_from)
    .collect::<Result<Vec<_>, _>>()?;
  let has_more = agents.len() > limit;
  agents.truncate(limit);
  let next_cursor = has_more.then(|| agents.last().expect("non-zero full page has a last Agent").id);
  Ok(AgentPage { agents, next_cursor })
}
