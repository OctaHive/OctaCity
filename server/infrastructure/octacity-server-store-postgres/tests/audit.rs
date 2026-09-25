mod support;

use octacity_server_domain::AuditFactId;
use octacity_server_store::{AuditActorKind, AuditFactQuery, AuditFactStore};
use octacity_server_store_postgres::PostgresStore;
use serde_json::json;
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn audit_reads_filter_and_paginate_immutable_facts() {
  let database = TestDatabase::migrated().await;
  seed(&database.pool).await;
  let store = PostgresStore::new(database.pool.clone());

  let first = store.list_audit_facts(query(2)).await.unwrap();
  assert_eq!(first.items.len(), 2);
  assert_eq!(first.items[0].actor.kind, AuditActorKind::Agent);
  assert_eq!(first.items[1].actor.kind, AuditActorKind::UnauthenticatedManagement);
  assert!(first.items[1].actor.identity.is_none());
  assert_eq!(first.items[1].request_identity.as_deref(), Some("request-2"));
  assert_eq!(first.items[1].metadata.as_value(), &json!({"cancelled_job_count": 2}));

  let cursor = first.next_cursor.unwrap();
  let second = store
    .list_audit_facts(AuditFactQuery {
      after: Some(cursor),
      ..query(2)
    })
    .await
    .unwrap();
  assert_eq!(second.items.len(), 1);
  assert!(second.next_cursor.is_none());

  let management = store
    .list_audit_facts(AuditFactQuery {
      actor_kind: Some(AuditActorKind::UnauthenticatedManagement),
      limit: 10,
      ..query(10)
    })
    .await
    .unwrap();
  assert_eq!(management.items.len(), 1);
  assert_eq!(management.items[0].operation, "cancel-build");

  database.cleanup().await;
}

fn query(limit: u16) -> AuditFactQuery {
  AuditFactQuery {
    actor_kind: None,
    actor_identity: None,
    operation: None,
    target_kind: None,
    target_identity: None,
    request_identity: None,
    occurred_from: None,
    occurred_through: None,
    after: None,
    limit,
  }
}

async fn seed(pool: &sqlx::PgPool) {
  for (id, actor_kind, actor_identity, operation, request_identity, metadata, occurred_at) in [
    (
      1_u128,
      "worker",
      Some("retention-worker"),
      "finish-retention-pass",
      "worker-1",
      json!({"phase": "completed"}),
      100_i64,
    ),
    (
      2,
      "unauthenticated_management",
      None,
      "cancel-build",
      "request-2",
      json!({"cancelled_job_count": 2}),
      200,
    ),
    (
      3,
      "agent",
      Some("agent-3"),
      "complete-job",
      "agent-3:completion-1",
      json!({"ready_job_count": 1}),
      300,
    ),
  ] {
    let id = AuditFactId::from_uuid(uuid::Uuid::from_u128(id)).unwrap();
    sqlx::query(
      "INSERT INTO audit_facts \
         (id, actor_kind, actor_identity, operation, target_kind, target_identity, request_identity, \
          idempotency_key, outcome, safe_metadata, occurred_at) \
       VALUES ($1, $2, $3, $4, 'test_target', $5, $6, $6, 'accepted', $7, \
         to_timestamp($8::double precision / 1000.0))",
    )
    .bind(id.as_uuid())
    .bind(actor_kind)
    .bind(actor_identity)
    .bind(operation)
    .bind(format!("target-{id}"))
    .bind(request_identity)
    .bind(sqlx::types::Json(metadata))
    .bind(occurred_at)
    .execute(pool)
    .await
    .unwrap();
  }
}
