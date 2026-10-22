//! A real browser speaks the released HTTP contracts backed by disposable PostgreSQL.
use octacity_server::ServerRuntime;
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres;
#[path = "support/runtime.rs"]
mod runtime;

#[path = "support/console_pilot.rs"]
mod console_pilot;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and the locked Console/browser dependencies"]
async fn manual_console_pilot_uses_postgres_http_without_factory_or_provider() {
  let database = postgres::TestDatabase::migrated().await;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let server = ServerRuntime::start(runtime::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"))
    .await
    .unwrap();
  let result = console_pilot::run(&format!("http://{}", server.management_addr()), None).await;
  server.shutdown().await.unwrap();
  database.cleanup().await;
  assert!(
    result.status.success(),
    "{}\n{}",
    String::from_utf8_lossy(&result.stdout),
    String::from_utf8_lossy(&result.stderr)
  );
}
