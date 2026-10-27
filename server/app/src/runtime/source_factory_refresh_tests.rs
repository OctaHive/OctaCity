//! Retained Issue pull stays independent of execution selectors and input lifecycle.
use serde_json::{Value, json};

pub(super) async fn assert_retained_pull_survives_input_mismatch_and_disablement(
  client: &reqwest::Client,
  project: &str,
  published: &Value,
  imported: &Value,
  provider: &std::path::Path,
) {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let issue = imported["issue"]["id"].as_str().unwrap();
  let issue_path = format!("{base}/issues/{issue}");
  let before: Value = client.get(&issue_path).send().await.unwrap().json().await.unwrap();
  let runs: Value = client
    .get(format!("{base}/factory-runs"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  let mut mode: Value = serde_json::from_slice(&original).unwrap();
  mode["candidates"][0]["state"] = json!("native-state");
  mode["candidates"][0]["labels"] = json!(["reported-triage-label"]);
  mode["candidates"][0]["title"] = json!("Refreshed outside the execution selector");
  mode["candidates"][0]["body"] = json!("Source owned description after the selector mismatch");
  std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let id = uuid::Uuid::new_v4().to_string();
  let mut intent = published["intent"].clone();
  intent["inputs"][0]["id"] = json!(id);
  intent["inputs"][0]["definition"]["enabled"] = json!(true);
  intent["inputs"][0]["definition"]["predicate"] =
    json!({"state":"native-state","all_of":[],"any_of":[],"none_of":[],"label_presence":"none"});
  let path = format!("{base}/factory-source-inputs/{id}/versions");
  let response = client
    .post(&path)
    .header("idempotency-key", "input-before-retained-refresh")
    .header("if-none-match", "*")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let diagnostics =
    super::source_factory_predicate_tests::wait_for_selection(client, &format!("{path}/1/diagnostics")).await;
  assert_eq!(diagnostics["scan"]["selection"]["matching_inputs"], json!([]));
  set_pull(client, &issue_path, true).await;
  super::source_import_tests::wait_for_task_title(client, &base, issue, "Refreshed outside the execution selector")
    .await;
  let refreshed: Value = client.get(&issue_path).send().await.unwrap().json().await.unwrap();
  assert_eq!(
    refreshed["content"]["description"],
    "Source owned description after the selector mismatch"
  );
  assert_eq!(refreshed["content"]["acceptance"], before["content"]["acceptance"]);
  assert_eq!(
    refreshed["status_id"], before["status_id"],
    "a native state cannot invent a business transition"
  );

  intent["inputs"][0]["expected_version"] = json!(1);
  intent["inputs"][0]["definition"]["enabled"] = json!(false);
  let response = client
    .post(&path)
    .header("idempotency-key", "disable-input-before-retained-refresh")
    .header("if-match", "\"1\"")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  // Reuse the opaque native update token: bounded content provenance must detect this change.
  mode["candidates"][0]["title"] = json!("Refreshed after execution was disabled");
  std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
  super::source_import_tests::wait_for_task_title(client, &base, issue, "Refreshed after execution was disabled").await;
  let sync: Value = client
    .get(format!("{issue_path}/synchronization"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(sync["observation"]["enabled"], true);
  let projections: Value = client
    .get(format!("{issue_path}/synchronization/projections"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    projections["items"],
    json!([]),
    "source origin refresh cannot echo an outbound status"
  );
  let after: Value = client
    .get(format!("{base}/factory-runs"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(after, runs, "independent refresh cannot create or modify execution");
  assert!(!provider.join("reporter-requests.jsonl").exists());
  set_pull(client, &issue_path, false).await;
  std::fs::write(mode_path, original).unwrap();
}

async fn set_pull(client: &reqwest::Client, issue_path: &str, enabled: bool) {
  let sync: Value = client
    .get(format!("{issue_path}/synchronization"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let response = client
    .post(format!("{issue_path}/synchronization/pull"))
    .header("idempotency-key", format!("factory-independent-pull.{enabled}"))
    .json(&json!({"expected_generation":sync["observation"]["generation"],"enabled":enabled}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
}
