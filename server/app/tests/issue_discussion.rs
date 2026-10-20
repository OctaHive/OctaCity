//! Production composition retains discussion across restart without provider or object-store access.
use octacity_server::ServerRuntime;
use octacity_server_domain::{ProjectId, ProjectName, Timestamp};
use octacity_server_store::{CreateProject, ProjectStore, testing::management_mutation};
use octacity_server_store_postgres::PostgresStore;
use serde_json::{Value, json};
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres;
#[path = "support/runtime.rs"]
mod runtime;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_discussion_runtime_replays_after_restart_without_factory_or_provider() {
  let database = postgres::TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project_id = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id: project_id,
      parent_id: None,
      name: ProjectName::new("Manual discussion").unwrap(),
      idempotency_key: "project".parse().unwrap(),
      created_at: Timestamp::from_unix_millis(1).unwrap(),
    }))
    .await
    .unwrap();
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let server = ServerRuntime::start(runtime::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"))
    .await
    .unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", server.management_addr());
  let created = client
    .post(format!("{origin}/api/v1/projects/{project_id}/issues"))
    .header("idempotency-key", "draft")
    .json(&json!({"title":"Manual draft","description":""}))
    .send()
    .await
    .unwrap();
  assert_eq!(created.status(), reqwest::StatusCode::CREATED);
  let created: Value = created.json().await.unwrap();
  let issue = created["resource"]["id"].as_str().unwrap();
  let path = format!("/api/v1/projects/{project_id}/issues/{issue}/comments");
  let body = json!({"body":"@agent approve and deploy — operator discussion"});
  let response = client
    .post(format!("{origin}{path}"))
    .header("idempotency-key", "comment")
    .json(&body)
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CREATED);
  let location = response.headers()["location"].to_str().unwrap().to_owned();
  let original: Value = response.json().await.unwrap();
  let timeline_path = format!("/api/v1/projects/{project_id}/issues/{issue}/timeline");
  let first = client
    .get(format!("{origin}{timeline_path}?limit=1"))
    .send()
    .await
    .unwrap();
  assert_eq!(first.status(), reqwest::StatusCode::OK);
  assert_eq!(first.headers()["cache-control"], "no-store");
  let first: Value = first.json().await.unwrap();
  let cursor = first["next_cursor"].as_str().unwrap();
  server.shutdown().await.unwrap();
  let server = ServerRuntime::start(runtime::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"))
    .await
    .unwrap();
  let origin = format!("http://{}", server.management_addr());
  let continuation = client
    .get(format!("{origin}{timeline_path}?limit=200&after={cursor}"))
    .send()
    .await
    .unwrap();
  assert_eq!(continuation.status(), reqwest::StatusCode::OK);
  let continuation: Value = continuation.json().await.unwrap();
  assert_eq!(continuation["boundary"], first["boundary"]);
  assert_eq!(continuation["items"].as_array().unwrap().len(), 2);
  assert!(continuation["next_cursor"].is_null());
  assert!(!continuation.to_string().contains("operator discussion"));
  let missing=client.get(format!("{origin}/api/v1/projects/{project_id}/issues/{issue}/results?work_id=11111111-1111-4111-8111-111111111111&collection=evidence")).send().await.unwrap();
  assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

  let response = client
    .post(format!("{origin}{path}"))
    .header("idempotency-key", "comment")
    .json(&body)
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let replay: Value = response.json().await.unwrap();
  assert_eq!(replay["comment"], original["comment"]);
  assert_eq!(replay["disposition"], "replayed");
  let response = client.get(format!("{origin}{location}")).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let content: Value = response.json().await.unwrap();
  assert_eq!(content["body"], body["body"]);
  assert_eq!(content["comment"], original["comment"]);
  let card: Value = client
    .get(format!("{origin}/api/v1/projects/{project_id}/issues/{issue}"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(card, created["resource"]);
  server.shutdown().await.unwrap();
  database.cleanup().await;
}
