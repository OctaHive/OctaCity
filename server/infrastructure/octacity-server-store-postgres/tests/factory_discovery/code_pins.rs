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
  let admission = &fixture.admission;
  let (store, repository) = seed_projection_configuration(&database, admission).await;
  // Current admission/read ports require the latest schema. Construct the
  // historical row with the same frozen writer as the other upgrade contracts.
  support::legacy_factory_admission::seed(
    &database.pool,
    AdmitFactoryWork {
      incoming: None,
      probe: FactoryAdmissionProbe {
        source_scope: FactoryWorkSourceScope {
          source: key("manual"),
          security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
        },
        external_identity: admission.work.external_identity().clone(),
        intent_digest: digest(1),
        idempotency_key: idempotency("legacy-code-pins-admission"),
      },
      repository_version: Some(repository.version),
      work: admission.work.clone(),
      run: admission.run.clone(),
      flow: admission.flow.clone(),
      admitted_at: time(1),
    },
  )
  .await;
  let run = admission.run.id();
  for _ in 0..2 {
    octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  }
  let original = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(original.work, admission.work);
  assert_eq!(original.run, admission.run);
  assert_eq!(original.admitted_flow, admission.flow);
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
