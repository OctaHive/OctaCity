//! Real released HTTP composition, with disposable PostgreSQL and no Factory/provider.
use super::*;
use super::{postgres_test_support as postgres, test_runtime_config as config};
use serde_json::{Value, json};
async fn accepted(response: reqwest::Response, status: reqwest::StatusCode) -> Value {
  assert_eq!(response.status(), status);
  response.json().await.unwrap()
}
#[tokio::test]
#[ignore = "requires OCTACITY_POSTGRES_URL pointing to disposable PostgreSQL"]
async fn runtime_issue_journey_and_original_replay_survive_restart_without_factory_or_providers() {
  let database = postgres::TestDatabase::migrated().await;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let settings = config::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9");
  let runtime = ServerRuntime::start(settings.clone()).await.unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());
  let project = accepted(
    client
      .post(format!("{origin}/api/v1/projects"))
      .header("idempotency-key", "project")
      .json(&json!({"name":"Manual tasks"}))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::CREATED,
  )
  .await;
  let project = project["resource"]["id"].as_str().unwrap();
  let collection = format!("/api/v1/projects/{project}/issues");
  let draft = json!({"title":"Manual draft","description":"","acceptance":null,"priority":null});
  let response = client
    .post(format!("{origin}{collection}"))
    .header("idempotency-key", "draft")
    .json(&draft)
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.headers().get("cache-control").and_then(|v| v.to_str().ok()),
    Some("no-store")
  );
  let created = accepted(response, reqwest::StatusCode::CREATED).await;
  assert_eq!(created["resource"]["display_key"], "ISS-1");
  let path = format!("{collection}/{}", created["resource"]["id"].as_str().unwrap());
  let sync = accepted(
    client
      .get(format!("{origin}{path}/synchronization"))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert!(sync["link"].is_null());
  assert!(sync["mapping"].is_null());
  let choices = accepted(
    client
      .get(format!("{origin}{path}/source-choices"))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(choices["items"], json!([]));
  let mut edited = draft.clone();
  edited["description"] = json!("Reviewed requirements");
  let edited = accepted(
    client
      .patch(format!("{origin}{path}"))
      .header("idempotency-key", "edit")
      .header("if-match", "\"1\"")
      .json(&edited)
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(edited["resource"]["version"], 2);
  let default = accepted(
    client
      .get(format!("{origin}/api/v1/projects/{project}/issue-workflow-default"))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  let mut definition = default["workflow"]["definition"].clone();
  let initial = definition["initial_status_id"].clone();
  let to = definition["statuses"][1]["id"].clone();
  let edge = "30000000-0000-4000-8000-000000000001";
  definition["transitions"] =
    json!([{"id":edge,"name":"Local organization","from":initial,"to":to,"origins":["manual"],"evidence":[]}]);
  let target = "20000000-0000-4000-8000-000000000001";
  accepted(
    client
      .post(format!("{origin}/api/v1/projects/{project}/issue-workflows"))
      .header("idempotency-key", "workflow")
      .json(&json!({"id":target,"definition":definition}))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::CREATED,
  )
  .await;
  let review = accepted(
    client.get(format!("{origin}{path}/transitions")).send().await.unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  let mut correspondence = serde_json::Map::new();
  correspondence.insert(initial.as_str().unwrap().into(), initial.clone());
  let adoption = json!({"expected_workflow_id":review["workflow_id"],"expected_workflow_version":review["workflow_version"],"expected_status_id":review["status_id"],"target_workflow_id":target,"target_workflow_version":1,"status_correspondence":correspondence,"expected_field_policy":review["field_policy"]});
  accepted(
    client
      .post(format!("{origin}{path}/workflow-adoption"))
      .header("idempotency-key", "adopt")
      .header("if-match", "\"2\"")
      .json(&adoption)
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  let review = accepted(
    client.get(format!("{origin}{path}/transitions")).send().await.unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(review["items"][0]["available"], true);
  let movement =
    json!({"workflow_id":target,"workflow_version":1,"expected_status_id":initial,"transition_id":edge,"reason":null});
  let moved = accepted(
    client
      .post(format!("{origin}{path}/transitions"))
      .header("idempotency-key", "move")
      .header("if-match", "\"3\"")
      .json(&movement)
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(moved["resource"]["version"], 4);
  assert_eq!(moved["resource"]["status_id"], to);
  let history = accepted(
    client
      .get(format!("{origin}{path}/status-history?limit=200"))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(history["items"].as_array().unwrap().len(), 3);
  let revisions = accepted(
    client
      .get(format!("{origin}{path}/revisions?limit=200"))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(revisions["items"].as_array().unwrap().len(), 4);
  assert!(revisions["items"][0].get("content").is_none());
  let list = accepted(
    client
      .get(format!("{origin}{collection}?limit=1"))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(list["items"][0]["version"], 4);
  runtime.shutdown().await.unwrap();
  let runtime = ServerRuntime::start(settings).await.unwrap();
  let origin = format!("http://{}", runtime.management_addr());
  let replay = accepted(
    client
      .post(format!("{origin}{collection}"))
      .header("idempotency-key", "draft")
      .json(&draft)
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["resource"], created["resource"]);
  let retained = accepted(
    client.get(format!("{origin}{path}/revisions/1")).send().await.unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(retained, created["resource"]);
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}

struct UnavailableInstalledSource;
#[async_trait::async_trait]
impl octacity_server_application::WorkSourceObservations for UnavailableInstalledSource {
  async fn authorize(
    &self,
    _: &octacity_server_application::ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
    _: octacity_server_factory::WorkSourceBinding,
  ) -> Result<(), octacity_server_application::ApplicationError> {
    Ok(())
  }
  async fn read(
    &self,
    _: &octacity_server_application::ManagementRequestContext,
    _: octacity_server_application::ReadWorkSourceObservations,
  ) -> Result<octacity_server_application::WorkSourceObservationPage, octacity_server_application::ApplicationError> {
    Err(octacity_server_application::ApplicationError::SourceDependency {
      class: octacity_server_store::WorkSourceFailure::Transient,
      retry_after_millis: None,
    })
  }
}
#[tokio::test]
#[ignore = "requires OCTACITY_POSTGRES_URL pointing to disposable PostgreSQL"]
async fn runtime_installs_issue_sources_independently_and_provider_outages_preserve_manual_edits() {
  let database = postgres::TestDatabase::migrated().await;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let settings = config::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9");
  let runtime = ServerRuntime::start(settings.clone()).await.unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());
  let project = accepted(
    client
      .post(format!("{origin}/api/v1/projects"))
      .header("idempotency-key", "project")
      .json(&json!({"name":"Manual source"}))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::CREATED,
  )
  .await;
  let project = project["resource"]["id"].as_str().unwrap();
  let issue = accepted(
    client
      .post(format!("{origin}/api/v1/projects/{project}/issues"))
      .header("idempotency-key", "draft")
      .json(&json!({"title":"Local","description":"","acceptance":null,"priority":null}))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::CREATED,
  )
  .await;
  runtime.shutdown().await.unwrap();
  let pin = |id: &str| json!({"identity":id,"version":"v1","digest":"01".repeat(32)});
  let source=serde_json::from_value(json!({"installation":pin("fake-source"),"capability":pin("read"),"configuration":pin("scope"),"resource":pin("board")})).unwrap();
  let runtime = ServerRuntime::start_with_issue_sources(
    settings,
    IssueSourceRuntimeInstallation {
      reader: Arc::new(UnavailableInstalledSource),
      refresh_policy: octacity_server_application::IssueSourceRefreshPolicy {
        read: octacity_server_application::IssueSourceReadPolicy {
          polling: octacity_server_application::SourcePollingPolicy {
            page_items: 100,
            max_pages: 1,
            max_items: 100,
            max_bytes: 65536,
            max_page_bytes: 65536,
            overlap_millis: 1,
            reconciliation_interval_millis: 1000,
            retention_millis: 86400000,
          },
          retry: octacity_server_application::DurableRetryPolicy::new(3, 1, 100).unwrap(),
          poll_interval_millis: 1,
        },
        claim_lifetime: std::time::Duration::from_secs(30),
      },
      mappings: vec![octacity_server_application::InstalledIssueSourceMapping {
        project_id: project.parse().unwrap(),
        source,
        native_resource: octacity_server_factory::ExternalWorkIdentity::new("board-9").unwrap(),
        readable: octacity_server_factory::WorkSourceReportingPolicy {
          states: vec!["Waiting for native review".into()],
          labels: vec![],
        },
        writer: None,
      }],
    },
  )
  .await
  .unwrap();
  let path = format!(
    "http://{}/api/v1/projects/{project}/issues/{}",
    runtime.management_addr(),
    issue["resource"]["id"].as_str().unwrap()
  );
  let choices = accepted(
    client.get(format!("{path}/source-choices")).send().await.unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(
    choices["items"][0]["readable"]["states"],
    json!(["Waiting for native review"])
  );
  let response = client
    .post(format!("{path}/source-link/candidates"))
    .json(&json!({"source":choices["items"][0]["source"]}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
  let mut absent = choices["items"][0]["source"].clone();
  absent["installation"]["identity"] = json!("absent-adapter");
  let response = client
    .post(format!("{path}/source-link/candidates"))
    .json(&json!({"source":absent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

  let edited = accepted(
    client
      .patch(&path)
      .header("idempotency-key", "edit")
      .header("if-match", "\"1\"")
      .json(&json!({"title":"Still local","description":"","acceptance":null,"priority":null}))
      .send()
      .await
      .unwrap(),
    reqwest::StatusCode::OK,
  )
  .await;
  assert_eq!(edited["resource"]["version"], 2);
  assert!(
    accepted(
      client.get(format!("{path}/synchronization")).send().await.unwrap(),
      reqwest::StatusCode::OK
    )
    .await["link"]
      .is_null()
  );
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}
#[path = "issue_code_review_http_tests.rs"]
mod code_reviews;
