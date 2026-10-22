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

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn runtime_content_expiry_is_visible_over_http_and_cannot_be_replayed_as_text() {
  use octacity_server_store::*;
  let database = postgres::TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = ProjectId::generate();
  store
    .create_project(management_mutation(CreateProject {
      id: project,
      parent_id: None,
      name: ProjectName::new("Retention pilot").unwrap(),
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
  let origin = format!("http://{}", server.management_addr());
  let client = reqwest::Client::new();
  let collection = format!("{origin}/api/v1/projects/{project}/issues");
  let body = json!({"title":"Retained manual task","description":"Private retained text"});
  let response = client
    .post(&collection)
    .header("idempotency-key", "manual")
    .json(&body)
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CREATED);
  let created: Value = response.json().await.unwrap();
  let issue = created["resource"]["id"].as_str().unwrap().parse().unwrap();
  let now = Timestamp::from_unix_millis(
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap()
      .as_millis()
      .try_into()
      .unwrap(),
  )
  .unwrap();
  {
    use octacity_server_application as app;
    use std::sync::Arc;
    let context =
      app::ManagementRequestContext::trusted_network(app::ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    for (admin, visible) in [(false, true), (true, false)] {
      let policy = Arc::new(RetentionAuthority { admin, visible });
      let handlers = Arc::new(app::IssueHandlers::new(
        Arc::new(PostgresStore::new(database.pool.clone())),
        policy.clone(),
      ));
      let query = app::GetIssueContentRetentionQuery {
        project_id: project,
        issue_id: issue,
        kind: app::IssueContentRetentionKind::Requirements,
      };
      assert!(
        app::AuthorizedQueryHandler::new(policy.clone(), handlers.clone())
          .handle_query(&context, query)
          .await
          .is_err()
      );
      assert!(
        app::AuthorizedCommandHandler::new(policy, handlers)
          .handle_command(
            &context,
            app::SetIssueContentRetentionCommand {
              project_id: project,
              issue_id: issue,
              expected_version: 0,
              plan: app::IssueContentRetentionPlan {
                scope: app::IssueContentRetentionScope::Requirements {
                  through: octacity_server_domain::IssueVersion::INITIAL
                },
                expires_at: now,
                hold: None
              },
              idempotency_key: "denied-retention".parse().unwrap(),
              configured_at: now,
            }
          )
          .await
          .is_err(),
        "editing or hidden-Issue administration cannot configure body expiry"
      );
    }
  }
  let retention_url = format!("{collection}/{issue}/content-retention/requirements");
  let initial = client.get(&retention_url).send().await.unwrap();
  assert_eq!(initial.status(), reqwest::StatusCode::OK);
  assert_eq!(initial.headers()["cache-control"], "no-store");
  let initial: Value = initial.json().await.unwrap();
  assert_eq!(initial["version"], 0);
  assert!(initial["plan"].is_null());
  let configured=client.post(&retention_url).header("idempotency-key","expire").json(&json!({"expected_version":0,"plan":{"scope":{"kind":"requirements","through":1},"expires_at":now.unix_millis(),"hold":null}})).send().await.unwrap();
  assert_eq!(configured.status(), reqwest::StatusCode::OK);
  let configured: Value = configured.json().await.unwrap();
  assert_eq!(configured["version"], 1);
  tokio::time::timeout(std::time::Duration::from_secs(10), async {
    while store.issue(project, issue, None).await.unwrap().content_available {
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
  })
  .await
  .expect("content retention runs independently of Factory and providers");
  let response = client.get(format!("{collection}/{issue}")).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let current: Value = response.json().await.unwrap();
  assert_eq!(current["content_available"], false);
  assert_eq!(current["content"]["description"], "");
  let replay: Value = client
    .post(&collection)
    .header("idempotency-key", "manual")
    .json(&body)
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(replay["resource"]["id"], created["resource"]["id"]);
  assert_eq!(replay["resource"]["content_available"], false);
  assert!(!replay.to_string().contains("Private retained text"));
  server.shutdown().await.unwrap();
  database.cleanup().await;
}

struct RetentionAuthority {
  admin: bool,
  visible: bool,
}
#[async_trait::async_trait]
impl octacity_server_application::ManagementAuthorizationPolicy for RetentionAuthority {
  async fn authorize(
    &self,
    _: &octacity_server_application::ManagementRequestContext,
    action: octacity_server_application::ManagementAction,
    _: &octacity_server_application::ManagementResource,
  ) -> Result<
    octacity_server_application::ManagementAuthorizationGrant,
    octacity_server_application::ManagementAuthorizationDenial,
  > {
    use octacity_server_application::*;
    if action == ManagementAction::Administer && !self.admin {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(if self.visible {
      ManagementVisibility::all()
    } else {
      ManagementVisibility::none()
    }))
  }
}
