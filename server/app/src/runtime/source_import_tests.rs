//! Reviewed import lifecycle and real supervised consumer qualification through protected HTTP and PostgreSQL.
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use octacity_server_store_postgres::PostgresStore;
use serde_json::{Value, json};

pub(super) async fn assert_project_import_controls(client: &reqwest::Client, project: &str, choices: &Value) -> Value {
  let id = uuid::Uuid::new_v4().to_string();
  let base = format!("http://localhost/api/v1/projects/{project}/issue-source-imports");
  let path = format!("{base}/{id}/versions");
  let choice = &choices["items"][0];
  let intent = json!({"id":id,"project_id":project,"expected_current_version":null,"definition":{
    "source":choice["binding"],"connection":{"id":choices["connection_id"],"version":choices["connection_version"],
      "access_version":choices["access_version"],"source_configuration":choice["connection_configuration"],"reporter_configuration":null},
    "predicate":{"state":"native-state","all_of":["control-test-no-match"],"any_of":[],"none_of":[]},"enabled":true
  }});
  let response = client
    .post(&path)
    .header("idempotency-key", "enable-project-import")
    .header("if-none-match", "*")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "reviewed import activation is independent of a Factory target"
  );
  let enabled: Value = response.json().await.unwrap();
  assert_eq!(enabled["configuration"]["version"], 1);
  assert_eq!(enabled["configuration"]["definition"]["enabled"], true);
  let mut disabled = intent.clone();
  disabled["expected_current_version"] = json!(1);
  disabled["definition"]["enabled"] = json!(false);
  let response = client
    .post(&path)
    .header("idempotency-key", "disable-project-import")
    .header("if-match", "\"1\"")
    .json(&json!({"intent":disabled}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let stopped: Value = response.json().await.unwrap();
  assert_eq!(stopped["configuration"]["version"], 2);
  assert_eq!(stopped["configuration"]["definition"]["enabled"], false);
  let replay: Value = client
    .post(&path)
    .header("idempotency-key", "enable-project-import")
    .header("if-none-match", "*")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["configuration"], enabled["configuration"]);
  let current: Value = client
    .get(format!("{base}/{id}"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    current["configuration"], stopped["configuration"],
    "replaying an old enable cannot enable the current configuration"
  );
  let history: Value = client
    .get(format!("{path}/1"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(history["configuration"], enabled["configuration"]);
  let conflict = client
    .post(&path)
    .header("idempotency-key", "stale-project-import")
    .header("if-match", "\"1\"")
    .json(&json!({"intent":disabled}))
    .send()
    .await
    .unwrap();
  assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);
  let broken_id = uuid::Uuid::new_v4().to_string();
  let mut broken = disabled.clone();
  broken["id"] = json!(broken_id);
  broken["expected_current_version"] = Value::Null;
  broken["definition"]["connection"]["version"] = json!(u64::MAX);
  let response = client
    .post(format!("{base}/{broken_id}/versions"))
    .header("idempotency-key", "invalid-import-version")
    .header("if-none-match", "*")
    .json(&json!({"intent":broken}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::BAD_REQUEST,
    "out-of-range native versions are input errors even in disabled drafts"
  );
  let absent_id = uuid::Uuid::new_v4().to_string();
  let mut absent = disabled.clone();
  absent["id"] = json!(absent_id);
  absent["expected_current_version"] = Value::Null;
  absent["definition"]["connection"]["version"] = json!(999);
  let response = client
    .post(format!("{base}/{absent_id}/versions"))
    .header("idempotency-key", "absent-import-version")
    .header("if-none-match", "*")
    .json(&json!({"intent":absent}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::NOT_FOUND,
    "a disabled draft still requires a retained native metadata version"
  );
  json!({"id":id,"disabled":disabled,"configuration":stopped["configuration"]})
}

pub(super) async fn assert_import_disable_after_revocation(client: &reqwest::Client, project: &str, imports: &Value) {
  let mut intent = imports["disabled"].clone();
  intent["expected_current_version"] = json!(2);
  let path = format!(
    "http://localhost/api/v1/projects/{project}/issue-source-imports/{}/versions",
    imports["id"].as_str().unwrap()
  );
  let response = client
    .post(path)
    .header("idempotency-key", "disable-import-with-revoked-read")
    .header("if-match", "\"2\"")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "disablement requires no active read grant"
  );
  let response: Value = response.json().await.unwrap();
  assert_eq!(response["configuration"]["version"], 3);
  assert_eq!(response["configuration"]["definition"]["enabled"], false);
}

#[tokio::test]
#[ignore = "requires OCTACITY_POSTGRES_URL pointing to disposable PostgreSQL"]
async fn source_import_enable_requires_create_visibility_before_resolving_readers() {
  use octacity_server_application as app;
  use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  };
  struct RestrictedCreate;
  #[async_trait::async_trait]
  impl app::ManagementAuthorizationPolicy for RestrictedCreate {
    async fn authorize(
      &self,
      _: &app::ManagementRequestContext,
      action: app::ManagementAction,
      _: &app::ManagementResource,
    ) -> Result<app::ManagementAuthorizationGrant, app::ManagementAuthorizationDenial> {
      Ok(app::ManagementAuthorizationGrant::new(
        if action == app::ManagementAction::Create {
          app::ManagementVisibility::none()
        } else {
          app::ManagementVisibility::all()
        },
      ))
    }
  }
  struct ReaderChoices(
    Arc<AtomicUsize>,
    Arc<PostgresStore>,
    Arc<dyn app::ManagementAuthorizationPolicy>,
  );
  impl app::FactorySourceConfigurationProvider<PostgresStore> for ReaderChoices {
    fn for_project(
      &self,
      _: ProjectId,
      _: &app::ManagementRequestContext,
    ) -> Result<app::FactorySourceConfigurationHandlers<PostgresStore>, app::ApplicationError> {
      self.0.fetch_add(1, Ordering::SeqCst);
      app::FactorySourceConfigurationHandlers::new(self.1.clone(), self.2.clone(), vec![])
    }
  }
  struct UnusedReader;
  #[async_trait::async_trait]
  impl app::WorkSourceObservations for UnusedReader {
    async fn read(
      &self,
      _: &app::ManagementRequestContext,
      _: app::ReadWorkSourceObservations,
    ) -> Result<app::WorkSourceObservationPage, app::ApplicationError> {
      panic!("enablement never reads tasks")
    }
  }
  let database = super::postgres_test_support::TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let calls = Arc::new(AtomicUsize::new(0));
  let policy: Arc<dyn app::ManagementAuthorizationPolicy> = Arc::new(RestrictedCreate);
  let handler = Arc::new(
    app::IssueSourceHandlers::new(
      store.clone(),
      Arc::new(UnusedReader),
      policy.clone(),
      Arc::new(app::SystemFactoryExecutionClock),
    )
    .with_project_source_profiles(Arc::new(ReaderChoices(calls.clone(), store, policy.clone()))),
  );
  let (_fixture, _connection, check) = super::integration_check_tests::fixture("custom-provider");
  let context =
    app::ManagementRequestContext::trusted_network(app::ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let command = app::PublishIssueSourceImportCommand {
    intent: app::IssueSourceImportIntent {
      id: IssueSourceImportId::generate(),
      project_id: ProjectId::generate(),
      expected_current_version: None,
      definition: app::IssueSourceImportDefinition {
        enabled: true,
        predicate: None,
        source: WorkSourceBinding {
          installation: check.installation,
          capability: check.capability.capability,
          configuration: check.capability.configuration.clone(),
          resource: check.resource_reference,
        },
        connection: WorkSourceConnectionRef {
          id: IntegrationId::generate(),
          version: IntegrationVersion::INITIAL,
          access_version: IntegrationVersion::INITIAL,
          source_configuration: check.capability.configuration,
          reporter_configuration: None,
        },
      },
    },
    idempotency_key: "restricted-create-import".parse().unwrap(),
    published_at: Timestamp::from_unix_millis(1).unwrap(),
  };
  let result = app::AuthorizedCommandHandler::new(policy, handler)
    .handle_command(&context, command.clone())
    .await;
  assert!(result.is_err());
  assert_eq!(
    calls.load(Ordering::SeqCst),
    0,
    "a grant excluding this Project cannot resolve readers or enable import"
  );
  struct HiddenProject;
  #[async_trait::async_trait]
  impl app::ManagementAuthorizationPolicy for HiddenProject {
    async fn authorize(
      &self,
      _: &app::ManagementRequestContext,
      _: app::ManagementAction,
      resource: &app::ManagementResource,
    ) -> Result<app::ManagementAuthorizationGrant, app::ManagementAuthorizationDenial> {
      Ok(app::ManagementAuthorizationGrant::new(
        if resource.kind() == app::ManagementResourceKind::Project {
          app::ManagementVisibility::none()
        } else {
          app::ManagementVisibility::all()
        },
      ))
    }
  }
  let policy: Arc<dyn app::ManagementAuthorizationPolicy> = Arc::new(HiddenProject);
  let handler = Arc::new(app::IssueSourceHandlers::new(
    Arc::new(PostgresStore::new(database.pool.clone())),
    Arc::new(UnusedReader),
    policy.clone(),
    Arc::new(app::SystemFactoryExecutionClock),
  ));
  let query = app::GetIssueSourceImportQuery {
    project_id: command.intent.project_id,
    id: command.intent.id,
    version: None,
  };
  let result = app::AuthorizedQueryHandler::new(policy.clone(), handler.clone())
    .handle_query(&context, query)
    .await;
  assert!(
    matches!(
      result,
      Err(app::ManagementHandlerError::Application(app::ApplicationError::Store(
        StoreError::NotFound {
          entity: EntityKind::Project
        }
      )))
    ),
    "hidden Project is rejected before looking up import identity: {result:?}"
  );
  let result = app::AuthorizedCommandHandler::new(policy, handler)
    .handle_command(&context, command)
    .await;
  assert!(
    matches!(
      result,
      Err(app::ManagementHandlerError::Application(app::ApplicationError::Store(
        StoreError::NotFound {
          entity: EntityKind::Project
        }
      )))
    ),
    "hidden Project is rejected before replay or publication: {result:?}"
  );
  database.cleanup().await;
}

pub(super) async fn assert_normal_import_creates_task(
  client: &reqwest::Client,
  project: &str,
  choices: &Value,
  provider: &std::path::Path,
) -> Value {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let configurations: Value = client
    .get(format!("{base}/factory-configurations"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    configurations["items"],
    json!([]),
    "ordinary task import starts before any Factory configuration"
  );
  let id = uuid::Uuid::new_v4().to_string();
  let intent = json!({"id":id,"project_id":project,"expected_current_version":null,"definition":{
    "source":choices["items"][0]["binding"],"connection":{"id":choices["connection_id"],"version":choices["connection_version"],
      "access_version":choices["access_version"],"source_configuration":choices["items"][0]["connection_configuration"],"reporter_configuration":null},"enabled":true}});
  let response = client
    .post(format!("{base}/issue-source-imports/{id}/versions"))
    .header("idempotency-key", "activate-real-task-import")
    .header("if-none-match", "*")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
  let issue = loop {
    let response: Value = client
      .get(format!("{base}/issues"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    if let Some(issue) = response["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|i| i["title"] == "External task draft")
    {
      break issue.clone();
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "explicit normal-startup import must create a visible native task without any Factory target: {response}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
  };
  let task: Value = client
    .get(format!("{base}/issues/{}", issue["id"].as_str().unwrap()))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert!(
    task["content"]["acceptance"].is_null(),
    "a task draft cannot fabricate acceptance"
  );
  let linked = client
    .get(format!("{base}/issues/{}/source-link", issue["id"].as_str().unwrap()))
    .send()
    .await
    .unwrap();
  assert_eq!(linked.status(), reqwest::StatusCode::OK);
  let linked: Value = linked.json().await.unwrap();
  assert_eq!(
    linked["selection"]["connection"], intent["definition"]["connection"],
    "the primary native link must retain its original managed connection pin"
  );
  assert_eq!(linked["selection"]["item"]["item"], "opaque-task");
  let source_choices = client
    .get(format!(
      "{base}/issues/{}/source-choices",
      issue["id"].as_str().unwrap()
    ))
    .send()
    .await
    .unwrap();
  assert_eq!(source_choices.status(), reqwest::StatusCode::OK);
  let source_choices: Value = source_choices.json().await.unwrap();
  assert_eq!(
    source_choices["items"].as_array().unwrap().len(),
    1,
    "a managed linked task uses the same installed native reader as Project discovery"
  );
  assert_eq!(source_choices["items"][0]["source"], intent["definition"]["source"]);
  assert_eq!(
    source_choices["items"][0]["readable"]["states"],
    json!(["native-state"])
  );
  assert_eq!(source_choices["items"][0]["push_fields"], json!([]));
  assert!(source_choices["items"][0]["writer"].is_null());
  let candidates = client
    .post(format!(
      "{base}/issues/{}/source-link/candidates",
      issue["id"].as_str().unwrap()
    ))
    .json(&json!({"source":intent["definition"]["source"]}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    candidates.status(),
    reqwest::StatusCode::OK,
    "existing native candidate preview resolves the retained managed parent"
  );
  let candidates: Value = candidates.json().await.unwrap();
  assert_eq!(candidates["items"][0]["item"], "opaque-task");
  assert!(candidates["items"][0].get("body").is_none());

  let refreshed = client
    .post(format!(
      "{base}/issues/{}/synchronization/refresh",
      issue["id"].as_str().unwrap()
    ))
    .header("idempotency-key", "refresh-managed-native-task")
    .json(&json!({"source":intent["definition"]["source"]}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    refreshed.status(),
    reqwest::StatusCode::OK,
    "explicit refresh uses the retained managed pin and installed finite read policy"
  );
  let refreshed: Value = refreshed.json().await.unwrap();
  assert_eq!(refreshed["issue_id"], issue["id"]);
  let mapped = client.post(format!("{base}/issues/{}/source-link/mappings",issue["id"].as_str().unwrap()))
    .header("idempotency-key","review-managed-pull-map").header("if-match",format!("\"{}\"",source_choices["issue_version"].as_u64().unwrap()))
    .json(&json!({"expected_mapping":null,"definition":{"source":intent["definition"]["source"],"reporter":null,"push_enabled":false,
      "fields":{"title":"source_owned_pull","description":"source_owned_pull"},"status":null}})).send().await.unwrap();
  assert_eq!(
    mapped.status(),
    reqwest::StatusCode::CREATED,
    "reviewed pull mapping uses the installed managed reader without a writer"
  );
  let mapped: Value = mapped.json().await.unwrap();
  assert_eq!(mapped["resource"]["definition"]["push_enabled"], false);
  assert!(
    mapped.get("projection").is_none(),
    "pull publication cannot create an outbound effect"
  );

  let mut disabled = intent.clone();
  disabled["expected_current_version"] = json!(1);
  disabled["definition"]["enabled"] = json!(false);
  let response = client
    .post(format!("{base}/issue-source-imports/{id}/versions"))
    .header("idempotency-key", "disable-import-before-retained-refresh")
    .header("if-match", "\"1\"")
    .json(&json!({"intent":disabled}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  let mut mode: Value = serde_json::from_slice(&original).unwrap();
  mode["candidates"][0]["title"] = json!("Native title after import was disabled");
  mode["candidates"][0]["update_identity"] = json!("native-update-2");
  std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
  wait_for_task_title(
    client,
    &base,
    issue["id"].as_str().unwrap(),
    "Native title after import was disabled",
  )
  .await;
  std::fs::write(mode_path, original).unwrap();
  wait_for_task_title(client, &base, issue["id"].as_str().unwrap(), "External task draft").await;
  assert_manual_attachment(client, &base, &intent, provider).await;

  assert_eq!(task["content"]["title"], "External task draft");
  assert_eq!(task["content"]["description"], "");
  let default: Value = client
    .get(format!("{base}/issue-workflow-default"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(task["workflow_id"], default["workflow"]["id"]);
  assert_eq!(task["workflow_version"], default["workflow"]["version"]);
  assert_eq!(
    task["status_id"],
    default["workflow"]["definition"]["initial_status_id"]
  );
  let runs: Value = client
    .get(format!("{base}/factory-runs"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    runs["items"],
    json!([]),
    "import activation grants no execution authority"
  );
  let schema: Value = client
    .get("http://localhost/api/v1/openapi.json")
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    schema["components"]["schemas"]["IssueSourceSelection"]["properties"]["connection"]["$ref"],
    "#/components/schemas/WorkSourceConnectionReference"
  );
  json!({"intent":intent,"issue":issue,"task":task})
}

pub(super) async fn assert_filtered_import_keeps_task_list(
  client: &reqwest::Client,
  pool: &sqlx::PgPool,
  project: &str,
  choices: &Value,
  provider: &std::path::Path,
) {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let before: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  let cases = [
    (
      json!({"state":"native-state","all_of":["absent-required-label"],"any_of":[],"none_of":[]}),
      "native-state",
    ),
    (
      json!({"state":"native-state","all_of":[],"any_of":["absent-alternative"],"none_of":[]}),
      "native-state",
    ),
    (
      json!({"state":"native-state","all_of":[],"any_of":[],"none_of":["arbitrary"]}),
      "native-state",
    ),
    (
      json!({"state":"native-state","all_of":[],"any_of":[],"none_of":[]}),
      "different-native-state",
    ),
  ];
  for (index, (predicate, state)) in cases.into_iter().enumerate() {
    let mut mode: Value = serde_json::from_slice(&original).unwrap();
    mode["candidates"][0]["state"] = json!(state);
    std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
    let id = IssueSourceImportId::generate();
    let mut intent = json!({"id":id,"project_id":project,"expected_current_version":null,"definition":{
    "source":choices["items"][0]["binding"],"connection":{"id":choices["connection_id"],"version":choices["connection_version"],
      "access_version":choices["access_version"],"source_configuration":choices["items"][0]["connection_configuration"],"reporter_configuration":null},
    "predicate":predicate,"enabled":true}});
    let response = client
      .post(format!("{base}/issue-source-imports/{id}/versions"))
      .header("idempotency-key", format!("activate-filtered-import.{index}"))
      .header("if-none-match", "*")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
      // Completion barrier in the existing durable journal; assertions use the public task list.
      let rows: Vec<Vec<u8>> = sqlx::query_scalar("SELECT document FROM issue_source_reads WHERE project_id=$1")
        .bind(project.parse::<ProjectId>().unwrap().as_uuid())
        .fetch_all(pool)
        .await
        .unwrap();
      if let Some(scan) = rows
        .iter()
        .map(|b| serde_json::from_slice::<IssueSourceReadScan>(b).unwrap())
        .find(|s| s.claim.target.import.as_ref().is_some_and(|i| i.id == id) && s.schedule.is_some())
      {
        assert!(
          scan.schedule.unwrap().failure.is_none(),
          "a valid filtered observation must complete successfully"
        );
        assert!(scan.progress.is_some(), "the native page must have been checkpointed");
        break;
      }
      assert!(
        tokio::time::Instant::now() < deadline,
        "normal import must finish its filtered native page"
      );
      tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let after: Value = client
      .get(format!("{base}/issues"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    assert_eq!(after, before, "a nonmatching native task must not be imported");
    intent["expected_current_version"] = json!(1);
    intent["definition"]["enabled"] = json!(false);
    let response = client
      .post(format!("{base}/issue-source-imports/{id}/versions"))
      .header("idempotency-key", format!("disable-filtered-import.{index}"))
      .header("if-match", "\"1\"")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
  }
  std::fs::write(mode_path, original).unwrap();
}

pub(super) async fn assert_unavailable_import_lifecycle(client: &reqwest::Client, project: &str, controls: &Value) {
  let base = format!("http://localhost/api/v1/projects/{project}/issue-source-imports");
  let mut disabled = controls["disabled"].clone();
  disabled["expected_current_version"] = json!(3);
  let response = client
    .post(format!("{base}/{}/versions", controls["id"].as_str().unwrap()))
    .header("idempotency-key", "disable-with-missing-source-profile")
    .header("if-match", "\"3\"")
    .json(&json!({"intent":disabled}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "disablement remains available without installed profiles"
  );
  let response: Value = response.json().await.unwrap();
  assert_eq!(response["configuration"]["version"], 4);
  assert_eq!(response["configuration"]["definition"]["enabled"], false);
  let mut original = controls["disabled"].clone();
  original["expected_current_version"] = Value::Null;
  original["definition"]["enabled"] = json!(true);
  let response = client
    .post(format!("{base}/{}/versions", controls["id"].as_str().unwrap()))
    .header("idempotency-key", "enable-project-import")
    .header("if-none-match", "*")
    .json(&json!({"intent":original}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "exact original receipt needs no current native profile"
  );
  let replay: Value = response.json().await.unwrap();
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["configuration"]["version"], 1);
  let id = IssueSourceImportId::generate();
  original["id"] = json!(id);
  let response = client
    .post(format!("{base}/{id}/versions"))
    .header("idempotency-key", "enable-with-missing-source-profile")
    .header("if-none-match", "*")
    .json(&json!({"intent":original}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::SERVICE_UNAVAILABLE,
    "an advertised missing read installation is unavailable, not unsupported capability"
  );
}

async fn wait_for_task_title(client: &reqwest::Client, base: &str, issue: &str, title: &str) {
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
  loop {
    let task: Value = client
      .get(format!("{base}/issues/{issue}"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    if task["content"]["title"] == title {
      return;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "reviewed link pull must refresh independently of disabled import: {task}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
  }
}

async fn assert_manual_attachment(client: &reqwest::Client, base: &str, source: &Value, provider: &std::path::Path) {
  let created=client.post(format!("{base}/issues")).header("idempotency-key","manual-native-attachment")
    .json(&json!({"title":"Manual task with native link","description":"Local description","acceptance":null,"priority":null})).send().await.unwrap();
  assert_eq!(created.status(), reqwest::StatusCode::CREATED);
  let created: Value = created.json().await.unwrap();
  let path = format!("{base}/issues/{}", created["resource"]["id"].as_str().unwrap());
  let review: Value = client
    .get(format!("{path}/synchronization"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  let mut mode: Value = serde_json::from_slice(&original).unwrap();
  let mut native = mode["candidates"][0].clone();
  native["item_id"] = json!("manual-native-target");
  native["title"] = json!("A different native title");
  mode["candidates"].as_array_mut().unwrap().push(native);
  std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let response=client.post(format!("{path}/source-link")).header("idempotency-key","attach-reviewed-managed-source")
    .header("if-match",format!("\"{}\"",created["resource"]["version"].as_u64().unwrap()))
    .json(&json!({"source":source["definition"]["source"],"connection":source["definition"]["connection"],
      "item":{"resource":"external-project-42","item":"manual-native-target"},"expected_field_policy":review["field_policy"]})).send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::CREATED,
    "existing manual attachment supports the reviewed managed connection without importing or executing"
  );
  let linked: Value = client
    .get(format!("{path}/source-link"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(linked["selection"]["connection"], source["definition"]["connection"]);
  let after: Value = client.get(&path).send().await.unwrap().json().await.unwrap();
  assert_eq!(
    after["content"], created["resource"]["content"],
    "attachment does not enable field synchronization"
  );
  std::fs::write(mode_path, original).unwrap();
}
