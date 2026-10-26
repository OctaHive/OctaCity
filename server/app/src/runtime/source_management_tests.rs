//! Protected Source publication against PostgreSQL; setup uses existing public store contracts.
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::{testing::management_mutation, *};
use octacity_server_store_postgres::PostgresStore;
use serde_json::{Value, json};

pub(super) async fn assert_input_publication(
  client: &reqwest::Client,
  pool: sqlx::PgPool,
  project: ProjectId,
  choices: &Value,
  provider: &std::path::Path,
) -> Value {
  let store = PostgresStore::new(pool);
  let at = Timestamp::from_unix_millis(1).unwrap();
  let configuration = super::work_source_tests::configuration(project, FactoryConfigurationId::generate());
  let draft = FactoryConfigurationDraft {
    flow: Some(configuration.flow().clone()),
    admission_policy: FactoryKey::new("admission").unwrap(),
    wip_limits: configuration.wip_limits(),
    hard_budget: configuration.hard_budget(),
    permission_ceiling: FactoryKey::new("permissions").unwrap(),
    enabled: true,
  };
  store
    .create_factory_configuration(management_mutation(CreateFactoryConfiguration {
      intent: FactoryConfigurationMutationIntent::Create {
        id: configuration.reference().id(),
        project_id: project,
        definition_digest: configuration.reference().definition_digest(),
        draft,
      },
      configuration: configuration.clone(),
      idempotency_key: IdempotencyKey::new("http-input-target").unwrap(),
      published_at: at,
    }))
    .await
    .unwrap();
  let repository = super::work_source_tests::repository(project, RepositoryId::generate(), None);
  store
    .create_repository(management_mutation(CreateRepository {
      id: repository.id,
      project_id: project,
      name: repository.name.clone(),
      definition: repository.definition.clone(),
      idempotency_key: IdempotencyKey::new("http-input-repository").unwrap(),
      published_at: at,
    }))
    .await
    .unwrap();
  let repository_ref = WorkSourceRepositoryRef {
    project_id: project,
    id: repository.id,
    version: repository.version,
    definition_digest: FactoryDigest::content_sha256(&serde_json::to_vec(&repository.definition).unwrap()),
  };
  let input = WorkSourceInputId::generate();
  let source = &choices["items"][0];
  let intent = json!({"project_id":project,"inputs":[{"id":input,"expected_version":null,
    "definition":{"target":configuration.reference(),"repository":repository_ref,
      "source":source["binding"],"reporter":null,"enabled":false,
      "connection":{"id":choices["connection_id"],"version":choices["connection_version"],
        "access_version":choices["access_version"],"source_configuration":source["connection_configuration"],
        "reporter_configuration":null}}}],"group":null});
  let base = format!("http://localhost/api/v1/projects/{project}");
  let response = client
    .post(format!("{base}/factory-sources/validate"))
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  // Validation grants no activation and creates no retained input history.
  let history: Value = client
    .get(format!("{base}/factory-sources"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(history, json!({"items":[],"next_cursor":null}));
  assert_preview(client, &base, &intent, provider).await;
  let path = format!("{base}/factory-source-inputs/{input}/versions");
  let publish = || {
    client
      .post(&path)
      .header("idempotency-key", "http-input")
      .header("if-none-match", "*")
      .json(&json!({"intent":intent}))
  };
  let response = publish().send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let original: Value = response.json().await.unwrap();
  assert_eq!(original["disposition"], "applied");
  let replay: Value = publish().send().await.unwrap().json().await.unwrap();
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["publication"], original["publication"]);
  assert_eq!(replay["published_at_unix_ms"], original["published_at_unix_ms"]);
  let exact: Value = client
    .get(format!("{path}/1"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(exact["definition"], original["publication"]["inputs"][0]["input"]);
  let stale = client
    .post(&path)
    .header("idempotency-key", "stale-input")
    .header("if-none-match", "*")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(stale.status(), reqwest::StatusCode::CONFLICT);
  let mut invalid = intent.clone();
  invalid["inputs"][0]["definition"]["predicate"] =
    json!({"state":"uninstalled-state","all_of":[],"any_of":[],"none_of":[]});
  let response = client
    .post(format!("{base}/factory-sources/validate"))
    .json(&json!({"intent":invalid}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  let diagnostics = client.get(format!("{path}/1/diagnostics")).send().await.unwrap();
  assert_eq!(diagnostics.status(), reqwest::StatusCode::OK);
  let diagnostics: Value = diagnostics.json().await.unwrap();
  assert!(diagnostics["scan"].is_null());
  assert_eq!(diagnostics["admissions"], json!([]));
  assert_eq!(store.source_scan_targets(project, None, 64).await.unwrap().len(), 0);
  assert!(
    store
      .list_factory_runs(
        ListFactoryRuns::new(
          Some(project),
          FactoryRunFilter::default(),
          None,
          64,
          FactoryRunListVisibility::all()
        )
        .unwrap()
      )
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert_group_publication(client, &base, &intent).await;
  json!({"input":input,"intent":intent,"publication":original["publication"]})
}

async fn assert_preview(client: &reqwest::Client, base: &str, intent: &Value, provider: &std::path::Path) {
  let previous_calls = std::fs::read_to_string(provider.join("requests.jsonl"))
    .unwrap_or_default()
    .lines()
    .count();
  let issues: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let request = |intent: &Value| {
    client
      .post(format!("{base}/factory-sources/preview"))
      .json(&json!({"intent":intent,"limit":2}))
  };
  let response = request(intent).send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let preview: Value = response.json().await.unwrap();
  assert!(preview["failure"].is_null(), "{preview}");
  assert_eq!(preview["candidates"].as_array().unwrap().len(), 1);
  assert_eq!(preview["candidates"][0]["disposition"], "selected");
  assert_eq!(preview["candidates"][0]["summary"]["title"], "External task draft");
  assert!(preview["candidates"][0]["issue"].is_null());
  assert_eq!(preview["candidates"][0]["admissions"], json!([]));
  assert!(!preview.to_string().contains("reader"));
  let mut narrowed = intent.clone();
  narrowed["inputs"][0]["definition"]["predicate"] =
    json!({"state":"native-state","all_of":["not-present"],"any_of":[],"none_of":[]});
  let filtered: Value = request(&narrowed).send().await.unwrap().json().await.unwrap();
  assert_eq!(filtered["candidates"][0]["disposition"], "filtered");
  let mut group = intent.clone();
  let first = group["inputs"][0].clone();
  let mut second = first.clone();
  second["id"] = json!(WorkSourceInputId::generate());
  group["inputs"] = json!([first, second]);
  group["group"] = json!({"id":WorkSourceGroupId::generate(),"expected_version":null,"members":[
    {"id":group["inputs"][0]["id"],"version":1},{"id":group["inputs"][1]["id"],"version":1}
  ]});
  let ambiguous: Value = request(&group).send().await.unwrap().json().await.unwrap();
  assert_eq!(ambiguous["candidates"][0]["disposition"], "ambiguous");
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  std::fs::write(
    &mode_path,
    br#"{"failure":{"class":"transient","retry_after_millis":500}}"#,
  )
  .unwrap();
  let deferred: Value = request(intent).send().await.unwrap().json().await.unwrap();
  assert_eq!(deferred["failure"], "rate_limited");
  assert_eq!(deferred["retry_after_millis"], 500);
  assert_eq!(deferred["candidates"], json!([]));
  std::fs::write(mode_path, original).unwrap();
  let after: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(after, issues, "preview cannot import or alter an Issue");
  let calls = std::fs::read_to_string(provider.join("requests.jsonl")).unwrap();
  assert_eq!(calls.lines().count() - previous_calls, 4);
  for line in calls.lines().skip(previous_calls) {
    let wire: Value = serde_json::from_str(line).unwrap();
    assert_eq!(wire["message"]["command"]["operation"], "preview");
  }
}

pub(super) async fn assert_history_after_replacement(client: &reqwest::Client, project: ProjectId, published: &Value) {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let path = format!(
    "{base}/factory-source-inputs/{}/versions",
    published["input"].as_str().unwrap()
  );
  let exact: Value = client
    .get(format!("{path}/1"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(exact["definition"], published["publication"]["inputs"][0]["input"]);
  let replay: Value = client
    .post(&path)
    .header("idempotency-key", "http-input")
    .header("if-none-match", "*")
    .json(&json!({"intent":published["intent"]}))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["publication"], published["publication"]);
  // A fresh request cannot substitute the new installation for an older exact Source binding.
  let mut intent = published["intent"].clone();
  intent["inputs"][0]["definition"]["connection"]["access_version"] = json!(3);
  let response = client
    .post(format!("{base}/factory-sources/validate"))
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  let failure: Value = response.json().await.unwrap();
  assert!(failure.to_string().contains("/inputs/0/definition/source"), "{failure}");
}

pub(super) async fn assert_project_preview(
  client: &reqwest::Client,
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
  let response = client
    .post(format!("{base}/issue-sources/preview"))
    .json(
      &json!({"connection_id":choices["connection_id"],"connection_version":choices["connection_version"],
      "access_version":choices["access_version"],"source":choices["items"][0]["binding"],"limit":2}),
    )
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "Project read-only preview needs no Issue, Repository or Factory target"
  );
  let preview: Value = response.json().await.unwrap();
  assert!(preview["failure"].is_null(), "{preview}");
  assert_eq!(preview["items"].as_array().unwrap().len(), 1);
  assert_eq!(preview["items"][0]["title"], "External task draft");
  assert_eq!(preview["items"][0]["item"]["item"], "opaque-task");
  assert!(preview["items"][0].get("body").is_none());
  assert!(preview["items"][0].get("acceptance").is_none());
  assert!(!preview.to_string().contains("reader"));
  let mode = provider.join("mode.json");
  let original = std::fs::read(&mode).unwrap();
  std::fs::write(&mode, br#"{"failure":{"class":"transient","retry_after_millis":500}}"#).unwrap();
  let deferred: Value = client
    .post(format!("{base}/issue-sources/preview"))
    .json(
      &json!({"connection_id":choices["connection_id"],"connection_version":choices["connection_version"],
      "access_version":choices["access_version"],"source":choices["items"][0]["binding"],"limit":2}),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(deferred["failure"], "rate_limited");
  assert_eq!(deferred["retry_after_millis"], 500);
  assert_eq!(deferred["items"], json!([]));
  std::fs::write(mode, original).unwrap();
  let after: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(after, before, "read-only preview cannot import a task");
}

pub(super) async fn assert_project_preview_rejects_foreign_scope(
  client: &reqwest::Client,
  project: &str,
  foreign: &Value,
) {
  let path = format!("http://localhost/api/v1/projects/{project}/issue-sources/preview");
  let body = json!({"connection_id":foreign["connection_id"],"connection_version":foreign["connection_version"],
    "access_version":foreign["access_version"],"source":foreign["items"][0]["binding"],"limit":2});
  let response = client.post(&path).json(&body).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  for limit in [0, 101] {
    let mut invalid = body.clone();
    invalid["limit"] = json!(limit);
    let response = client.post(&path).json(&invalid).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  }
  let mut invalid = body;
  invalid["token"] = json!("private-preview-canary");
  let response = client.post(path).json(&invalid).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  assert!(!response.text().await.unwrap().contains("private-preview-canary"));
}

async fn assert_group_publication(client: &reqwest::Client, base: &str, original: &Value) {
  let mut grouped = original.clone();
  let first = grouped["inputs"][0]["id"].clone();
  let second = WorkSourceInputId::generate();
  grouped["inputs"][0]["id"] = json!(second);
  let group = WorkSourceGroupId::generate();
  grouped["group"] = json!({"id":group,"expected_version":null,"members":[
    {"id":first,"version":1},{"id":second,"version":1}
  ]});
  let path = format!("{base}/factory-source-groups/{group}/versions");
  let response = client
    .post(&path)
    .header("idempotency-key", "http-group")
    .header("if-none-match", "*")
    .json(&json!({"intent":grouped}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let publication: Value = response.json().await.unwrap();
  let exact: Value = client
    .get(format!("{path}/1"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(exact["definition"], publication["publication"]["group"]["group"]);
  let replay: Value = client
    .post(&path)
    .header("idempotency-key", "http-group")
    .header("if-none-match", "*")
    .json(&json!({"intent":grouped}))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["publication"], publication["publication"]);
  // A group conflict rolls back its accompanying new input in the same transaction.
  let rolled_back = WorkSourceInputId::generate();
  grouped["inputs"][0]["id"] = json!(rolled_back);
  grouped["group"]["members"][1]["id"] = json!(rolled_back);
  let response = client
    .post(&path)
    .header("idempotency-key", "conflicting-http-group")
    .header("if-none-match", "*")
    .json(&json!({"intent":grouped}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
  assert_eq!(
    client
      .get(format!("{base}/factory-source-inputs/{rolled_back}/versions/1"))
      .send()
      .await
      .unwrap()
      .status(),
    reqwest::StatusCode::NOT_FOUND
  );
  let mut cursor: Option<String> = None;
  let mut identities = std::collections::BTreeSet::new();
  loop {
    let mut url = url::Url::parse(&format!("{base}/factory-sources")).unwrap();
    url.query_pairs_mut().append_pair("limit", "1");
    if let Some(after) = &cursor {
      url.query_pairs_mut().append_pair("after", after);
    }
    let page: Value = client.get(url).send().await.unwrap().json().await.unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    let record = &page["items"][0];
    assert!(identities.insert((
      record["kind"].as_str().unwrap().to_owned(),
      record["definition"]["id"].as_str().unwrap().to_owned()
    )));
    cursor = page["next_cursor"].as_str().map(str::to_owned);
    if cursor.is_none() {
      break;
    }
    assert!(
      identities.len() < 4,
      "pagination must advance through only the three retained definitions"
    );
  }
  assert_eq!(identities.len(), 3);
}

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

pub(super) async fn assert_project_preview_retry_bound(
  pool: sqlx::PgPool,
  project: ProjectId,
  choices: &Value,
  profiles: &std::path::Path,
  registry: &std::path::Path,
  installed: &InstalledIntegration,
) {
  use octacity_server_application as app;
  use std::sync::Arc;
  struct DeferredReader;
  #[async_trait::async_trait]
  impl app::WorkSourceObservations for DeferredReader {
    async fn read(
      &self,
      _: &app::ManagementRequestContext,
      _: app::ReadWorkSourceObservations,
    ) -> Result<app::WorkSourceObservationPage, app::ApplicationError> {
      Err(app::ApplicationError::SourceDependency {
        class: WorkSourceFailure::RateLimited,
        retry_after_millis: Some(u32::MAX),
      })
    }
  }
  let registry = Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(registry).unwrap());
  let profiles =
    super::source_profiles_file::SourceProfiles::load(profiles, std::slice::from_ref(installed), &registry)
      .await
      .unwrap();
  let store = Arc::new(PostgresStore::new(pool));
  let policy: Arc<dyn app::ManagementAuthorizationPolicy> = Arc::new(app::TrustedNetworkManagementPolicy);
  let provider = Arc::new(super::source_management::SourceManagement {
    store: store.clone(),
    policy: policy.clone(),
    profiles,
    registry,
    timeout: std::time::Duration::from_secs(1),
    grace: std::time::Duration::from_millis(20),
    cancellation: tokio_util::sync::CancellationToken::new(),
  });
  let handlers = Arc::new(
    app::IssueSourceHandlers::new(
      store,
      Arc::new(DeferredReader),
      policy.clone(),
      Arc::new(app::SystemFactoryExecutionClock),
    )
    .with_project_source_profiles(Arc::new(super::source_management::IssueSourceChoiceProvider(provider))),
  );
  let context =
    app::ManagementRequestContext::trusted_network(app::ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let result = app::AuthorizedQueryHandler::new(policy, handlers)
    .handle_query(
      &context,
      app::PreviewProjectIssueSourcesQuery {
        selection: app::ListWorkSourceInputChoicesQuery {
          project_id: project,
          connection_id: serde_json::from_value(choices["connection_id"].clone()).unwrap(),
          connection_version: IntegrationVersion::INITIAL,
        },
        access_version: serde_json::from_value(choices["access_version"].clone()).unwrap(),
        source: serde_json::from_value(choices["items"][0]["binding"].clone()).unwrap(),
        limit: 1,
      },
    )
    .await
    .unwrap();
  assert_eq!(
    result.retry_after_millis,
    Some(3_600_000),
    "public advice stays within the documented preview bound regardless of the observation port"
  );
  assert!(result.items.is_empty());
}
