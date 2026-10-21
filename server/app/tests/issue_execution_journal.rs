//! Production journal reads retain the original request across restart without a provider.
use octacity_server::ServerRuntime;
use octacity_server_domain::*;
use octacity_server_factory::{FactoryConfigurationId, FactoryConfigurationVersion};
use octacity_server_store::{testing::management_mutation, *};
use octacity_server_store_postgres::PostgresStore;
use serde_json::{Value, json};

#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres;
#[path = "support/runtime.rs"]
mod runtime;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn journal_runtime_reads_original_pending_request_after_restart_without_provider() {
  let database = postgres::TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project_id = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id: project_id,
      parent_id: None,
      name: ProjectName::new("Manual journal").unwrap(),
      idempotency_key: "project".parse().unwrap(),
      created_at: Timestamp::from_unix_millis(1).unwrap(),
    }))
    .await
    .unwrap();
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let mut server = ServerRuntime::start(runtime::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"))
    .await
    .unwrap();
  let client = reqwest::Client::new();
  let created = client
    .post(format!(
      "http://{}/api/v1/projects/{project_id}/issues",
      server.management_addr()
    ))
    .header("idempotency-key", "draft")
    .json(&json!({"title":"Manual draft","description":"Reviewed task","acceptance":"Reviewed acceptance"}))
    .send()
    .await
    .unwrap();
  assert_eq!(created.status(), reqwest::StatusCode::CREATED);
  let created: Value = created.json().await.unwrap();
  let issue_id = created["resource"]["id"].as_str().unwrap().parse().unwrap();
  let issue = store.issue(project_id, issue_id, None).await.unwrap();
  let request_id = IssueExecutionRequestId::generate();
  // Reserve through the existing durable port, leaving publication pending. Reading
  // must neither publish inputs nor require a configured Factory or provider.
  store
    .request_issue_execution(management_mutation(RequestIssueExecution {
      id: request_id,
      intent: RequestIssueExecutionIntent {
        classification: None,
        project_id,
        issue_id,
        expected_version: issue.version,
        target: IssueExecutionTarget {
          configuration_id: FactoryConfigurationId::generate(),
          configuration_version: FactoryConfigurationVersion::INITIAL,
          repository_id: RepositoryId::generate(),
          repository_version: RepositoryVersion::INITIAL,
          source: RepositorySourceSelection::DefaultReference,
          mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
        },
      },
      idempotency_key: "execute".parse().unwrap(),
      requested_at: issue.updated_at,
    }))
    .await
    .unwrap();
  let path = format!("/api/v1/projects/{project_id}/issues/{issue_id}/executions/{request_id}/journal");
  let first = client
    .get(format!("http://{}{path}", server.management_addr()))
    .send()
    .await
    .unwrap();
  assert_eq!(first.status(), reqwest::StatusCode::OK);
  assert_eq!(first.headers()["cache-control"], "no-store");
  let first: Value = first.json().await.unwrap();
  assert_eq!(first["execution"]["request"]["id"], request_id.to_string());
  assert_eq!(first["execution"]["run"]["availability"], "preparing");
  assert!(first["records"].is_null());
  assert!(first["execution"]["request"]["association"].is_null());
  server.shutdown().await.unwrap();
  server = ServerRuntime::start(runtime::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"))
    .await
    .unwrap();
  let replay = client
    .get(format!("http://{}{path}", server.management_addr()))
    .send()
    .await
    .unwrap();
  assert_eq!(replay.status(), reqwest::StatusCode::OK);
  assert_eq!(replay.headers()["cache-control"], "no-store");
  assert_eq!(replay.json::<Value>().await.unwrap(), first);
  assert_eq!(store.issue(project_id, issue_id, None).await.unwrap(), issue);
  server.shutdown().await.unwrap();
  database.cleanup().await;
}
