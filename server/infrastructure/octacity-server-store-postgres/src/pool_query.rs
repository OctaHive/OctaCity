use octacity_server_domain::{EntityKind, PoolId, PoolVersion};
use octacity_server_store::{AgentPoolPage, ListAgentPools, PublishedAgentPool, StoreError};

use crate::{database::unavailable, pool_row::PoolRow};

pub(crate) async fn read(
  pool: &sqlx::PgPool,
  pool_id: PoolId,
  version: PoolVersion,
) -> Result<PublishedAgentPool, StoreError> {
  sqlx::query_as::<_, PoolRow>(
    "SELECT id, name, version, enabled, drain_state, admission_policy, concurrency_limit, fairness_policy, \
       static_capacity_limit, FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS published_at_millis \
     FROM pools WHERE id = $1 AND version = $2",
  )
  .bind(pool_id.as_uuid())
  .bind(i64::try_from(version.get()).map_err(|_| StoreError::Unavailable)?)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Pool,
  })?
  .try_into()
}

pub(crate) async fn list(pool: &sqlx::PgPool, request: ListAgentPools) -> Result<AgentPoolPage, StoreError> {
  let limit = i64::from(request.limit().get()) + 1;
  let rows = sqlx::query_as::<_, PoolRow>(
    "SELECT id, name, version, enabled, drain_state, admission_policy, concurrency_limit, fairness_policy, \
       static_capacity_limit, FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS published_at_millis \
     FROM (SELECT DISTINCT ON (id) * FROM pools ORDER BY id, version DESC) AS current \
     WHERE ($1::uuid IS NULL OR id > $1) ORDER BY id LIMIT $2",
  )
  .bind(request.after().map(PoolId::as_uuid))
  .bind(limit)
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let page_limit = usize::from(request.limit().get());
  let mut pools = rows
    .into_iter()
    .map(PublishedAgentPool::try_from)
    .collect::<Result<Vec<_>, _>>()?;
  let has_more = pools.len() > page_limit;
  pools.truncate(page_limit);
  let next_cursor = has_more.then(|| pools.last().expect("a non-zero full page has a last item").id);
  Ok(AgentPoolPage { pools, next_cursor })
}
