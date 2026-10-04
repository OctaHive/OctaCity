mod support;

use std::collections::BTreeSet;

use octacity_server_domain::{AgentId, BuildId, PoolId, PoolName, PoolVersion, Timestamp};
use octacity_server_scheduler::PoolDrainState;
use octacity_server_store::testing::{InMemoryOperatorAttentionStore, management_mutation};
use octacity_server_store::{
  AgentPoolDefinition, AgentPoolStore as _, CreateAgentPool, CriticalSystemConditionChange,
  CriticalSystemConditionSourceId, CriticalSystemConditionStore as _, IdempotencyKey, ListOperatorAttention,
  OperatorAttentionEvent, OperatorAttentionEventKind, OperatorAttentionId, OperatorAttentionPage,
  OperatorAttentionScope, OperatorAttentionStore as _, OperatorAttentionTarget, OperatorAttentionVisibility,
  PoolAdmissionPolicy, PoolFairnessPolicy, PublishAgentPoolVersion,
};
use octacity_server_store_postgres::PostgresStore;
use sqlx::Executor as _;
use support::TestDatabase;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_operator_attention_matches_the_in_memory_contract() {
  let database = TestDatabase::migrated().await;
  let memory = InMemoryOperatorAttentionStore::new();
  let postgres = PostgresStore::new(database.pool.clone());
  let fixture = Fixture::new();
  seed_fixture(&database.pool, &memory, &fixture).await;

  let restricted = || {
    OperatorAttentionVisibility::restricted(
      [
        OperatorAttentionTarget::Build(fixture.build),
        OperatorAttentionTarget::Agent(fixture.agent),
        OperatorAttentionTarget::AgentPool(fixture.pool),
      ],
      true,
    )
    .unwrap()
  };
  let first = assert_parity(
    &memory,
    &postgres,
    request(fixture.visible_scope(), None, 2, restricted()),
  )
  .await;
  assert_eq!(ids(&first), [attention_id(5), attention_id(3)]);
  assert!(first.items[0].resolved_at.is_some());
  assert!(first.items[1].resolved_at.is_some());

  let second = assert_parity(
    &memory,
    &postgres,
    request(fixture.visible_scope(), first.next_cursor, 2, restricted()),
  )
  .await;
  assert_eq!(ids(&second), [attention_id(1), attention_id(4)]);
  assert_eq!(second.next_cursor, None);

  let pool_only = assert_parity(
    &memory,
    &postgres,
    request(
      scope([OperatorAttentionTarget::AgentPool(fixture.pool)], false),
      None,
      20,
      restricted(),
    ),
  )
  .await;
  assert_eq!(ids(&pool_only), [attention_id(4)]);

  let critical_only = assert_parity(&memory, &postgres, request(scope([], true), None, 20, restricted())).await;
  assert_eq!(ids(&critical_only), [attention_id(5)]);

  let bounded = OperatorAttentionScope::new(
    BTreeSet::from([OperatorAttentionTarget::Build(fixture.build)]),
    true,
    Some(timestamp(1_000)),
    Some(timestamp(1_000)),
  )
  .unwrap();
  let equal_time = assert_parity(&memory, &postgres, request(bounded, None, 20, restricted())).await;
  assert_eq!(ids(&equal_time), [attention_id(5), attention_id(1)]);

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn representative_operator_attention_plans_use_the_scoped_indexes() {
  let database = TestDatabase::migrated().await;
  let memory = InMemoryOperatorAttentionStore::new();
  let fixture = Fixture::new();
  seed_fixture(&database.pool, &memory, &fixture).await;
  sqlx::query("ANALYZE operator_attention_events")
    .execute(&database.pool)
    .await
    .unwrap();
  let mut transaction = database.pool.begin().await.unwrap();
  transaction.execute("SET LOCAL enable_seqscan = off").await.unwrap();

  let target = sqlx::query_scalar::<_, String>(
    "EXPLAIN (FORMAT TEXT) SELECT id FROM operator_attention_events \
     WHERE ((source_kind = 'build_failed' AND target_kind = 'build' AND target_id = ANY($1::uuid[])) \
         OR (source_kind = 'agent_unavailable' AND target_kind = 'agent' AND target_id = ANY('{}'::uuid[])) \
         OR (source_kind = 'agent_pool_unavailable' AND target_kind = 'agent_pool' AND target_id = ANY('{}'::uuid[])) \
         OR (source_kind = 'critical_system_condition' AND false)) \
     ORDER BY occurred_at DESC, id DESC LIMIT 10",
  )
  .bind([fixture.build.as_uuid()].as_slice())
  .fetch_all(&mut *transaction)
  .await
  .unwrap()
  .join("\n");
  assert!(target.contains("operator_attention_target_order_idx"), "{target}");

  let critical = sqlx::query_scalar::<_, String>(
    "EXPLAIN (FORMAT TEXT) SELECT id FROM operator_attention_events \
     WHERE ((source_kind = 'build_failed' AND target_kind = 'build' AND target_id = ANY('{}'::uuid[])) \
         OR (source_kind = 'agent_unavailable' AND target_kind = 'agent' AND target_id = ANY('{}'::uuid[])) \
         OR (source_kind = 'agent_pool_unavailable' AND target_kind = 'agent_pool' AND target_id = ANY('{}'::uuid[])) \
         OR (source_kind = 'critical_system_condition' AND true)) \
     ORDER BY occurred_at DESC, id DESC LIMIT 10",
  )
  .fetch_all(&mut *transaction)
  .await
  .unwrap()
  .join("\n");
  assert!(critical.contains("operator_attention_critical_order_idx"), "{critical}");

  transaction.rollback().await.unwrap();
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn critical_system_condition_source_is_idempotent_and_resolvable() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let source = CriticalSystemConditionSourceId::from_uuid(Uuid::from_u128(1)).unwrap();
  let open = || {
    CriticalSystemConditionChange::open(
      source,
      "server_readiness_test",
      "Server readiness lost: test",
      timestamp(10),
    )
    .unwrap()
  };

  store.record_critical_system_condition(open()).await.unwrap();
  store.record_critical_system_condition(open()).await.unwrap();
  let active = store
    .list_operator_attention(request(scope([], true), None, 20, OperatorAttentionVisibility::all()))
    .await
    .unwrap();
  assert_eq!(active.items.len(), 1);
  assert_eq!(active.items[0].code, "server_readiness_test");
  assert_eq!(active.items[0].resolved_at, None);

  store
    .record_critical_system_condition(
      CriticalSystemConditionChange::resolve(source, "server_readiness_test", timestamp(20)).unwrap(),
    )
    .await
    .unwrap();
  let resolved = store
    .list_operator_attention(request(scope([], true), None, 20, OperatorAttentionVisibility::all()))
    .await
    .unwrap();
  assert_eq!(resolved.items[0].resolved_at, Some(timestamp(20)));

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pool_availability_transitions_open_once_and_resolve_atomically() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let pool = pool_id(401);
  let unavailable = pool_definition(false);
  store
    .create_agent_pool(management_mutation(CreateAgentPool {
      id: pool,
      name: PoolName::new("attention-pool").unwrap(),
      definition: unavailable.clone(),
      idempotency_key: key("create-attention-pool"),
      published_at: timestamp(10),
    }))
    .await
    .unwrap();

  let initial = store
    .list_operator_attention(request(
      scope([OperatorAttentionTarget::AgentPool(pool)], false),
      None,
      20,
      OperatorAttentionVisibility::all(),
    ))
    .await
    .unwrap();
  assert_eq!(initial.items.len(), 1);
  assert_eq!(initial.items[0].resolved_at, None);

  store
    .publish_agent_pool_version(management_mutation(PublishAgentPoolVersion {
      id: pool,
      expected_current_version: PoolVersion::INITIAL,
      definition: unavailable,
      idempotency_key: key("keep-attention-pool-unavailable"),
      published_at: timestamp(20),
    }))
    .await
    .unwrap();
  let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operator_attention_events")
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(count, 1, "unchanged unavailability must not duplicate a transition");

  store
    .publish_agent_pool_version(management_mutation(PublishAgentPoolVersion {
      id: pool,
      expected_current_version: PoolVersion::new(2).unwrap(),
      definition: pool_definition(true),
      idempotency_key: key("restore-attention-pool"),
      published_at: timestamp(30),
    }))
    .await
    .unwrap();
  let resolved = store
    .list_operator_attention(request(
      scope([OperatorAttentionTarget::AgentPool(pool)], false),
      None,
      20,
      OperatorAttentionVisibility::all(),
    ))
    .await
    .unwrap();
  assert_eq!(resolved.items.len(), 1);
  assert_eq!(resolved.items[0].resolved_at, Some(timestamp(30)));

  database.cleanup().await;
}

#[derive(Clone, Copy)]
enum Source {
  BuildFailed(BuildId),
  BuildSucceeded(BuildId),
  AgentUnavailable(AgentId),
  AgentPoolUnavailable(PoolId),
  Critical(&'static str),
  Audit,
  Job,
}

struct EventFixture {
  id: u128,
  source: Source,
  occurred_at: i64,
  resolved_at: Option<i64>,
  summary: &'static str,
}

struct Fixture {
  build: BuildId,
  hidden_build: BuildId,
  agent: AgentId,
  pool: PoolId,
  events: Vec<EventFixture>,
}

impl Fixture {
  fn new() -> Self {
    let build = build_id(101);
    let hidden_build = build_id(102);
    let agent = agent_id(201);
    let pool = pool_id(301);
    Self {
      build,
      hidden_build,
      agent,
      pool,
      events: vec![
        EventFixture {
          id: 1,
          source: Source::BuildFailed(build),
          occurred_at: 1_000,
          resolved_at: None,
          summary: "Build failed",
        },
        EventFixture {
          id: 2,
          source: Source::BuildFailed(hidden_build),
          occurred_at: 1_100,
          resolved_at: None,
          summary: "Hidden Build failed",
        },
        EventFixture {
          id: 3,
          source: Source::AgentUnavailable(agent),
          occurred_at: 1_000,
          resolved_at: Some(1_300),
          summary: "Agent unavailable",
        },
        EventFixture {
          id: 4,
          source: Source::AgentPoolUnavailable(pool),
          occurred_at: 900,
          resolved_at: None,
          summary: "Agent Pool unavailable",
        },
        EventFixture {
          id: 5,
          source: Source::Critical("storage_degraded"),
          occurred_at: 1_000,
          resolved_at: Some(1_200),
          summary: "Storage is degraded",
        },
        EventFixture {
          id: 6,
          source: Source::BuildSucceeded(build),
          occurred_at: 1_200,
          resolved_at: None,
          summary: "Build succeeded",
        },
        EventFixture {
          id: 7,
          source: Source::Audit,
          occurred_at: 1_300,
          resolved_at: None,
          summary: "Audit activity",
        },
        EventFixture {
          id: 8,
          source: Source::Job,
          occurred_at: 1_400,
          resolved_at: None,
          summary: "Job activity",
        },
      ],
    }
  }

  fn visible_scope(&self) -> OperatorAttentionScope {
    scope(
      [
        OperatorAttentionTarget::Build(self.build),
        OperatorAttentionTarget::Build(self.hidden_build),
        OperatorAttentionTarget::Agent(self.agent),
        OperatorAttentionTarget::AgentPool(self.pool),
      ],
      true,
    )
  }
}

async fn seed_fixture(pool: &sqlx::PgPool, memory: &InMemoryOperatorAttentionStore, fixture: &Fixture) {
  for event in &fixture.events {
    let source = source_parts(event.source);
    sqlx::query(
      "INSERT INTO operator_attention_events \
         (id, source_kind, target_kind, target_id, source_identity, code, summary, occurred_at, resolved_at) \
       VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0), \
         CASE WHEN $9::bigint IS NULL THEN NULL ELSE to_timestamp($9::double precision / 1000.0) END)",
    )
    .bind(attention_id(event.id).as_uuid())
    .bind(source.kind)
    .bind(source.target_kind)
    .bind(source.target_id)
    .bind(source.identity)
    .bind(source.code)
    .bind(event.summary)
    .bind(event.occurred_at)
    .bind(event.resolved_at)
    .execute(pool)
    .await
    .unwrap();
    memory
      .seed(
        OperatorAttentionEvent::new(
          attention_id(event.id),
          source.event_kind,
          event.summary,
          timestamp(event.occurred_at),
          event.resolved_at.map(timestamp),
        )
        .unwrap(),
      )
      .unwrap();
  }
}

struct SourceParts {
  kind: &'static str,
  target_kind: Option<&'static str>,
  target_id: Option<Uuid>,
  identity: Option<Uuid>,
  code: Option<&'static str>,
  event_kind: OperatorAttentionEventKind,
}

fn source_parts(source: Source) -> SourceParts {
  match source {
    Source::BuildFailed(id) => SourceParts {
      kind: "build_failed",
      target_kind: Some("build"),
      target_id: Some(id.as_uuid()),
      identity: None,
      code: None,
      event_kind: OperatorAttentionEventKind::BuildFailed(id),
    },
    Source::BuildSucceeded(id) => SourceParts {
      kind: "build_succeeded",
      target_kind: Some("build"),
      target_id: Some(id.as_uuid()),
      identity: None,
      code: None,
      event_kind: OperatorAttentionEventKind::BuildSucceeded(id),
    },
    Source::AgentUnavailable(id) => SourceParts {
      kind: "agent_unavailable",
      target_kind: Some("agent"),
      target_id: Some(id.as_uuid()),
      identity: None,
      code: None,
      event_kind: OperatorAttentionEventKind::AgentUnavailable(id),
    },
    Source::AgentPoolUnavailable(id) => SourceParts {
      kind: "agent_pool_unavailable",
      target_kind: Some("agent_pool"),
      target_id: Some(id.as_uuid()),
      identity: None,
      code: None,
      event_kind: OperatorAttentionEventKind::AgentPoolUnavailable(id),
    },
    Source::Critical(code) => SourceParts {
      kind: "critical_system_condition",
      target_kind: None,
      target_id: None,
      identity: Some(Uuid::from_u128(900)),
      code: Some(code),
      event_kind: OperatorAttentionEventKind::CriticalSystemCondition { code: code.to_owned() },
    },
    Source::Audit => SourceParts {
      kind: "audit_activity",
      target_kind: None,
      target_id: None,
      identity: None,
      code: None,
      event_kind: OperatorAttentionEventKind::AuditActivity,
    },
    Source::Job => SourceParts {
      kind: "job_activity",
      target_kind: None,
      target_id: None,
      identity: None,
      code: None,
      event_kind: OperatorAttentionEventKind::JobActivity,
    },
  }
}

async fn assert_parity(
  memory: &InMemoryOperatorAttentionStore,
  postgres: &PostgresStore,
  request: ListOperatorAttention,
) -> OperatorAttentionPage {
  let expected = memory.list_operator_attention(request.clone()).await.unwrap();
  let actual = postgres.list_operator_attention(request).await.unwrap();
  assert_eq!(actual, expected);
  actual
}

fn request(
  scope: OperatorAttentionScope,
  after: Option<octacity_server_store::OperatorAttentionPagePosition>,
  limit: u16,
  visibility: OperatorAttentionVisibility,
) -> ListOperatorAttention {
  ListOperatorAttention::new(scope, after, limit, visibility).unwrap()
}

fn scope<const N: usize>(targets: [OperatorAttentionTarget; N], critical: bool) -> OperatorAttentionScope {
  OperatorAttentionScope::new(BTreeSet::from(targets), critical, None, None).unwrap()
}

fn ids(page: &OperatorAttentionPage) -> Vec<OperatorAttentionId> {
  page.items.iter().map(|item| item.id).collect()
}

fn attention_id(value: u128) -> OperatorAttentionId {
  OperatorAttentionId::from_uuid(Uuid::from_u128(value)).unwrap()
}

fn build_id(value: u128) -> BuildId {
  BuildId::from_uuid(Uuid::from_u128(value)).unwrap()
}

fn agent_id(value: u128) -> AgentId {
  AgentId::from_uuid(Uuid::from_u128(value)).unwrap()
}

fn pool_id(value: u128) -> PoolId {
  PoolId::from_uuid(Uuid::from_u128(value)).unwrap()
}

fn timestamp(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

fn pool_definition(enabled: bool) -> AgentPoolDefinition {
  AgentPoolDefinition {
    enabled,
    drain_state: PoolDrainState::Accepting,
    admission_policy: PoolAdmissionPolicy::Any,
    concurrency_limit: 1,
    fairness_policy: PoolFairnessPolicy::PriorityFifo,
    static_capacity_limit: 1,
  }
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}
