use octacity_server_domain::{AgentId, EntityKind};
use octacity_server_store::{AgentPage, EnrolledAgent, ListAgents, StoreError};

use crate::{agent_row::AgentRow, database::unavailable};

pub(crate) async fn read(pool: &sqlx::PgPool, agent_id: AgentId) -> Result<EnrolledAgent, StoreError> {
  sqlx::query_as::<_, AgentRow>(
    "SELECT id, name, version, pool_id, pool_version, inventory, state, \
       FLOOR(EXTRACT(EPOCH FROM last_seen_at) * 1000)::BIGINT AS last_seen_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM agents WHERE id = $1",
  )
  .bind(agent_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Agent,
  })?
  .try_into()
}

pub(crate) async fn list(pool: &sqlx::PgPool, request: ListAgents) -> Result<AgentPage, StoreError> {
  let rows = sqlx::query_as::<_, AgentRow>(
    "SELECT id, name, version, pool_id, pool_version, inventory, state, \
       FLOOR(EXTRACT(EPOCH FROM last_seen_at) * 1000)::BIGINT AS last_seen_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM agents WHERE ($1::uuid IS NULL OR id > $1) ORDER BY id LIMIT $2",
  )
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
