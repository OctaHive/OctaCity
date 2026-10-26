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
      "source":source["binding"],"reporter":null,"enabled":true,
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
  assert_eq!(store.source_scan_targets(project, None, 64).await.unwrap().len(), 1);
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
  json!({"input":input,"intent":intent,"publication":original["publication"]})
}

async fn assert_preview(client: &reqwest::Client, base: &str, intent: &Value, provider: &std::path::Path) {
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
  assert_eq!(calls.lines().count(), 4);
  for line in calls.lines() {
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
