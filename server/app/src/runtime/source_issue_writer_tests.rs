//! Protected Issue choices use the same installed Reporter and independent live grants.
use super::*;

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn normal_issue_choices_expose_only_the_independently_granted_installed_writer() {
  let (fixture, profile) = fixture("opaque-writer").await;
  let (fixture, database) = ordering::postgres::postgres_fixture(fixture).await;
  let (fixture, _) = managed_fixture_with_native_parent(fixture, profile, Some(exact("native-writer", 71))).await;
  let (deployment, config, _) = installed_reporter(&fixture, &database.url()).await;
  let connection = fixture.input.definition().connection.as_ref().unwrap();
  let metadata = fixture.store.integration_connection(connection.id, None).await.unwrap();
  let read = &fixture.profile.read;
  let native = metadata.definition.resources[0]
    .capabilities
    .iter()
    .find(|c| c.purpose == IntegrationPurpose::TaskRead)
    .unwrap();
  let profiles_path = deployment.path().join("shared-source-profiles.json");
  let mut profiles: Value = serde_json::from_slice(&std::fs::read(&profiles_path).unwrap()).unwrap();
  profiles["readers"] = json!([{
    "identity":"task-reader","version":"v1",
    "read":{"installation":read.binding.installation,"resource_reference":read.binding.resource,
      "capability":native,"plugin":read.plugin,"provider_configuration":read.provider_configuration,
      "resource":read.resource,"network_hosts":read.network_hosts},
    "selection":read.selection,"readable":{"labels":[],"states":["native-state"]},
    "polling":{"page_items":2,"max_pages":1,"max_items":2,"max_bytes":65536,"max_page_bytes":65536,
      "overlap_millis":20,"reconciliation_interval_millis":1000,"retention_millis":60000}
  }]);
  profiles["reporters"][0]["reporting"] = json!({
    "mapping":{"statuses":[{"status":"reviewed","add":["custom-ready"],"remove":[],"state":null}]},
    "editable":{"labels":["custom-ready"],"states":[]}
  });
  profiles["reporters"][0]["issue_delivery"] = json!({
    "retry":{"max_attempts":3,"retry_delay_millis":100},"window_millis":30000,
    "budget":BudgetLimit::new(1,5000,0,0,10000).unwrap()
  });
  std::fs::write(&profiles_path, serde_json::to_vec(&profiles).unwrap()).unwrap();
  let runtime = crate::ServerRuntime::start(config.clone()).await.unwrap();
  let client = reqwest::Client::builder()
    .unix_socket(deployment.path().join("management.sock"))
    .build()
    .unwrap();
  let created = client
    .post("http://localhost/api/v1/projects")
    .header("idempotency-key", "writer-choice-project")
    .json(&json!({"name":"Independent task sync"}))
    .send()
    .await
    .unwrap();
  assert_eq!(created.status(), reqwest::StatusCode::CREATED);
  let created: Value = created.json().await.unwrap();
  let project = created["resource"]["id"].as_str().unwrap();
  let base = format!("http://localhost/api/v1/projects/{project}");
  let grant_path = format!(
    "http://localhost/api/v1/integrations/{}/projects/{project}/access",
    connection.id
  );
  let grant = |expected: Option<u64>, purposes: Value, key: &str| {
    let request = client.post(&grant_path).header("idempotency-key", key).json(
      &json!({"intent":{"connection_id":connection.id,"connection_version":connection.version,
        "project_id":project,"expected_current_version":expected,
        "resources":[{"reference":read.binding.resource,"purposes":purposes}]}}),
    );
    match expected {
      Some(version) => request.header("if-match", format!("\"{version}\"")),
      None => request.header("if-none-match", "*"),
    }
  };
  let response = grant(None, json!(["task_read", "task_update"]), "grant-issue-writer")
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let choices: Value = client
    .get(format!(
      "{base}/issue-source-choices?connection_id={}&connection_version={}",
      connection.id,
      connection.version.get()
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let source = &choices["items"][0];
  let import = uuid::Uuid::new_v4().to_string();
  let response = client
    .post(format!("{base}/issue-source-imports/{import}/versions"))
    .header("idempotency-key", "import-before-writer-choice")
    .header("if-none-match", "*")
    .json(
      &json!({"intent":{"id":import,"project_id":project,"expected_current_version":null,
      "definition":{"source":source["binding"],"connection":{"id":connection.id,"version":connection.version,
        "access_version":choices["access_version"],"source_configuration":source["connection_configuration"],
        "reporter_configuration":null},"enabled":true}}}),
    )
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
  let issue = loop {
    let tasks: Value = client
      .get(format!("{base}/issues"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    if let Some(issue) = tasks["items"].as_array().unwrap().first() {
      break issue.clone();
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "Source-only import must create a visible task"
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
  };
  let choices_path = format!("{base}/issues/{}/source-choices", issue["id"].as_str().unwrap());
  let observed: Value = client.get(&choices_path).send().await.unwrap().json().await.unwrap();
  assert_eq!(
    observed["items"][0]["writer"]["effects"],
    json!({"labels":["custom-ready"],"states":[]})
  );
  let binding = observed["items"][0]["writer"]["binding"].clone();
  assert_eq!(binding["capability"], json!(capability()));
  assert_ne!(
    binding["configuration"],
    json!(exact("native-writer", 71)),
    "native parent and derived settings stay separate"
  );
  let mut mapping = review_push_after_workflow_change(&client, &base, &issue, &source["binding"], &binding).await;
  let mapping_path = format!("{base}/issues/{}/source-link/mappings", issue["id"].as_str().unwrap());
  let publish = |body: Value, key: &str| {
    client
      .post(&mapping_path)
      .header("idempotency-key", key)
      .header("if-match", format!("\"{}\"", issue["version"].as_u64().unwrap()))
      .json(&body)
  };
  let mut foreign = mapping.clone();
  foreign["definition"]["reporter"]["configuration"]["digest"] = json!("f".repeat(64));
  assert_eq!(
    publish(foreign, "reject-foreign-writer").send().await.unwrap().status(),
    reqwest::StatusCode::UNPROCESSABLE_ENTITY
  );
  let response = publish(mapping.clone(), "review-exact-writer").send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::CREATED,
    "{}",
    response.text().await.unwrap()
  );
  let published: Value = response.json().await.unwrap();
  assert_eq!(published["resource"]["definition"], mapping["definition"]);
  mapping["expected_mapping"] = published["mapping_reference"].clone();
  let response = grant(Some(1), json!(["task_read"]), "revoke-only-issue-writer")
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let revoked: Value = client.get(&choices_path).send().await.unwrap().json().await.unwrap();
  assert_eq!(revoked["items"].as_array().unwrap().len(), 1);
  assert!(revoked["items"][0]["writer"].is_null());
  assert_eq!(revoked["items"][0]["push_fields"], json!([]));
  assert_eq!(
    publish(mapping.clone(), "reject-revoked-writer")
      .send()
      .await
      .unwrap()
      .status(),
    reqwest::StatusCode::UNPROCESSABLE_ENTITY
  );
  let restored = grant(Some(2), json!(["task_read", "task_update"]), "restore-issue-writer")
    .send()
    .await
    .unwrap();
  assert_eq!(restored.status(), reqwest::StatusCode::OK);
  let restored: Value = client.get(&choices_path).send().await.unwrap().json().await.unwrap();
  assert_eq!(
    restored["items"][0]["writer"]["binding"], binding,
    "grant renewal cannot replace installed settings"
  );
  let response = grant(Some(3), json!(["task_update"]), "revoke-only-issue-reader")
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let read_revoked: Value = client.get(&choices_path).send().await.unwrap().json().await.unwrap();
  assert_eq!(
    read_revoked["items"],
    json!([]),
    "a writer grant cannot substitute for task-read authority"
  );
  let response = grant(Some(4), json!(["task_read", "task_update"]), "restore-issue-reader")
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let runs: Value = client
    .get(format!("{base}/factory-runs"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(runs["items"], json!([]));
  assert!(captured_requests(&fixture.root.path().join("opaque-writer")).is_empty());
  runtime.shutdown().await.unwrap();
  // Mapping and delivery bounds both participate in the exact immutable writer choice.
  for (pointer, value, key) in [
    (
      "/reporters/0/mapping/sha256",
      json!("f".repeat(64)),
      "native-map-replaced",
    ),
    (
      "/reporters/0/issue_delivery/window_millis",
      json!(15000),
      "delivery-bounds-replaced",
    ),
  ] {
    *profiles.pointer_mut(pointer).unwrap() = value;
    std::fs::write(&profiles_path, serde_json::to_vec(&profiles).unwrap()).unwrap();
    let runtime = crate::ServerRuntime::start(config.clone()).await.unwrap();
    let changed: Value = client.get(&choices_path).send().await.unwrap().json().await.unwrap();
    let replacement = changed["items"][0]["writer"]["binding"].clone();
    assert_ne!(
      replacement, mapping["definition"]["reporter"],
      "{key} must replace the writer pin"
    );
    assert_eq!(
      changed["items"][0]["source"], source["binding"],
      "writer replacement must not migrate the task reader"
    );
    assert_eq!(
      publish(mapping.clone(), key).send().await.unwrap().status(),
      reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );
    mapping["definition"]["reporter"] = replacement;
    let response = publish(mapping.clone(), &format!("review-{key}")).send().await.unwrap();
    assert_eq!(
      response.status(),
      reqwest::StatusCode::CREATED,
      "{}",
      response.text().await.unwrap()
    );
    let accepted: Value = response.json().await.unwrap();
    mapping["expected_mapping"] = accepted["mapping_reference"].clone();
    let task: Value = client
      .get(format!("{base}/issues/{}", issue["id"].as_str().unwrap()))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    assert_eq!(task["workflow_id"], issue["workflow_id"]);
    assert_eq!(task["workflow_version"], issue["workflow_version"]);
    assert_eq!(task["status_id"], issue["status_id"]);
    assert!(captured_requests(&fixture.root.path().join("opaque-writer")).is_empty());
    runtime.shutdown().await.unwrap();
  }
  // A Work-only Reporter cannot advertise task delivery; ambiguous settings fail closed.
  for ambiguous in [false, true] {
    let mut unavailable = profiles.clone();
    if ambiguous {
      let mut other = unavailable["reporters"][0].clone();
      other["identity"] = json!("another-issue-writer");
      unavailable["reporters"].as_array_mut().unwrap().push(other);
    } else {
      unavailable["reporters"][0]["issue_delivery"] = Value::Null;
    }
    std::fs::write(&profiles_path, serde_json::to_vec(&unavailable).unwrap()).unwrap();
    let runtime = crate::ServerRuntime::start(config.clone()).await.unwrap();
    let response = client.get(&choices_path).send().await.unwrap();
    if ambiguous {
      assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
      assert_eq!(
        response.json::<Value>().await.unwrap()["code"],
        "capability_unavailable"
      );
    } else {
      assert_eq!(response.status(), reqwest::StatusCode::OK);
      let choices: Value = response.json().await.unwrap();
      assert_eq!(choices["items"].as_array().unwrap().len(), 1);
      assert!(choices["items"][0]["writer"].is_null());
    }
    assert_eq!(
      publish(
        mapping.clone(),
        if ambiguous {
          "reject-ambiguous-writer"
        } else {
          "reject-work-only-writer"
        }
      )
      .send()
      .await
      .unwrap()
      .status(),
      reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(captured_requests(&fixture.root.path().join("opaque-writer")).is_empty());
    runtime.shutdown().await.unwrap();
  }
  std::fs::write(&profiles_path, serde_json::to_vec(&profiles).unwrap()).unwrap();
  replace_installed_native_parent(deployment.path());
  let runtime = crate::ServerRuntime::start(config).await.unwrap();
  let choices: Value = client.get(&choices_path).send().await.unwrap().json().await.unwrap();
  assert_eq!(choices["items"].as_array().unwrap().len(), 1);
  assert!(
    choices["items"][0]["writer"].is_null(),
    "a replacement installed native parent cannot borrow the original connection's grant"
  );
  assert_eq!(
    publish(mapping, "reject-replaced-native-parent")
      .send()
      .await
      .unwrap()
      .status(),
    reqwest::StatusCode::UNPROCESSABLE_ENTITY
  );
  assert!(captured_requests(&fixture.root.path().join("opaque-writer")).is_empty());
  let historical: Value = client
    .get(format!("{mapping_path}/1"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    historical, published["resource"],
    "installation replacement cannot rewrite the original mapping"
  );
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}

async fn review_push_after_workflow_change(
  client: &reqwest::Client,
  base: &str,
  issue: &Value,
  source: &Value,
  writer: &Value,
) -> Value {
  let default: Value = client
    .get(format!("{base}/issue-workflow-default"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let workflow = &default["workflow"];
  let mut replacement = workflow["definition"].clone();
  replacement["name"] = json!("Published after import");
  let response = client
    .post(format!(
      "{base}/issue-workflows/{}/versions",
      workflow["id"].as_str().unwrap()
    ))
    .header("idempotency-key", "publish-after-attachment")
    .header("if-match", format!("\"{}\"", workflow["version"].as_u64().unwrap()))
    .json(&json!({"definition":replacement}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let response = client
    .post(format!("{base}/issue-workflow-default"))
    .header("idempotency-key", "select-after-attachment")
    .header("if-match", format!("\"{}\"", default["version"].as_u64().unwrap()))
    .json(&json!({"workflow_id":workflow["id"],"workflow_version":2}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let issue_path = format!("{base}/issues/{}", issue["id"].as_str().unwrap());
  let task: Value = client.get(&issue_path).send().await.unwrap().json().await.unwrap();
  assert_eq!(task["workflow_id"], issue["workflow_id"]);
  assert_eq!(task["workflow_version"], issue["workflow_version"]);
  assert_eq!(task["status_id"], issue["status_id"]);
  let sync: Value = client
    .get(format!("{issue_path}/synchronization"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert!(
    sync["mapping"].is_null(),
    "attachment and workflow/default publication cannot enable a mapping"
  );
  let effects: Value = client
    .get(format!("{issue_path}/synchronization/projections"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    effects["items"],
    json!([]),
    "attachment and workflow/default publication cannot enqueue push"
  );
  let mapping = json!({"expected_mapping":null,"definition":{
    "source":source,"reporter":writer,"push_enabled":true,"fields":{"status":"octacity_owned_push"},
    "status":{"workflow":{"id":issue["workflow_id"],"version":issue["workflow_version"]},"pull":[],
      "push":{"statuses":[{"status":workflow["definition"]["transitions"][0]["to"],"add":["custom-ready"],"remove":[],"state":null}]}}
  }});
  let mut incompatible = mapping.clone();
  incompatible["definition"]["status"]["workflow"]["version"] = json!(2);
  let response = client
    .post(format!("{issue_path}/source-link/mappings"))
    .header("idempotency-key", "reject-implicit-default-adoption")
    .header("if-match", format!("\"{}\"", issue["version"].as_u64().unwrap()))
    .json(&incompatible)
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  mapping
}
