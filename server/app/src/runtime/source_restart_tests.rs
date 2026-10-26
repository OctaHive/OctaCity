//! Normal startup resumes a retained page after an effect commits but its checkpoint fails.
use super::ServerRuntime;
use serde_json::{Value, json};

pub(super) async fn assert_import_restart(
  runtime: ServerRuntime,
  config: &crate::ServerConfig,
  client: &reqwest::Client,
  pool: &sqlx::PgPool,
  project: &str,
  choices: &Value,
  provider: &std::path::Path,
) -> ServerRuntime {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let id = uuid::Uuid::new_v4().to_string();
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  let mut mode: Value = serde_json::from_slice(&original).unwrap();
  let mut first = mode["candidates"][0].clone();
  first["item_id"] = json!("restart-import-one");
  first["title"] = json!("Original first task before restart");
  let mut second = first.clone();
  second["item_id"] = json!("restart-import-two");
  second["title"] = json!("Original remaining task before restart");
  mode["candidates"] = json!([first, second]);
  std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();

  // Fault only progress writes after the first canonical link exists. Claim/schedule writes
  // still work, so shutdown can drain normally while the retained page remains unacknowledged.
  sqlx::query(
    r#"CREATE FUNCTION test_import_checkpoint() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN
      IF convert_from(NEW.document,'UTF8')::jsonb #>> '{claim,target,import,id}' IS NOT NULL
        AND convert_from(NEW.document,'UTF8')::jsonb->'progress'
          IS DISTINCT FROM convert_from(OLD.document,'UTF8')::jsonb->'progress'
        AND EXISTS(SELECT 1 FROM issue_source_links WHERE project_id=NEW.project_id
          AND document->'selection'->'item'->>'item'='restart-import-one') THEN
        RAISE EXCEPTION 'controlled checkpoint interruption after canonical import';
      END IF;
      RETURN NEW;
    END $$"#,
  )
  .execute(pool)
  .await
  .unwrap();
  sqlx::query("CREATE TRIGGER test_import_checkpoint BEFORE UPDATE ON issue_source_reads FOR EACH ROW EXECUTE FUNCTION test_import_checkpoint()")
    .execute(pool).await.unwrap();
  let mut intent = json!({"id":id,"project_id":project,"expected_current_version":null,"definition":{
    "source":choices["items"][0]["binding"],"connection":{"id":choices["connection_id"],"version":choices["connection_version"],
      "access_version":choices["access_version"],"source_configuration":choices["items"][0]["connection_configuration"],"reporter_configuration":null},"enabled":true}});
  let path = format!("{base}/issue-source-imports/{id}/versions");
  let response = client
    .post(&path)
    .header("idempotency-key", "import-before-checkpoint-interruption")
    .header("if-none-match", "*")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
  loop {
    let scans = super::source_overlap_tests::scans(pool, project).await;
    if scans.iter().any(|s| {
      s.claim.target.import.as_ref().is_some_and(|i| i.id.to_string() == id)
        && s
          .schedule
          .as_ref()
          .is_some_and(|s| s.failure == Some(octacity_server_store::WorkSourceFailure::Transient))
    }) {
      break;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "the post-import checkpoint fault must be reached"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }
  let before: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let first = before["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|i| i["title"] == "Original first task before restart")
    .unwrap();
  let first_id = first["id"].clone();
  assert!(
    !before["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|i| i["title"] == "Original remaining task before restart")
  );
  runtime.shutdown().await.unwrap();
  sqlx::query("DROP TRIGGER test_import_checkpoint ON issue_source_reads")
    .execute(pool)
    .await
    .unwrap();
  sqlx::query("DROP FUNCTION test_import_checkpoint()")
    .execute(pool)
    .await
    .unwrap();
  // Any reread now fails. Recovery can finish only from the original retained snapshots.
  std::fs::write(
    &mode_path,
    br#"{"failure":{"class":"transient","retry_after_millis":1000}}"#,
  )
  .unwrap();
  let runtime = ServerRuntime::start(config.clone()).await.unwrap();
  super::source_import_tests::wait_for_task_title(
    client,
    &base,
    first_id.as_str().unwrap(),
    "Original first task before restart",
  )
  .await;
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
  loop {
    let tasks: Value = client
      .get(format!("{base}/issues"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    let items = tasks["items"].as_array().unwrap();
    if items
      .iter()
      .any(|i| i["title"] == "Original remaining task before restart")
    {
      let first: Vec<_> = items
        .iter()
        .filter(|i| i["title"] == "Original first task before restart")
        .collect();
      assert_eq!(
        first.len(),
        1,
        "restart must not duplicate the committed canonical Issue"
      );
      assert_eq!(first[0]["id"], first_id);
      assert_eq!(
        items
          .iter()
          .filter(|i| i["title"] == "Original remaining task before restart")
          .count(),
        1
      );
      break;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "restart must consume the retained page despite failed new reads"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }
  intent["expected_current_version"] = json!(1);
  intent["definition"]["enabled"] = json!(false);
  let response = client
    .post(&path)
    .header("idempotency-key", "stop-recovered-import")
    .header("if-match", "\"1\"")
    .json(&json!({"intent":intent}))
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
  assert!(!provider.join("reporter-requests.jsonl").exists());
  std::fs::write(mode_path, original).unwrap();
  runtime
}
