//! Current grants fence actual in-flight import independently of manual task creation.
use serde_json::{Value, json};

pub(super) enum ScanBlock {
  UntilReleased,
  UntilCancelled,
}

pub(super) struct PausedImport {
  intent: Value,
  original: Vec<u8>,
  runs: Value,
}

pub(super) async fn pause_import(
  client: &reqwest::Client,
  pool: &sqlx::PgPool,
  template: &Value,
  provider: &std::path::Path,
  block: ScanBlock,
) -> PausedImport {
  let project = template["project_id"].as_str().unwrap();
  let base = format!("http://localhost/api/v1/projects/{project}");
  super::source_overlap_tests::pause_retained_reads(client, project).await;
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
  for name in ["release", "cancelled"] {
    let marker = provider.join(name);
    if marker.exists() {
      std::fs::remove_file(marker).unwrap();
    }
  }
  let mut mode: Value = serde_json::from_slice(&original).unwrap();
  mode["candidates"][0]["item_id"] = json!("grant-revoked-in-flight");
  mode["candidates"][0]["title"] = json!("An interrupted read cannot import this task");
  let (flag, key) = match block {
    ScanBlock::UntilReleased => ("pause_scan", "source-revocation"),
    ScanBlock::UntilCancelled => ("hold", "source-shutdown"),
  };
  mode[flag] = json!(true);
  std::fs::write(mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let mut intent = template.clone();
  intent["id"] = json!(uuid::Uuid::new_v4().to_string());
  intent["expected_current_version"] = Value::Null;
  intent["definition"]["enabled"] = json!(true);
  let response = client
    .post(format!(
      "{base}/issue-source-imports/{}/versions",
      intent["id"].as_str().unwrap()
    ))
    .header("idempotency-key", format!("import-before-{key}"))
    .header("if-none-match", "*")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
  loop {
    let scans = super::source_overlap_tests::scans(pool, project).await;
    let scan_id = scans
      .iter()
      .find(|s| {
        s.claim
          .target
          .import
          .as_ref()
          .is_some_and(|i| i.id.to_string() == intent["id"].as_str().unwrap())
      })
      .and_then(|s| s.progress.as_ref())
      .and_then(|p| p.polling.as_ref())
      .map(|p| p.scan_id.to_string());
    let requests = std::fs::read_to_string(provider.join("requests.jsonl")).unwrap();
    if scan_id.as_deref().is_some_and(|id| {
      requests
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .any(|r| r["message"]["command"]["operation"] == "scan" && r["message"]["command"]["payload"]["scan_id"] == id)
    }) {
      break;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "{key} must reach its native scan; expected={scan_id:?}, claims={:?}",
      scans
        .iter()
        .map(|s| (
          s.claim.target.import.as_ref().map(|i| i.id),
          s.schedule.as_ref().map(|s| s.failure),
          s.progress.as_ref().and_then(|p| p.polling.as_ref()).map(|p| p.scan_id),
        ))
        .collect::<Vec<_>>()
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }
  let manual = client
    .post(format!("{base}/issues"))
    .header("idempotency-key", format!("manual-during-{key}"))
    .json(&json!({"title":"Manual task while provider is stalled","description":"","acceptance":null,"priority":null}))
    .send()
    .await
    .unwrap();
  assert_eq!(manual.status(), reqwest::StatusCode::CREATED);
  // A stalled first consumer pass may be unready; ordinary CI/manual tasks remain available.
  let ready = client.get("http://localhost/health/ready").send().await.unwrap();
  assert_eq!(ready.status(), reqwest::StatusCode::OK);
  PausedImport { intent, original, runs }
}

impl PausedImport {
  pub(super) async fn assert_revoked(
    mut self,
    client: &reqwest::Client,
    pool: &sqlx::PgPool,
    provider: &std::path::Path,
  ) {
    let project = self.intent["project_id"].as_str().unwrap();
    let base = format!("http://localhost/api/v1/projects/{project}");
    std::fs::write(provider.join("release"), b"release").unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
      let scans = super::source_overlap_tests::scans(pool, project).await;
      if scans.iter().any(|s| {
        s.claim
          .target
          .import
          .as_ref()
          .is_some_and(|i| i.id.to_string() == self.intent["id"].as_str().unwrap())
          && s.schedule.as_ref().is_some_and(|s| s.failure.is_some())
      }) {
        break;
      }
      assert!(
        tokio::time::Instant::now() < deadline,
        "current read revocation must settle the in-flight scan safely"
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
    assert!(
      !tasks["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["title"] == "An interrupted read cannot import this task")
    );
    assert!(
      tasks["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["title"] == "Manual task while provider is stalled")
    );
    let runs: Value = client
      .get(format!("{base}/factory-runs"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    assert_eq!(runs, self.runs, "revocation cannot rewrite accepted execution history");
    self.intent["expected_current_version"] = json!(1);
    self.intent["definition"]["enabled"] = json!(false);
    let response = client
      .post(format!(
        "{base}/issue-source-imports/{}/versions",
        self.intent["id"].as_str().unwrap()
      ))
      .header("idempotency-key", "disable-import-after-in-flight-revocation")
      .header("if-match", "\"1\"")
      .json(&json!({"intent":self.intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(!provider.join("reporter-requests.jsonl").exists());
    std::fs::write(provider.join("mode.json"), self.original).unwrap();
  }
}

pub(super) async fn assert_supervised_shutdown(
  runtime: super::ServerRuntime,
  config: &crate::ServerConfig,
  client: &reqwest::Client,
  pool: &sqlx::PgPool,
  template: &Value,
  provider: &std::path::Path,
) -> super::ServerRuntime {
  let project = template["project_id"].as_str().unwrap();
  let base = format!("http://localhost/api/v1/projects/{project}");
  let connection = &template["definition"]["connection"];
  let choices: Value = client
    .get(format!(
      "{base}/issue-source-choices?connection_id={}&connection_version={}",
      connection["id"].as_str().unwrap(),
      connection["version"].as_u64().unwrap()
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let mut current = template.clone();
  current["definition"]["source"] = choices["items"][0]["binding"].clone();
  current["definition"]["connection"]["access_version"] = choices["access_version"].clone();
  current["definition"]["connection"]["source_configuration"] = choices["items"][0]["connection_configuration"].clone();
  let mut paused = pause_import(client, pool, &current, provider, ScanBlock::UntilCancelled).await;
  tokio::time::timeout(std::time::Duration::from_secs(5), runtime.shutdown())
    .await
    .expect("normal shutdown must bound a stalled native read")
    .unwrap();
  let cancellation = std::fs::read_to_string(provider.join("cancelled")).unwrap();
  // This fixture advertises cancel=false: EOF is the supported stop signal.
  assert!(
    cancellation.is_empty(),
    "the host must close and drain the verified process before returning"
  );
  let scans = super::source_overlap_tests::scans(pool, project).await;
  let scan = scans
    .iter()
    .find(|s| {
      s.claim
        .target
        .import
        .as_ref()
        .is_some_and(|i| i.id.to_string() == paused.intent["id"].as_str().unwrap())
    })
    .unwrap();
  assert!(
    scan.schedule.as_ref().is_none_or(|s| s.attempts == 0 && !s.exhausted),
    "operator shutdown cannot spend the provider failure budget: {:?}",
    scan.schedule
  );
  std::fs::write(
    provider.join("mode.json"),
    br#"{"failure":{"class":"transient","retry_after_millis":1000}}"#,
  )
  .unwrap();
  let runtime = super::ServerRuntime::start(config.clone()).await.unwrap();
  paused.intent["expected_current_version"] = json!(1);
  paused.intent["definition"]["enabled"] = json!(false);
  let response = client
    .post(format!(
      "{base}/issue-source-imports/{}/versions",
      paused.intent["id"].as_str().unwrap()
    ))
    .header("idempotency-key", "disable-import-after-shutdown")
    .header("if-match", "\"1\"")
    .json(&json!({"intent":paused.intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let tasks: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert!(
    !tasks["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|i| i["title"] == "An interrupted read cannot import this task")
  );
  let runs: Value = client
    .get(format!("{base}/factory-runs"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(runs, paused.runs);
  assert!(!provider.join("reporter-requests.jsonl").exists());
  std::fs::write(provider.join("mode.json"), paused.original).unwrap();
  runtime
}
