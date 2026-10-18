mod support;

use octacity_server_store::testing::verify_connection_publication;
use octacity_server_store_postgres::PostgresStore;
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn connection_publication_retains_immutable_versions_and_atomic_replay() {
  let database = TestDatabase::migrated().await;
  verify_connection_publication(&PostgresStore::new(database.pool.clone())).await;
  let counts: (i64, i64, i64, i64) = sqlx::query_as(
    "SELECT (SELECT count(*) FROM integration_connection_versions), \
     (SELECT count(*) FROM idempotency_records WHERE scope = 'publish-integration-connection'), \
     (SELECT count(*) FROM audit_facts WHERE operation = 'publish-integration-connection'), \
     (SELECT count(*) FROM outbox_entries WHERE topic = 'integration.connection.published')",
  )
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert_eq!(counts, (2, 2, 2, 2));
  database.cleanup().await;
}

use octacity_server_domain::{IntegrationId, Timestamp};
use octacity_server_store::{
  IdempotencyKey, IntegrationConnectionIntent, IntegrationConnectionStore, MutationDisposition,
  PublishIntegrationConnection,
  testing::{integration_connection_fixture, management_mutation},
};

fn request() -> PublishIntegrationConnection {
  let (installed, draft) = integration_connection_fixture("custom-tracker");
  PublishIntegrationConnection {
    intent: IntegrationConnectionIntent {
      id: IntegrationId::generate(),
      expected_current_version: None,
      draft,
    },
    installed,
    idempotency_key: IdempotencyKey::new("create-connection").unwrap(),
    published_at: Timestamp::from_unix_millis(1).unwrap(),
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn connection_concurrent_retry_and_replacement_commit_one_receipt() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let request = request();
  let (left, right) = tokio::join!(
    store.publish_integration_connection(management_mutation(request.clone())),
    store.publish_integration_connection(management_mutation(request.clone())),
  );
  let (left, right) = (left.unwrap(), right.unwrap());
  assert_eq!(left.connection, right.connection);
  assert_eq!(
    [left.disposition, right.disposition]
      .into_iter()
      .filter(|d| *d == MutationDisposition::Applied)
      .count(),
    1
  );
  let mut next = request;
  next.intent.expected_current_version = Some(left.connection.version);
  next.idempotency_key = IdempotencyKey::new("replace-left").unwrap();
  let mut competing = next.clone();
  competing.idempotency_key = IdempotencyKey::new("replace-right").unwrap();
  competing.intent.draft.name = octacity_server_domain::IntegrationName::new("Competing update").unwrap();
  let (left, right) = tokio::join!(
    store.publish_integration_connection(management_mutation(next.clone())),
    store.publish_integration_connection(management_mutation(competing)),
  );
  assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
  let winner = left.or(right).unwrap().connection;
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted.integration_connection(next.intent.id, None).await.unwrap(),
    winner
  );
  assert_eq!(winner.version.get(), 2);
  let counts: (i64, i64, i64, i64) = sqlx::query_as(
    "SELECT (SELECT count(*) FROM integration_connection_versions), \
     (SELECT count(*) FROM idempotency_records), (SELECT count(*) FROM audit_facts), \
     (SELECT count(*) FROM outbox_entries)",
  )
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert_eq!(counts, (2, 2, 2, 2));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn connection_outbox_failure_rolls_back_metadata_replay_and_audit_together() {
  let database = TestDatabase::migrated().await;
  sqlx::raw_sql(
    "CREATE FUNCTION reject_connection_event() RETURNS trigger LANGUAGE plpgsql AS $$ \
    BEGIN RAISE EXCEPTION 'test outbox failure'; END $$; \
    CREATE TRIGGER reject_connection_event BEFORE INSERT ON outbox_entries \
    FOR EACH ROW EXECUTE FUNCTION reject_connection_event()",
  )
  .execute(&database.pool)
  .await
  .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let request = request();
  assert!(
    store
      .publish_integration_connection(management_mutation(request.clone()))
      .await
      .is_err()
  );
  assert!(store.integration_connection(request.intent.id, None).await.is_err());
  let counts: (i64, i64, i64, i64) = sqlx::query_as(
    "SELECT (SELECT count(*) FROM integration_connections), \
     (SELECT count(*) FROM integration_connection_versions), \
     (SELECT count(*) FROM idempotency_records), (SELECT count(*) FROM audit_facts)",
  )
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert_eq!(counts, (0, 0, 0, 0));
  sqlx::query("DROP TRIGGER reject_connection_event ON outbox_entries")
    .execute(&database.pool)
    .await
    .unwrap();
  assert_eq!(
    store
      .publish_integration_connection(management_mutation(request))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Applied
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn connection_read_rejects_metadata_inconsistent_with_its_immutable_digest() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let request = request();
  store
    .publish_integration_connection(management_mutation(request.clone()))
    .await
    .unwrap();
  sqlx::query("UPDATE integration_connection_versions SET document = jsonb_set(document, '{definition,endpoint}', '\"https://substituted.invalid/\"'::jsonb) WHERE connection_id = $1")
    .bind(request.intent.id.as_uuid()).execute(&database.pool).await.unwrap();
  assert!(matches!(
    store.integration_connection(request.intent.id, None).await,
    Err(octacity_server_store::StoreError::Unavailable)
  ));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn connection_list_filters_before_pagination_and_retains_current_metadata() {
  use octacity_server_store::{IntegrationConnectionListVisibility, ReadIntegrationConnections};
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let mut ids = vec![];
  for index in 1..=3 {
    let mut request = request();
    request.intent.id = format!("00000000-0000-7000-8000-{index:012}").parse().unwrap();
    request.idempotency_key = IdempotencyKey::new(format!("list-{index}")).unwrap();
    ids.push(
      store
        .publish_integration_connection(management_mutation(request))
        .await
        .unwrap()
        .connection
        .id,
    );
  }
  let visibility = IntegrationConnectionListVisibility::restricted([ids[0], ids[2]]).unwrap();
  let first = store
    .list_integration_connections(ReadIntegrationConnections {
      visibility: visibility.clone(),
      after: None,
      limit: 1,
    })
    .await
    .unwrap();
  assert_eq!(first.items.iter().map(|c| c.id).collect::<Vec<_>>(), vec![ids[0]]);
  assert_eq!(first.next_cursor, Some(ids[0]));
  let last = store
    .list_integration_connections(ReadIntegrationConnections {
      visibility,
      after: first.next_cursor,
      limit: 1,
    })
    .await
    .unwrap();
  assert_eq!(last.items.iter().map(|c| c.id).collect::<Vec<_>>(), vec![ids[2]]);
  assert_eq!(last.next_cursor, None);
  database.cleanup().await;
}
