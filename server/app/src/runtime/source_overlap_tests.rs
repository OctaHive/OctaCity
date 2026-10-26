//! Two normal servers share the existing claims and canonical Issue identity at a real provider barrier.
use octacity_server_application::{FactoryExecutionClock as _, SystemFactoryExecutionClock};
use octacity_server_domain::ProjectId;
use octacity_server_store::IssueSourceReadScan;
use serde_json::{Value, json};

pub(super) async fn pause_retained_reads(client: &reqwest::Client, project: &str) {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let tasks: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  for task in tasks["items"].as_array().unwrap() {
    let path = format!("{base}/issues/{}", task["id"].as_str().unwrap());
    let sync: Value = client
      .get(format!("{path}/synchronization"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    if sync["link"].is_null() {
      continue;
    }
    let stopped = client
      .post(format!("{path}/synchronization/pull"))
      .header(
        "idempotency-key",
        format!("pause-for-overlap.{}", task["id"].as_str().unwrap()),
      )
      .json(&json!({"expected_generation":sync["observation"]["generation"],"enabled":false}))
      .send()
      .await
      .unwrap();
    assert_eq!(stopped.status(), reqwest::StatusCode::OK);
  }
}

pub(super) async fn assert_overlapping_imports(
  client: &reqwest::Client,
  pool: &sqlx::PgPool,
  project: &str,
  choices: &Value,
  provider: &std::path::Path,
) {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  let mut mode: Value = serde_json::from_slice(&original).unwrap();
  let mut native = mode["candidates"][0].clone();
  native["item_id"] = json!("overlap-native-task");
  native["display_id"] = json!("OVERLAP-1");
  native["title"] = json!("Overlapping native import");
  mode["candidates"].as_array_mut().unwrap().push(native);
  mode["pause_scan"] = json!(true);
  std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let previous = std::fs::read_to_string(provider.join("requests.jsonl"))
    .unwrap()
    .lines()
    .count();
  let mut intents = vec![];
  for index in 0..2 {
    let id = uuid::Uuid::new_v4().to_string();
    let intent = json!({"id":id,"project_id":project,"expected_current_version":null,"definition":{
      "source":choices["items"][0]["binding"],"connection":{"id":choices["connection_id"],"version":choices["connection_version"],
      "access_version":choices["access_version"],"source_configuration":choices["items"][0]["connection_configuration"],"reporter_configuration":null},"enabled":true}});
    let response = client
      .post(format!("{base}/issue-source-imports/{id}/versions"))
      .header("idempotency-key", format!("overlapping-import.{index}"))
      .header("if-none-match", "*")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    intents.push(intent);
  }
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
  loop {
    let requests = std::fs::read_to_string(provider.join("requests.jsonl")).unwrap();
    let started = requests
      .lines()
      .skip(previous)
      .map(|l| serde_json::from_str::<Value>(l).unwrap())
      .filter(|r| r["message"]["command"]["operation"] == "scan")
      .count();
    let scans = scans(pool, project).await;
    let claimed = intents.iter().all(|i| {
      scans.iter().any(|s| {
        s.claim
          .target
          .import
          .as_ref()
          .is_some_and(|c| c.id.to_string() == i["id"].as_str().unwrap())
      })
    });
    if started >= 2 && claimed {
      break;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "two servers must overlap real native scans under separate controls"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }
  let pending: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert!(
    !pending["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|i| i["title"] == "Overlapping native import")
  );
  std::fs::write(provider.join("release"), b"release").unwrap();
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
  loop {
    let scans = scans(pool, project).await;
    let now = SystemFactoryExecutionClock.now().unwrap();
    if intents.iter().all(|i| {
      scans.iter().any(|s| {
        s.claim
          .target
          .import
          .as_ref()
          .is_some_and(|c| c.id.to_string() == i["id"].as_str().unwrap())
          && s.claim.until <= now
          && s.schedule.as_ref().is_some_and(|v| v.failure.is_none())
      })
    }) {
      break;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "both original import pages must settle"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }
  let tasks: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    tasks["items"]
      .as_array()
      .unwrap()
      .iter()
      .filter(|i| i["title"] == "Overlapping native import")
      .count(),
    1,
    "overlapping controls and servers must retain one canonical Issue"
  );
  for (index, mut intent) in intents.into_iter().enumerate() {
    intent["expected_current_version"] = json!(1);
    intent["definition"]["enabled"] = json!(false);
    let response = client
      .post(format!(
        "{base}/issue-source-imports/{}/versions",
        intent["id"].as_str().unwrap()
      ))
      .header("idempotency-key", format!("stop-overlap.{index}"))
      .header("if-match", "\"1\"")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
  }
  std::fs::write(mode_path, original).unwrap();
}
async fn scans(pool: &sqlx::PgPool, project: &str) -> Vec<IssueSourceReadScan> {
  let rows: Vec<Vec<u8>> = sqlx::query_scalar("SELECT document FROM issue_source_reads WHERE project_id=$1")
    .bind(project.parse::<ProjectId>().unwrap().as_uuid())
    .fetch_all(pool)
    .await
    .unwrap();
  rows.into_iter().map(|b| serde_json::from_slice(&b).unwrap()).collect()
}

/// A stopped normal runtime leaves both discovery kinds available for one controlled pass.
pub(super) async fn assert_pass_budget(config: &crate::ServerConfig, pool: sqlx::PgPool, profiles: &std::path::Path) {
  let original = std::fs::read(profiles).unwrap();
  let mut document: Value = serde_json::from_slice(&original).unwrap();
  document["consumer"]["batch_size"] = json!(1);
  std::fs::write(profiles, serde_json::to_vec(&document).unwrap()).unwrap();
  let cancellation = tokio_util::sync::CancellationToken::new();
  let installed = super::integration_management::install(
    config,
    std::sync::Arc::new(octacity_server_store_postgres::PostgresStore::new(pool)),
    cancellation.clone(),
    &mut super::tests::healthy_checks(),
  )
  .await
  .unwrap()
  .unwrap();
  let worker = installed.source_reads.unwrap();
  for _ in 0..2 {
    assert_eq!(
      worker.run_once(cancellation.clone()).await.unwrap(),
      1,
      "one shared batch permits one target per pass across import and retained refresh"
    );
  }
  cancellation.cancel();
  std::fs::write(profiles, original).unwrap();
}

pub(super) async fn set_budget_import(client: &reqwest::Client, original: &Value, enabled: bool) {
  let mut intent = original.clone();
  let version = if enabled { 2 } else { 3 };
  intent["expected_current_version"] = json!(version);
  intent["definition"]["enabled"] = json!(enabled);
  let response = client
    .post(format!(
      "http://localhost/api/v1/projects/{}/issue-source-imports/{}/versions",
      intent["project_id"].as_str().unwrap(),
      intent["id"].as_str().unwrap()
    ))
    .header("idempotency-key", format!("batch-import.{enabled}"))
    .header("if-match", format!("\"{version}\""))
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
}
