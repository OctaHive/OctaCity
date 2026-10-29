//! The shared read-only catalog is installed in the released management composition.
use serde_json::json;
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres;
#[allow(dead_code)]
#[path = "support/runtime.rs"]
mod runtime;

#[tokio::test]
#[ignore = "requires explicitly configured disposable PostgreSQL"]
async fn released_management_exposes_presets_before_factory_execution_is_installed() {
  let database = postgres::TestDatabase::migrated().await;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let directory = tempfile::tempdir().unwrap();
  let server = octacity_server::ServerRuntime::start(runtime::runtime_config(
    directory.path(),
    url.as_str(),
    "http://127.0.0.1:1",
  ))
  .await
  .unwrap();
  let origin = format!("http://{}", server.management_addr());
  let client = reqwest::Client::new();
  let response = client
    .post(format!("{origin}/api/v1/projects"))
    .header("idempotency-key", "preset-project")
    .json(&json!({"parent_id":null,"name":"preset setup"}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::CREATED,
    "{}",
    response.text().await.unwrap()
  );
  let project = response.json::<serde_json::Value>().await.unwrap()["resource"]["id"]
    .as_str()
    .unwrap()
    .to_owned();
  let path = format!("{origin}/api/v1/projects/{project}/factory-presets");
  let response = client.get(&path).send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let page = response.json::<serde_json::Value>().await.unwrap();
  assert_eq!(page["items"][0]["reference"]["identity"], "triage");
  let response = client
    .post(format!("{path}/resolve"))
    .json(&json!({"preset":page["items"][0]["reference"]}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let draft = response.json::<serde_json::Value>().await.unwrap();
  assert!(!draft["diagnostics"].as_array().unwrap().is_empty());
  assert!(draft["flow"].is_null());
  assert!(draft["configuration"].is_null());
  server.shutdown().await.unwrap();
  database.cleanup().await;
}
