use octacity_server_domain::{AgentId, BuildId, PoolId, Timestamp};
use octacity_server_store::{
  CriticalSystemConditionChange, ListOperatorAttention, OperatorAttentionEvent, OperatorAttentionEventKind,
  OperatorAttentionId, OperatorAttentionPage, OperatorAttentionPagePosition, OperatorAttentionTarget, StoreError,
};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{database::unavailable, discovery};

const BUILD_FAILED: &str = "build_failed";
const AGENT_UNAVAILABLE: &str = "agent_unavailable";
const AGENT_POOL_UNAVAILABLE: &str = "agent_pool_unavailable";

#[derive(FromRow)]
struct OperatorAttentionRow {
  id: Uuid,
  source_kind: String,
  target_id: Option<Uuid>,
  code: Option<String>,
  summary: String,
  occurred_at_millis: i64,
  resolved_at_millis: Option<i64>,
}

pub(crate) async fn list(pool: &PgPool, request: ListOperatorAttention) -> Result<OperatorAttentionPage, StoreError> {
  let mut builds = Vec::new();
  let mut agents = Vec::new();
  let mut pools = Vec::new();
  for target in request.scope().targets() {
    if !request.visibility().allows_target(target) {
      continue;
    }
    match target {
      OperatorAttentionTarget::Build(identity) => builds.push(identity.as_uuid()),
      OperatorAttentionTarget::Agent(identity) => agents.push(identity.as_uuid()),
      OperatorAttentionTarget::AgentPool(identity) => pools.push(identity.as_uuid()),
    }
  }
  let include_critical =
    request.scope().includes_critical_conditions() && request.visibility().allows_critical_conditions();
  if builds.is_empty() && agents.is_empty() && pools.is_empty() && !include_critical {
    return Ok(OperatorAttentionPage {
      items: Vec::new(),
      next_cursor: None,
    });
  }

  let after = request.after();
  let rows = sqlx::query_as::<_, OperatorAttentionRow>(
    "SELECT id, source_kind, target_id, code, summary, \
       FLOOR(EXTRACT(EPOCH FROM occurred_at) * 1000)::BIGINT AS occurred_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM resolved_at) * 1000)::BIGINT AS resolved_at_millis \
     FROM operator_attention_events \
     WHERE ((source_kind = 'build_failed' AND target_kind = 'build' AND target_id = ANY($1::uuid[])) \
         OR (source_kind = 'agent_unavailable' AND target_kind = 'agent' AND target_id = ANY($2::uuid[])) \
         OR (source_kind = 'agent_pool_unavailable' AND target_kind = 'agent_pool' AND target_id = ANY($3::uuid[])) \
         OR (source_kind = 'critical_system_condition' AND $4)) \
       AND ($5::bigint IS NULL OR occurred_at >= to_timestamp($5::double precision / 1000.0)) \
       AND ($6::bigint IS NULL OR occurred_at <= to_timestamp($6::double precision / 1000.0)) \
       AND ($7::bigint IS NULL OR (occurred_at, id) < \
         (to_timestamp($7::double precision / 1000.0), $8::uuid)) \
     ORDER BY occurred_at DESC, id DESC LIMIT $9",
  )
  .bind(builds)
  .bind(agents)
  .bind(pools)
  .bind(include_critical)
  .bind(request.scope().occurred_from().map(Timestamp::unix_millis))
  .bind(request.scope().occurred_through().map(Timestamp::unix_millis))
  .bind(after.map(|position| position.occurred_at.unix_millis()))
  .bind(after.map(|position| position.id.as_uuid()))
  .bind(discovery::requested_row_limit(request.limit().get()))
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;

  let (items, next_cursor) = discovery::decode_page(rows, request.limit().get(), decode, |item| {
    OperatorAttentionPagePosition {
      occurred_at: item.occurred_at,
      id: item.id,
    }
  })?;
  Ok(OperatorAttentionPage { items, next_cursor })
}

pub(crate) async fn record_critical_system_condition(
  pool: &PgPool,
  change: CriticalSystemConditionChange,
) -> Result<(), StoreError> {
  match change {
    CriticalSystemConditionChange::Open {
      source,
      code,
      summary,
      occurred_at,
    } => {
      sqlx::query(
        "INSERT INTO operator_attention_events \
           (id, source_kind, target_kind, target_id, source_identity, code, summary, occurred_at, resolved_at) \
         VALUES ($1, 'critical_system_condition', NULL, NULL, $2, $3, $4, \
           to_timestamp($5::double precision / 1000.0), NULL) \
         ON CONFLICT (source_identity, code) \
           WHERE source_kind = 'critical_system_condition' AND resolved_at IS NULL \
         DO NOTHING",
      )
      .bind(Uuid::new_v4())
      .bind(source.as_uuid())
      .bind(code)
      .bind(summary)
      .bind(occurred_at.unix_millis())
      .execute(pool)
      .await
      .map_err(unavailable)?;
    }
    CriticalSystemConditionChange::Resolve {
      source,
      code,
      resolved_at,
    } => {
      sqlx::query(
        "UPDATE operator_attention_events \
         SET resolved_at = GREATEST(occurred_at, to_timestamp($1::double precision / 1000.0)) \
         WHERE source_kind = 'critical_system_condition' AND source_identity = $2 \
           AND code = $3 AND resolved_at IS NULL",
      )
      .bind(resolved_at.unix_millis())
      .bind(source.as_uuid())
      .bind(code)
      .execute(pool)
      .await
      .map_err(unavailable)?;
    }
  }
  Ok(())
}

fn decode(row: OperatorAttentionRow) -> Result<octacity_server_store::OperatorAttentionItem, StoreError> {
  let target_id = || row.target_id.ok_or(StoreError::Unavailable);
  let kind = match row.source_kind.as_str() {
    BUILD_FAILED => {
      OperatorAttentionEventKind::BuildFailed(BuildId::from_uuid(target_id()?).map_err(|_| StoreError::Unavailable)?)
    }
    AGENT_UNAVAILABLE => OperatorAttentionEventKind::AgentUnavailable(
      AgentId::from_uuid(target_id()?).map_err(|_| StoreError::Unavailable)?,
    ),
    AGENT_POOL_UNAVAILABLE => OperatorAttentionEventKind::AgentPoolUnavailable(
      PoolId::from_uuid(target_id()?).map_err(|_| StoreError::Unavailable)?,
    ),
    "critical_system_condition" => OperatorAttentionEventKind::CriticalSystemCondition {
      code: row.code.ok_or(StoreError::Unavailable)?,
    },
    _ => return Err(StoreError::Unavailable),
  };
  OperatorAttentionEvent::new(
    OperatorAttentionId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    kind,
    row.summary,
    discovery::timestamp(row.occurred_at_millis)?,
    row.resolved_at_millis.map(discovery::timestamp).transpose()?,
  )
  .map_err(|_| StoreError::Unavailable)?
  .classify()
  .ok_or(StoreError::Unavailable)
}

pub(crate) enum TargetAttentionChange {
  Open {
    source_kind: &'static str,
    target_kind: &'static str,
    target_id: Uuid,
    summary: String,
  },
  Resolve {
    source_kind: &'static str,
    target_kind: &'static str,
    target_id: Uuid,
  },
}

impl TargetAttentionChange {
  pub(crate) fn build_failed(identity: BuildId) -> Self {
    Self::Open {
      source_kind: BUILD_FAILED,
      target_kind: "build",
      target_id: identity.as_uuid(),
      summary: format!("Build {identity} failed"),
    }
  }

  pub(crate) fn resolve_build(identity: BuildId) -> Self {
    Self::Resolve {
      source_kind: BUILD_FAILED,
      target_kind: "build",
      target_id: identity.as_uuid(),
    }
  }

  pub(crate) fn agent_unavailable(identity: AgentId) -> Self {
    Self::Open {
      source_kind: AGENT_UNAVAILABLE,
      target_kind: "agent",
      target_id: identity.as_uuid(),
      summary: format!("Agent {identity} is unavailable"),
    }
  }

  pub(crate) fn resolve_agent(identity: AgentId) -> Self {
    Self::Resolve {
      source_kind: AGENT_UNAVAILABLE,
      target_kind: "agent",
      target_id: identity.as_uuid(),
    }
  }

  pub(crate) fn agent_pool_unavailable(identity: PoolId) -> Self {
    Self::Open {
      source_kind: AGENT_POOL_UNAVAILABLE,
      target_kind: "agent_pool",
      target_id: identity.as_uuid(),
      summary: format!("Agent Pool {identity} is unavailable"),
    }
  }

  pub(crate) fn resolve_agent_pool(identity: PoolId) -> Self {
    Self::Resolve {
      source_kind: AGENT_POOL_UNAVAILABLE,
      target_kind: "agent_pool",
      target_id: identity.as_uuid(),
    }
  }
}

pub(crate) async fn apply_change(
  transaction: &mut Transaction<'_, Postgres>,
  event_id: Uuid,
  occurred_at: Timestamp,
  change: &TargetAttentionChange,
) -> Result<(), StoreError> {
  match change {
    TargetAttentionChange::Open {
      source_kind,
      target_kind,
      target_id,
      summary,
    } => {
      sqlx::query(
        "INSERT INTO operator_attention_events \
           (id, source_kind, target_kind, target_id, source_identity, code, summary, occurred_at, resolved_at) \
         VALUES ($1, $2, $3, $4, NULL, NULL, $5, to_timestamp($6::double precision / 1000.0), NULL)",
      )
      .bind(event_id)
      .bind(source_kind)
      .bind(target_kind)
      .bind(target_id)
      .bind(summary)
      .bind(occurred_at.unix_millis())
      .execute(&mut **transaction)
      .await
      .map_err(unavailable)?;
    }
    TargetAttentionChange::Resolve {
      source_kind,
      target_kind,
      target_id,
    } => {
      sqlx::query(
        "UPDATE operator_attention_events \
         SET resolved_at = to_timestamp($1::double precision / 1000.0) \
         WHERE source_kind = $2 AND target_kind = $3 AND target_id = $4 AND resolved_at IS NULL",
      )
      .bind(occurred_at.unix_millis())
      .bind(source_kind)
      .bind(target_kind)
      .bind(target_id)
      .execute(&mut **transaction)
      .await
      .map_err(unavailable)?;
    }
  }
  Ok(())
}
