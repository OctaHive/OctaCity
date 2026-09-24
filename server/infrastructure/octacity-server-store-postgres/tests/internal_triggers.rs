#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::sync::Arc;

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{Timestamp, TriggerId, TriggerVersion};
use octacity_server_store::{
  ClaimInternalTriggerEvents, CompleteInternalTriggerEvent, CreateInternalTriggerDefinition, CreateTriggerDefinition,
  IdempotencyKey, InternalTriggerDefinitionStore as _, InternalTriggerEventStore as _, ListInternalTriggerDefinitions,
  PublishInternalTriggerVersion, TriggerAcceptanceStore as _, TriggerKind, WorkerOwner,
  testing::authoritative_store_contract_fixture,
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
    "upstream": {
      "configuration_id": target.configuration_id,
      "configuration_version": target.configuration_version
    },
    "event_kind": "build.succeeded",
    "source": {
      "kind": "resolve_target",
      "source": {"kind": "exact_revision", "value": "0123456789abcdef"}
    },
    "parameters": {},
    "priority": 0
  })))
  .execute(&database.pool)
  .await
  .unwrap();
  let fan_out_trigger_id = TriggerId::from_uuid(uuid::Uuid::from_u128(8_005)).unwrap();
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     SELECT $1, 1, build_configuration_id, build_configuration_version, kind, TRUE, definition, to_timestamp(0) \
     FROM triggers WHERE id = $2 AND version = 1",
  )
  .bind(fan_out_trigger_id.as_uuid())
  .bind(internal_trigger_id.as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();
  let unrelated_trigger_id = TriggerId::from_uuid(uuid::Uuid::from_u128(8_003)).unwrap();
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, 1, $2, $3, 'internal', TRUE, $4, to_timestamp(0))",
  )
  .bind(unrelated_trigger_id.as_uuid())
  .bind(target.configuration_id.as_uuid())
  .bind(i64::try_from(target.configuration_version.get()).unwrap())
  .bind(sqlx::types::Json(json!({
    "upstream": {
      "configuration_id": uuid::Uuid::from_u128(99_999).to_string(),
      "configuration_version": 1
    },
    "event_kind": "build.succeeded",
    "source": {"kind": "inherit_revision"},
    "parameters": {},
    "priority": 0
  })))
  .execute(&database.pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     SELECT id, 2, build_configuration_id, build_configuration_version, kind, FALSE, definition, to_timestamp(2) \
     FROM triggers WHERE id = ANY($1) AND version = 1",
  )
  .bind([internal_trigger_id.as_uuid(), fan_out_trigger_id.as_uuid()].as_slice())
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
  assert_eq!(claim.source_revision, fixture.request.build.immutable_revision);
  assert_eq!(claim.source_target, target);
  assert_eq!(claim.source_occurrence.id, fixture.request.trigger.id);
  assert_eq!(claim.trigger_ancestry, [fixture.request.trigger.trigger]);
  assert_eq!(claim.matches.len(), 2);
  assert_eq!(
    claim
      .matches
      .iter()
      .map(|matched| matched.trigger.id)
      .collect::<Vec<_>>(),
    [internal_trigger_id, fan_out_trigger_id]
  );

  let store = PostgresStore::new(database.pool.clone());
  store
    .complete_internal_trigger_event(CompleteInternalTriggerEvent {
      event_identity: claim.event_identity,
      owner: claim.owner,
      completed_at: Timestamp::from_unix_millis(1_500).unwrap(),
    })
    .await
    .unwrap();
  let later_event_id = uuid::Uuid::from_u128(8_004);
  sqlx::query(
    "INSERT INTO outbox_entries \
       (id, topic, aggregate_kind, aggregate_identity, payload, created_at, available_at, attempt_count) \
     VALUES ($1, 'job.completed', 'job', $2, $3, to_timestamp(3), to_timestamp(3), 0)",
  )
  .bind(later_event_id)
  .bind(source_job_id.to_string())
  .bind(sqlx::types::Json(
    json!({"build_state": "succeeded", "schema_version": 2}),
  ))
  .execute(&database.pool)
  .await
  .unwrap();
  let mut after_replacement = store
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
  assert_eq!(after_replacement.len(), 1);
  let later = after_replacement.pop().unwrap();
  assert!(later.matches.is_empty());
  store
    .complete_internal_trigger_event(CompleteInternalTriggerEvent {
      event_identity: later.event_identity,
      owner: later.owner,
      completed_at: Timestamp::from_unix_millis(3_500).unwrap(),
    })
    .await
    .unwrap();
  let after_restart = store
    .claim_internal_trigger_events(
      ClaimInternalTriggerEvents::new(
        WorkerOwner::new("server:restart").unwrap(),
        Timestamp::from_unix_millis(5_000).unwrap(),
        Timestamp::from_unix_millis(6_000).unwrap(),
        1,
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert!(after_restart.is_empty());

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn internal_trigger_versions_are_replay_safe_queryable_and_disableable() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let target = fixture.request.trigger.target;
  let definition = json!({
    "upstream": {
      "configuration_id": target.configuration_id,
      "configuration_version": target.configuration_version
    },
    "event_kind": "build.succeeded",
    "source": {"kind": "inherit_revision"},
    "parameters": {},
    "priority": 0
  });
  let trigger_id = TriggerId::from_uuid(uuid::Uuid::from_u128(8_100)).unwrap();
  let create = |id| CreateInternalTriggerDefinition {
    trigger: CreateTriggerDefinition {
      id,
      version: TriggerVersion::INITIAL,
      configuration_id: target.configuration_id,
      configuration_version: target.configuration_version,
      kind: TriggerKind::Internal,
      enabled: true,
      definition: definition.clone(),
      idempotency_key: IdempotencyKey::new("internal:create").unwrap(),
      created_at: Timestamp::from_unix_millis(1_000).unwrap(),
    },
  };
  let created = store
    .create_internal_trigger_definition(create(trigger_id))
    .await
    .unwrap();
  let replayed = store
    .create_internal_trigger_definition(create(TriggerId::from_uuid(uuid::Uuid::from_u128(8_101)).unwrap()))
    .await
    .unwrap();
  assert_eq!(replayed.trigger_id, created.trigger_id);
  assert_eq!(
    replayed.disposition,
    octacity_server_store::MutationDisposition::Replayed
  );

  let published = store
    .publish_internal_trigger_version(PublishInternalTriggerVersion {
      id: trigger_id,
      expected_current_version: TriggerVersion::INITIAL,
      target,
      enabled: false,
      definition,
      idempotency_key: IdempotencyKey::new("internal:disable").unwrap(),
      published_at: Timestamp::from_unix_millis(2_000).unwrap(),
    })
    .await
    .unwrap();
  assert_eq!(published.version, TriggerVersion::new(2).unwrap());
  let exact = store
    .internal_trigger_definition(trigger_id, published.version)
    .await
    .unwrap();
  assert!(!exact.enabled);

  let page = store
    .list_internal_trigger_definitions(ListInternalTriggerDefinitions::new(None, 10).unwrap())
    .await
    .unwrap();
  assert_eq!(page.items.len(), 1);
  assert_eq!(page.items[0].trigger.version, published.version);
  assert!(!page.items[0].enabled);

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
