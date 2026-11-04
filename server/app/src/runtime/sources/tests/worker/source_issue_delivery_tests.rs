//! Accepted manual business transitions reach the shared ordered Reporter without Factory intake.
use super::*;

#[path = "source_issue_recovery_tests.rs"]
mod recovery;

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn normal_issue_status_delivery_applies_reviewed_native_effects_without_a_run() {
  let IssueProject {
    fixture,
    database,
    deployment,
    config,
    runtime,
    client,
    base,
    source,
    issue,
  } = start_issue_project().await;
  runtime.shutdown().await.unwrap();
  let profiles_path = deployment.path().join("shared-source-profiles.json");
  let mut profiles: Value = serde_json::from_slice(&std::fs::read(&profiles_path).unwrap()).unwrap();
  profiles["reporters"][0]["reporting"]["editable"] =
    json!({"labels":["arbitrary","custom-ready"],"states":["native-closed"]});
  std::fs::write(&profiles_path, serde_json::to_vec(&profiles).unwrap()).unwrap();
  let directory = fixture.root.path().join("opaque-writer");
  let mut native: Value = serde_json::from_slice(&std::fs::read(directory.join("mode.json")).unwrap()).unwrap();
  native["candidates"][0]["labels"] = json!(["arbitrary", "human-owned"]);
  std::fs::write(directory.join("mode.json"), serde_json::to_vec(&native).unwrap()).unwrap();
  std::fs::write(directory.join("reporter-mode.json"), br#"{"source_effects":true}"#).unwrap();
  let runtime = crate::ServerRuntime::start(config).await.unwrap();
  let issue_path = format!("{base}/issues/{}", issue["id"].as_str().unwrap());
  let choices = get(&client, &format!("{issue_path}/source-choices")).await;
  let workflow = get(
    &client,
    &format!(
      "{base}/issue-workflows/{}/versions/{}",
      issue["workflow_id"].as_str().unwrap(),
      issue["workflow_version"]
    ),
  )
  .await;
  let edge = &workflow["definition"]["transitions"][0];
  let effects = json!({"status":edge["to"],"add":["custom-ready"],"remove":["arbitrary"],"state":"native-closed"});
  let definition = json!({"source":source["binding"],"reporter":choices["items"][0]["writer"]["binding"],
    "push_enabled":true,"fields":{"status":"octacity_owned_push"},"status":{
      "workflow":{"id":issue["workflow_id"],"version":issue["workflow_version"]},"pull":[],
      "push":{"statuses":[effects]}}});
  let response = client
    .post(format!("{issue_path}/source-link/mappings"))
    .header("idempotency-key", "review-delivery")
    .header("if-match", format!("\"{}\"", issue["version"].as_u64().unwrap()))
    .json(&json!({"expected_mapping":null,"definition":definition}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::CREATED,
    "{}",
    response.text().await.unwrap()
  );
  let mapping: Value = response.json().await.unwrap();
  assert!(
    captured_requests(&directory).is_empty(),
    "review itself must not dispatch"
  );
  let accepted = transition(&client, &issue_path, &issue, edge, "manual-business-move").await;
  assert_eq!(accepted["status_id"], edge["to"]);
  let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
  let receipt = loop {
    let page = get(&client, &format!("{issue_path}/synchronization/projections")).await;
    assert!(
      page["items"].as_array().unwrap().len() <= 1,
      "one accepted transition has one logical projection"
    );
    if page["items"][0]["state"] == "delivered" {
      break page["items"][0].clone();
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "normal Issue delivery must settle: {page}"
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
  };
  assert_eq!(receipt["effects"], effects);
  assert_eq!(receipt["mapping"], mapping["mapping_reference"]);
  assert!(receipt["pipeline"].is_null());
  let native: Value = serde_json::from_slice(&std::fs::read(directory.join("mode.json")).unwrap()).unwrap();
  assert_eq!(
    native["candidates"][0]["labels"],
    json!(["custom-ready", "human-owned"])
  );
  assert_eq!(native["candidates"][0]["state"], "native-closed");
  let captured = captured_requests(&directory);
  let calls: Vec<Value> = captured
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
  let operations: Vec<_> = calls
    .iter()
    .map(|c| c["message"]["command"]["operation"].as_str().unwrap())
    .collect();
  assert_eq!(operations, ["inspect", "inspect", "apply", "inspect"]);
  for call in &calls {
    let intent = original_intent(call);
    assert_eq!(intent["projection_id"], receipt["operation_id"]);
    assert_eq!(intent["resource"]["resource_id"], "native-container");
    assert_eq!(intent["item_id"], "TASK-native");
    assert_eq!(intent["mapping"]["identity"], "original-effects");
    assert_eq!(
      intent["effects"],
      json!({"add":["custom-ready"],"remove":["arbitrary"],"state":"native-closed"})
    );
  }
  let response = client
    .post(format!("{issue_path}/synchronization/refresh"))
    .header("idempotency-key", "source-echo-after-delivery")
    .json(&json!({"source":source["binding"],"continuation":null}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let after = get(&client, &issue_path).await;
  assert_eq!(after["version"], accepted["version"]);
  assert_eq!(after["status_id"], accepted["status_id"]);
  let page = get(&client, &format!("{issue_path}/synchronization/projections")).await;
  assert_eq!(page["items"], json!([receipt]));
  assert_eq!(
    captured_requests(&directory),
    captured,
    "source echo cannot enqueue another push"
  );
  assert_eq!(get(&client, &format!("{base}/factory-runs")).await["items"], json!([]));
  assert_eq!(
    get(&client, &format!("{base}/factory-configurations")).await["items"],
    json!([])
  );
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}

async fn get(client: &reqwest::Client, url: &str) -> Value {
  let response = client.get(url).send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{url}: {}",
    response.text().await.unwrap()
  );
  response.json().await.unwrap()
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn normal_issue_read_only_pull_and_unmapped_manual_moves_emit_no_reporter_effects() {
  let IssueProject {
    fixture,
    database,
    deployment: _deployment,
    runtime,
    client,
    base,
    source,
    issue,
    ..
  } = start_issue_project().await;
  let path = format!("{base}/issues/{}", issue["id"].as_str().unwrap());
  let workflow = get(
    &client,
    &format!(
      "{base}/issue-workflows/{}/versions/{}",
      issue["workflow_id"].as_str().unwrap(),
      issue["workflow_version"]
    ),
  )
  .await;
  let edges = workflow["definition"]["transitions"].as_array().unwrap();
  let manual = transition(&client, &path, &issue, &edges[0], "unmapped-local-move").await;
  assert_eq!(manual["status_id"], edges[0]["to"]);
  assert_eq!(
    get(&client, &format!("{path}/synchronization/projections")).await["items"],
    json!([])
  );
  let connection = fixture.input.definition().connection.as_ref().unwrap();
  let response = client
    .post(format!(
      "http://localhost/api/v1/integrations/{}/projects/{}/access",
      connection.id,
      issue["project_id"].as_str().unwrap()
    ))
    .header("idempotency-key", "source-owned-read-only")
    .header("if-match", "\"1\"")
    .json(
      &json!({"intent":{"connection_id":connection.id,"connection_version":connection.version,
      "project_id":issue["project_id"],"expected_current_version":1,
      "resources":[{"reference":source["binding"]["resource"],"purposes":["task_read"]}]}}),
    )
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let choices = get(&client, &format!("{path}/source-choices")).await;
  assert!(choices["items"][0]["writer"].is_null());
  let edge = edges.iter().find(|e| e["from"] == manual["status_id"]).unwrap();
  let response = client.post(format!("{path}/source-link/mappings"))
    .header("idempotency-key","review-incoming-status").header("if-match",format!("\"{}\"",manual["version"].as_u64().unwrap()))
    .json(&json!({"expected_mapping":null,"definition":{"source":source["binding"],"reporter":null,
      "push_enabled":false,"fields":{"status":"source_owned_pull"},"status":{
        "workflow":{"id":issue["workflow_id"],"version":issue["workflow_version"]},
        "pull":[{"native":{"state":"native-state","all_of":[],"any_of":[],"none_of":[]},"status_id":edge["to"]}],"push":null}}}))
    .send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::CREATED,
    "{}",
    response.text().await.unwrap()
  );
  let mut version = Value::Null;
  let mut previous_observation = Value::Null;
  for echo in [false, true] {
    // Observe the normal consumer's retained outcome. An explicit refresh may
    // yield to its claim and is not required to win a race against that worker.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let observation = loop {
      let sync = get(&client, &format!("{path}/synchronization")).await;
      let status = &sync["observation"]["status"];
      let condition = status["condition"].as_str();
      let id = &status["provenance"]["observation"];
      if matches!(condition, Some("applied" | "echo"))
        && !id.is_null()
        && (!echo || (condition == Some("echo") && *id != previous_observation))
      {
        break id.clone();
      }
      assert!(
        tokio::time::Instant::now() < deadline,
        "normal pull must reconcile status and a later echo: {sync}"
      );
      tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let current = get(&client, &path).await;
    assert_eq!(current["status_id"], edge["to"]);
    if version.is_null() {
      version = current["version"].clone();
    }
    assert_eq!(
      current["version"], version,
      "source echo cannot create another revision"
    );
    previous_observation = observation;
  }
  assert_eq!(
    get(&client, &format!("{path}/synchronization/projections")).await["items"],
    json!([])
  );
  let history = get(&client, &format!("{path}/status-history")).await;
  assert!(
    history["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|entry| entry["origin"] == "source" && entry["to"] == edge["to"])
  );
  assert!(captured_requests(&fixture.root.path().join("opaque-writer")).is_empty());
  assert_eq!(get(&client, &format!("{base}/factory-runs")).await["items"], json!([]));
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn normal_issue_delivery_failure_preserves_the_accepted_local_status() {
  let IssueProject {
    fixture,
    database,
    deployment: _deployment,
    runtime,
    client,
    base,
    source,
    issue,
    ..
  } = start_issue_project().await;
  let path = format!("{base}/issues/{}", issue["id"].as_str().unwrap());
  let directory = fixture.root.path().join("opaque-writer");
  std::fs::write(
    directory.join("reporter-mode.json"),
    br#"{"failure":{"class":"unsupported","retry_after_millis":null}}"#,
  )
  .unwrap();
  let choices = get(&client, &format!("{path}/source-choices")).await;
  let workflow = get(
    &client,
    &format!(
      "{base}/issue-workflows/{}/versions/{}",
      issue["workflow_id"].as_str().unwrap(),
      issue["workflow_version"]
    ),
  )
  .await;
  let edge = &workflow["definition"]["transitions"][0];
  let response = client
    .post(format!("{path}/source-link/mappings"))
    .header("idempotency-key", "review-failing-delivery")
    .header("if-match", format!("\"{}\"", issue["version"].as_u64().unwrap()))
    .json(
      &json!({"expected_mapping":null,"definition":{"source":source["binding"],
      "reporter":choices["items"][0]["writer"]["binding"],"push_enabled":true,"fields":{"status":"octacity_owned_push"},
      "status":{"workflow":{"id":issue["workflow_id"],"version":issue["workflow_version"]},"pull":[],
        "push":{"statuses":[{"status":edge["to"],"add":["custom-ready"],"remove":[],"state":null}]}}}}),
    )
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CREATED);
  let accepted = transition(&client, &path, &issue, edge, "accepted-before-provider-failure").await;
  let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
  loop {
    let page = get(&client, &format!("{path}/synchronization/projections")).await;
    if page["items"][0]["state"] == "failed" {
      assert_eq!(page["items"][0]["failure"], "unsupported");
      assert_eq!(page["items"][0]["status_id"], accepted["status_id"]);
      break;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "safe failed delivery must remain visible: {page}"
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  let retained = get(&client, &path).await;
  assert_eq!(retained["version"], accepted["version"]);
  assert_eq!(retained["status_id"], edge["to"]);
  assert!(!directory.join("reporter-applied").exists());
  let calls: Vec<Value> = captured_requests(&directory)
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
  assert_eq!(calls.len(), 1);
  assert_eq!(calls[0]["message"]["command"]["operation"], "inspect");
  assert_eq!(get(&client, &format!("{base}/factory-runs")).await["items"], json!([]));
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}

async fn transition(client: &reqwest::Client, path: &str, issue: &Value, edge: &Value, key: &str) -> Value {
  let response = client
    .post(format!("{path}/transitions"))
    .header("idempotency-key", key)
    .header("if-match", format!("\"{}\"", issue["version"].as_u64().unwrap()))
    .json(
      &json!({"workflow_id":issue["workflow_id"],"workflow_version":issue["workflow_version"],
      "expected_status_id":issue["status_id"],"transition_id":edge["id"],"reason":null}),
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
  response.json::<Value>().await.unwrap()["resource"].clone()
}
