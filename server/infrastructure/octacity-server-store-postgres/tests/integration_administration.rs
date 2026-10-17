mod support;
use octacity_server_store::testing::{verify_integration_administration, verify_integration_preparation_cleanup};
use octacity_server_store_postgres::PostgresStore;
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn administration_retains_recovery_replay_and_disabled_dependencies() {
  let database = TestDatabase::migrated().await;
  verify_integration_administration(&PostgresStore::new(database.pool.clone())).await;
  let evidence: (i64, i64) = sqlx::query_as(
    "SELECT \
    (SELECT count(*) FROM audit_facts WHERE operation = 'begin-integration-administration'), \
    (SELECT count(*) FROM outbox_entries WHERE topic = 'integration.administration.completed')",
  )
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert_eq!(evidence, (5, 1));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn administration_cleanup_is_bounded_and_recoverable() {
  let database = TestDatabase::migrated().await;
  verify_integration_preparation_cleanup(&PostgresStore::new(database.pool.clone())).await;
  let actors: Vec<(String, String, String, i64)> = sqlx::query_as(
    "SELECT actor_kind, actor_identity, request_identity, (extract(epoch FROM occurred_at) * 1000)::bigint FROM audit_facts WHERE actor_identity = 'cleanup-operator'",
  ).fetch_all(&database.pool).await.unwrap();
  assert_eq!(
    actors,
    vec![(
      "authenticated_management".into(),
      "cleanup-operator".into(),
      "cleanup-first-pass".into(),
      1000
    )],
    "expiry must retain its initiating actor exactly once despite retries"
  );
  let events: i64 =
    sqlx::query_scalar("SELECT count(*) FROM outbox_entries WHERE topic = 'integration.preparation.abandoned'")
      .fetch_one(&database.pool)
      .await
      .unwrap();
  assert_eq!(events, 2, "each expired preparation produces one transactional event");
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn administration_inspection_uses_read_only_database_authority() {
  use octacity_server_store::IntegrationAdministrationStore;
  let database = TestDatabase::migrated().await;
  verify_integration_administration(&PostgresStore::new(database.pool.clone())).await;
  let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM integration_connections")
    .fetch_one(&database.pool)
    .await
    .unwrap();
  let reader = sqlx::postgres::PgPoolOptions::new()
    .after_connect(|connection, _| {
      Box::pin(async move {
        sqlx::query("SET default_transaction_read_only = on")
          .execute(connection)
          .await?;
        Ok(())
      })
    })
    .connect_with(database.pool.connect_options().as_ref().clone())
    .await
    .unwrap();
  let state = PostgresStore::new(reader.clone())
    .integration_administration(id.to_string().parse().unwrap())
    .await
    .unwrap();
  assert_eq!(state.revision, 5);
  assert!(!state.enabled);
  reader.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn referenced_connection_versions_survive_replacement_and_cannot_be_collected() {
  let database = TestDatabase::migrated().await;
  verify_integration_administration(&PostgresStore::new(database.pool.clone())).await;
  let deletion = sqlx::query("DELETE FROM integration_connection_versions WHERE version = 1")
    .execute(&database.pool)
    .await;
  assert!(
    deletion.is_err(),
    "retained administrative consumers must protect historical connection versions"
  );
  assert_eq!(
    deletion
      .unwrap_err()
      .as_database_error()
      .and_then(|error| error.code())
      .as_deref(),
    Some("23503")
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn completion_failure_rolls_back_activation_state_and_recovery_commits_once() {
  use octacity_server_domain::{IntegrationId, IntegrationVersion, Timestamp};
  use octacity_server_store::testing::{integration_connection_fixture, management_mutation};
  use octacity_server_store::*;
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (installed, draft) = integration_connection_fixture("custom-tracker");
  let reference = installed.resources[0].capabilities[0].credential.clone();
  let id = IntegrationId::generate();
  store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id,
        expected_current_version: None,
        draft,
      },
      installed,
      idempotency_key: IdempotencyKey::new("atomic-metadata").unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    }))
    .await
    .unwrap();
  let request = BeginIntegrationAdministration {
    intent: serde_json::from_value(
      serde_json::json!({"id":id,"expected_metadata_version":IntegrationVersion::INITIAL,
    "expected_revision":0,"action":{"operation":"credential","reference":reference,"kind":"provision","activate_connection":true}}),
    )
    .unwrap(),
    idempotency_key: IdempotencyKey::new("atomic-credential").unwrap(),
    requested_at: Timestamp::from_unix_millis(10).unwrap(),
  };
  let (left, right) = tokio::join!(
    store.begin_integration_administration(management_mutation(request.clone())),
    store.begin_integration_administration(management_mutation(request.clone()))
  );
  let pending = left.unwrap();
  assert_eq!(pending, right.unwrap());
  sqlx::raw_sql("CREATE FUNCTION reject_completion_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
    IF NEW.topic = 'integration.administration.completed' THEN RAISE EXCEPTION 'forced completion failure'; END IF; RETURN NEW; END $$; \
    CREATE TRIGGER reject_completion BEFORE INSERT ON outbox_entries FOR EACH ROW EXECUTE FUNCTION reject_completion_outbox();")
    .execute(&database.pool).await.unwrap();
  assert!(
    store
      .complete_integration_administration(management_mutation(pending.clone()))
      .await
      .is_err()
  );
  let recovered = PostgresStore::new(database.pool.clone());
  assert_eq!(
    recovered
      .begin_integration_administration(management_mutation(request.clone()))
      .await
      .unwrap(),
    pending
  );
  let state = recovered.integration_administration(id).await.unwrap();
  assert!(!state.enabled);
  assert_eq!(state.pending, pending.publication);
  sqlx::raw_sql("DROP TRIGGER reject_completion ON outbox_entries; DROP FUNCTION reject_completion_outbox();")
    .execute(&database.pool)
    .await
    .unwrap();
  let completed = recovered
    .complete_integration_administration(management_mutation(pending))
    .await
    .unwrap();
  assert_eq!(completed.status, IntegrationAdministrationStatus::Completed);
  assert!(recovered.integration_administration(id).await.unwrap().enabled);
  assert_eq!(
    recovered
      .begin_integration_administration(management_mutation(request))
      .await
      .unwrap(),
    completed
  );
  let evidence: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM idempotency_records), (SELECT count(*) FROM audit_facts), (SELECT count(*) FROM outbox_entries)")
    .fetch_one(&database.pool).await.unwrap();
  assert_eq!(evidence, (3, 3, 3));
  database.cleanup().await;
}
