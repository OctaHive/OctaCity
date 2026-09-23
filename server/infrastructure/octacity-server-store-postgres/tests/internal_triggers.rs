#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::sync::Arc;

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{Timestamp, TriggerId};
use octacity_server_store::{
  ClaimInternalTriggerEvents, CompleteInternalTriggerEvent, InternalTriggerEventStore as _,
  TriggerAcceptanceStore as _, WorkerOwner, testing::authoritative_store_contract_fixture,
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use serde_json::json;
use support::TestDatabase;
use tokio::sync::Barrier;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn terminal_build_outbox_is_claimed_once_and_restart_safe() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer())
    .accept_trigger(fixture.request.clone())
    .await
    .unwrap();
  let target = fixture.request.trigger.target;
  let internal_trigger_id = TriggerId::from_uuid(uuid::Uuid::from_u128(8_001)).unwrap();
  sqlx::query("UPDATE builds SET state = 'succeeded' WHERE id = $1")
    .bind(fixture.request.build.id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, 1, $2, $3, 'internal', TRUE, $4, to_timestamp(0))",
  )
  .bind(internal_trigger_id.as_uuid())
  .bind(target.configuration_id.as_uuid())
  .bind(i64::try_from(target.configuration_version.get()).unwrap())
  .bind(sqlx::types::Json(json!({
    "event_kind": "build.succeeded",
    "source": {"kind": "exact_revision", "value": "0123456789abcdef"},
    "parameters": {},
    "priority": 0
  })))
  .execute(&database.pool)
  .await
  .unwrap();
  let event_id = uuid::Uuid::from_u128(8_002);
  let source_job_id = fixture.request.jobs[0].id;
  sqlx::query(
    "INSERT INTO outbox_entries \
       (id, topic, aggregate_kind, aggregate_identity, payload, created_at, available_at, attempt_count) \
     VALUES ($1, 'job.completed', 'job', $2, $3, to_timestamp(1), to_timestamp(1), 0)",
  )
  .bind(event_id)
  .bind(source_job_id.to_string())
  .bind(sqlx::types::Json(
    json!({"build_state": "succeeded", "schema_version": 2}),
  ))
  .execute(&database.pool)
  .await
  .unwrap();

  let left = PostgresStore::new(independent_pool(&database.pool).await);
  let right = PostgresStore::new(independent_pool(&database.pool).await);
  let barrier = Arc::new(Barrier::new(2));
  let (left_claims, right_claims) = tokio::join!(
    claim_after_barrier(left, "server:left", barrier.clone()),
    claim_after_barrier(right, "server:right", barrier),
  );
  let mut claims = [left_claims.unwrap(), right_claims.unwrap()].concat();
  assert_eq!(claims.len(), 1);
  let claim = claims.pop().unwrap();
  assert_eq!(claim.source_build_id, fixture.request.build.id);
  assert_eq!(claim.source_occurrence.id, fixture.request.trigger.id);
  assert_eq!(claim.trigger_ancestry, [fixture.request.trigger.trigger]);
  assert_eq!(claim.matches.len(), 1);
  assert_eq!(claim.matches[0].trigger.id, internal_trigger_id);

  let store = PostgresStore::new(database.pool.clone());
  store
    .complete_internal_trigger_event(CompleteInternalTriggerEvent {
      event_identity: claim.event_identity,
      owner: claim.owner,
      completed_at: Timestamp::from_unix_millis(1_500).unwrap(),
    })
    .await
    .unwrap();
  let after_restart = store
    .claim_internal_trigger_events(
      ClaimInternalTriggerEvents::new(
        WorkerOwner::new("server:replacement").unwrap(),
        Timestamp::from_unix_millis(3_000).unwrap(),
        Timestamp::from_unix_millis(4_000).unwrap(),
        1,
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert!(after_restart.is_empty());

  database.cleanup().await;
}

async fn independent_pool(source: &sqlx::PgPool) -> sqlx::PgPool {
  sqlx::PgPool::connect_with((*source.connect_options()).clone())
    .await
    .expect("connect an independent server pool to the test database")
}

async fn claim_after_barrier(
  store: PostgresStore,
  owner: &str,
  barrier: Arc<Barrier>,
) -> Result<Vec<octacity_server_store::InternalTriggerEventClaim>, octacity_server_store::StoreError> {
  barrier.wait().await;
  store
    .claim_internal_trigger_events(ClaimInternalTriggerEvents::new(
      WorkerOwner::new(owner)?,
      Timestamp::from_unix_millis(1_000).unwrap(),
      Timestamp::from_unix_millis(2_000).unwrap(),
      1,
    )?)
    .await
}
