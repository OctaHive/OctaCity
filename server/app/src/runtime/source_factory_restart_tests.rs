//! Actual managed admission is replayed after a committed Work loses its page checkpoint.
use super::ServerRuntime;
use octacity_server_store::{FactorySourceStore as _, WorkSourceArtifactStore as _};
use octacity_server_store_postgres::PostgresStore;
use serde_json::Value;

pub(super) async fn interrupt_admission_checkpoint(pool: &sqlx::PgPool) {
  // The admission transaction commits before the next page checkpoint. Leave selection,
  // claim and schedule writes available so only acknowledgement of that effect is lost.
  sqlx::query(
    r#"CREATE FUNCTION test_admission_checkpoint() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN
      IF NEW.kind='input' AND NEW.document->'progress' IS DISTINCT FROM OLD.document->'progress'
        AND EXISTS(SELECT 1 FROM factory_source_preparations
          WHERE project_id=NEW.project_id AND input_id=NEW.id AND settled) THEN
        RAISE EXCEPTION 'controlled checkpoint interruption after admission';
      END IF;
      RETURN NEW;
    END $$"#,
  )
  .execute(pool)
  .await
  .unwrap();
  sqlx::query("CREATE TRIGGER test_admission_checkpoint BEFORE UPDATE ON factory_source_scans FOR EACH ROW EXECUTE FUNCTION test_admission_checkpoint()")
    .execute(pool).await.unwrap();
}

pub(super) async fn assert_admission_restart(
  runtime: ServerRuntime,
  config: &crate::ServerConfig,
  client: &reqwest::Client,
  pool: &sqlx::PgPool,
  project: &str,
  diagnostics_path: &str,
  provider: &std::path::Path,
) -> ServerRuntime {
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
  let before = loop {
    let diagnostics: Value = client.get(diagnostics_path).send().await.unwrap().json().await.unwrap();
    if diagnostics["admissions"].as_array().is_some_and(|a| a.len() == 1)
      && diagnostics["scan"]["schedule"]["failure"] == "transient"
    {
      break diagnostics;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "the admitted Work must lose its checkpoint: {diagnostics}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  };
  let association = before["admissions"][0]["association"].clone();
  let project = project.parse().unwrap();
  let work = association["work_id"].as_str().unwrap().parse().unwrap();
  let store = PostgresStore::new(pool.clone());
  let preparation = store.source_preparation_for_work(project, work).await.unwrap().unwrap();
  let materialization = store
    .source_materialization(project, preparation.id)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(
    materialization.objects.len(),
    2,
    "only the reviewed task and acceptance objects are published"
  );
  assert!(materialization.published_at.is_some());
  let snapshot: Value = serde_json::from_slice(&preparation.snapshot).unwrap();
  assert_eq!(snapshot["body"], "Reviewed external acceptance criteria");
  runtime.shutdown().await.unwrap();
  sqlx::query("DROP TRIGGER test_admission_checkpoint ON factory_source_scans")
    .execute(pool)
    .await
    .unwrap();
  sqlx::query("DROP FUNCTION test_admission_checkpoint()")
    .execute(pool)
    .await
    .unwrap();
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  std::fs::write(
    &mode_path,
    br#"{"failure":{"class":"transient","retry_after_millis":1000}}"#,
  )
  .unwrap();
  let runtime = ServerRuntime::start(config.clone()).await.unwrap();
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
  loop {
    let diagnostics: Value = client.get(diagnostics_path).send().await.unwrap().json().await.unwrap();
    if let Some(admissions) = diagnostics["admissions"].as_array().filter(|a| a.len() == 2) {
      assert_eq!(
        admissions.iter().filter(|a| a["association"] == association).count(),
        1,
        "restart must retain the original input/Work/Run identity"
      );
      for admission in admissions {
        let work = admission["association"]["work_id"].as_str().unwrap().parse().unwrap();
        let retained = store.source_preparation_for_work(project, work).await.unwrap().unwrap();
        let snapshot: Value = serde_json::from_slice(&retained.snapshot).unwrap();
        assert_eq!(snapshot["body"], "Reviewed external acceptance criteria");
      }
      break;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "restart must admit the retained tail without a fresh native read: {diagnostics}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }
  let retained = store.source_preparation_for_work(project, work).await.unwrap().unwrap();
  assert_eq!(retained.id, preparation.id);
  assert_eq!(retained.snapshot, preparation.snapshot);
  assert_eq!(retained.intent, preparation.intent);
  assert_eq!(retained.admission, preparation.admission);
  assert_eq!(
    store.source_materialization(project, retained.id).await.unwrap(),
    Some(materialization),
    "restart cannot republish original objects or renew their retention"
  );
  assert!(!provider.join("reporter-requests.jsonl").exists());
  std::fs::write(mode_path, original).unwrap();
  runtime
}
