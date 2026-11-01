//! Storage bounds must not rewrite the immutable Run or its historical NULL pins.
use super::*;
use octacity_server_store::FactoryRunStore as _;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn retained_code_pins_reject_oversized_metadata_without_changing_the_run() {
  let database = TestDatabase::empty().await;
  let previous = sqlx::migrate::Migrator::with_migrations(
    octacity_server_store_postgres::MIGRATOR
      .iter()
      .filter(|migration| migration.version < 112)
      .cloned()
      .collect(),
  );
  octacity_server_store_postgres::run_migrator(&database.pool, &previous)
    .await
    .unwrap();
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let (store, _) = seed_projection_admission(&database, &fixture.admission).await;
  let run = fixture.admission.run.id();
  let original = store.factory_run_snapshot(run).await.unwrap();
  for _ in 0..2 {
    octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  }
  // The same 16 MiB storage ceiling as the containing Flow publication leaves
  // ample room for all 64 definitions' 64 bounded exact source references.
  for pins in [
    serde_json::json!(["x".repeat(16 * 1024 * 1024 + 1)]),
    serde_json::json!({}),
  ] {
    let error = sqlx::query("UPDATE factory_runs SET flow_code_pins=$1 WHERE id=$2")
      .bind(Json(pins))
      .bind(run.as_uuid())
      .execute(&database.pool)
      .await
      .unwrap_err();
    assert_eq!(
      error.as_database_error().and_then(|error| error.code()).as_deref(),
      Some("23514")
    );
  }
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(restarted.factory_run_snapshot(run).await.unwrap(), original);
  let pins: Option<Json<Value>> = sqlx::query_scalar("SELECT flow_code_pins FROM factory_runs WHERE id=$1")
    .bind(run.as_uuid())
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert!(
    pins.is_none(),
    "historical NULL remains distinct from an explicit empty source catalogue"
  );
  database.cleanup().await;
}
