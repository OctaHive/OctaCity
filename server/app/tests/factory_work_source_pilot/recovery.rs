//! Reopen durable Reporter state independently of an already accepted Flow result.
use super::*;
use octacity_server_store_postgres::PostgresStore;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pending_delivery_recovers_a_lost_reply_without_a_second_effect() {
  let (database, mut pilot, run) = admitted().await;
  let fixture = &mut pilot.fixture;
  let profile = &pilot.profile;
  let directory = fixture.root.path().join("github");
  std::fs::write(
    directory.join("reporter-mode.json"),
    br#"{"source_effects":true,"lose_project_response":true}"#,
  )
  .unwrap();
  let accepted = fixture.store.factory_run_snapshot(run).await.unwrap();
  let pending = status(&accepted);
  assert_eq!(pending.state, FactoryOutboxState::Pending);
  public_receipt(&database, run, pending).await;
  assert!(
    captured_requests(&directory).is_empty(),
    "accepted result is not a provider receipt"
  );
  assert_eq!(
    reporting_worker(fixture, managed_projector(fixture, profile), "before-restart", 10000)
      .run_once(time(120))
      .await
      .unwrap()
      .retrying,
    1
  );
  let uncertain = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(status(&uncertain).state, FactoryOutboxState::Pending);
  assert_eq!(uncertain.flow, accepted.flow);
  fixture.store = Arc::new(PostgresStore::new(database.pool.clone()));
  assert_eq!(fixture.store.factory_run_snapshot(run).await.unwrap(), uncertain);
  public_receipt(&database, run, status(&uncertain)).await;
  fixture.clock.0.store(220, Ordering::SeqCst);
  assert_eq!(
    reporting_worker(fixture, managed_projector(fixture, profile), "after-restart", 10000)
      .run_once(time(220))
      .await
      .unwrap()
      .delivered,
    1
  );
  let delivered = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(status(&delivered).state, FactoryOutboxState::Delivered);
  public_receipt(&database, run, status(&delivered)).await;
  assert_eq!(status(&delivered).operation_id, pending.operation_id);
  assert_eq!(delivered.work, accepted.work);
  assert_eq!(delivered.flow, accepted.flow);
  let writes = captured_requests(&directory)
    .lines()
    .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
    .filter(|call| call["message"]["command"]["operation"] == "apply")
    .count();
  assert_eq!(writes, 1);
  fixture.clock.0.store(10000, Ordering::SeqCst);
  assert_eq!(
    reporting_worker(fixture, managed_projector(fixture, profile), "settled", 10000)
      .run_once(time(10000))
      .await
      .unwrap()
      .claimed,
    0
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn failed_or_revoked_delivery_stays_distinct_from_the_accepted_result_after_reopen() {
  for reason in ["revoked", "permanent", "exhausted", "expired"] {
    let (database, mut pilot, run) = admitted().await;
    let fixture = &mut pilot.fixture;
    let directory = fixture.root.path().join("github");
    let accepted = fixture.store.factory_run_snapshot(run).await.unwrap();
    if reason == "expired" {
      fixture.clock.0.store(10000, Ordering::SeqCst);
    } else if reason == "revoked" {
      grant(
        fixture,
        pilot.connection.id,
        Some(pilot.connection.access_version),
        vec![IntegrationPurpose::TaskRead],
        "revoke-before-delivery",
      )
      .await;
    } else {
      let mode = if reason == "permanent" {
        serde_json::json!({"failure":{"class":"permanent","retry_after_millis":null}})
      } else {
        serde_json::json!({"failure":{"class":"transient","retry_after_millis":null}})
      };
      std::fs::write(directory.join("reporter-mode.json"), serde_json::to_vec(&mode).unwrap()).unwrap();
    }
    let first = reporting_worker(
      fixture,
      managed_projector(fixture, &pilot.profile),
      "first-failure",
      10000,
    )
    .run_once(time(120))
    .await
    .unwrap();
    if reason == "exhausted" {
      assert_eq!(first.retrying, 1, "{reason}: {first:?}");
    } else {
      assert_eq!(first.failed, 1, "{reason}: {first:?}");
    }
    let interrupted = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(interrupted.flow, accepted.flow);
    assert_ne!(status(&interrupted).state, FactoryOutboxState::Delivered);
    fixture.store = Arc::new(PostgresStore::new(database.pool.clone()));
    assert_eq!(fixture.store.factory_run_snapshot(run).await.unwrap(), interrupted);
    fixture.clock.0.store(220, Ordering::SeqCst);
    let retry = reporting_worker(
      fixture,
      managed_projector(fixture, &pilot.profile),
      "reopened-failure",
      10000,
    )
    .run_once(time(220))
    .await
    .unwrap();
    if reason == "exhausted" {
      assert_eq!(retry.failed, 1);
    } else {
      assert_eq!(retry.claimed, 0);
    }
    let failed = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(status(&failed).state, FactoryOutboxState::Failed);
    public_receipt(&database, run, status(&failed)).await;
    assert_eq!(status(&failed).operation_id, status(&accepted).operation_id);
    assert_eq!(failed.work, accepted.work);
    assert_eq!(failed.flow, accepted.flow);
    let calls = captured_requests(&directory);
    assert!(!calls.lines().any(
      |line| serde_json::from_str::<serde_json::Value>(line).unwrap()["message"]["command"]["operation"] == "apply"
    ));
    if reason == "revoked" {
      assert!(calls.is_empty(), "revocation is checked before invoking the provider");
      assert_eq!(status(&failed).failure, Some(FactoryOutboxFailure::Forbidden));
    }
    if ["exhausted", "expired"].contains(&reason) {
      assert_eq!(status(&failed).failure, Some(FactoryOutboxFailure::Exhausted));
    }
    if reason == "expired" {
      assert!(calls.is_empty(), "restart cannot renew the original delivery deadline");
    }
    fixture.clock.0.store(10000, Ordering::SeqCst);
    assert_eq!(
      reporting_worker(
        fixture,
        managed_projector(fixture, &pilot.profile),
        "after-exhaustion",
        10000
      )
      .run_once(time(10000))
      .await
      .unwrap()
      .claimed,
      0
    );
    database.cleanup().await;
  }
}

async fn admitted() -> (database::TestDatabase, Pilot<PostgresStore>, FactoryRunId) {
  let database = database::TestDatabase::migrated().await;
  let labels = ["approved", "rejected"];
  let (fixture, profile) = fixture_with_policy(
    "github",
    None,
    Some(WorkSourceReportingPolicy {
      ownership: None,
      labels: labels.iter().map(|label| (*label).into()).collect(),
      states: vec![],
    }),
    None,
  )
  .await;
  let fixture = fixture.into_postgres(database.pool.clone()).await;
  let pilot = pipeline_with_target(fixture, profile, &labels, None).await;
  let source = pilot
    .fixture
    .worker()
    .with_managed_connections(ManagedIntegrationConnections::new(pilot.fixture.store.clone()));
  assert_eq!(source.run_once(CancellationToken::new()).await.unwrap().completed, 2);
  drop(source);
  let run = pilot.fixture.runs().await[0].id;
  accept_status_at(
    &pilot.fixture,
    managed_projector(&pilot.fixture, &pilot.profile),
    run,
    100,
  )
  .await;
  (database, pilot, run)
}

fn status(snapshot: &FactoryRunSnapshot) -> &FactoryOutboxRecord {
  snapshot
    .outbox
    .iter()
    .filter(|row| row.kind.as_str() == "work.status.projection")
    .max_by_key(|row| row.order_key())
    .expect("accepted action retains one delivery operation")
}

/// A fresh normal Server instance must expose the retained delivery truth,
/// independently of the process peer and the reconstructed Reporter worker.
async fn public_receipt(database: &database::TestDatabase, run: FactoryRunId, expected: &FactoryOutboxRecord) {
  let root = tempfile::tempdir().unwrap();
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let server = ServerRuntime::start(runtime::runtime_config(
    root.path(),
    url.as_str(),
    &std::env::var("OCTACITY_MINIO_ENDPOINT").unwrap(),
  ))
  .await
  .unwrap();
  let response = reqwest::Client::new()
    .get(format!(
      "http://{}/api/v1/factory-runs/{run}/effect-delivery?limit=100",
      server.management_addr()
    ))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap();
  let page: serde_json::Value = response.json().await.unwrap();
  let rows: Vec<_> = page["items"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|row| row["record"]["kind"] == "work.status.projection")
    .collect();
  assert_eq!(
    rows.len(),
    1,
    "one logical reporting operation despite lost replies and restarts"
  );
  assert_eq!(rows[0]["record"]["id"], expected.operation_id.to_string());
  assert_eq!(
    rows[0]["record"]["state"],
    serde_json::to_value(expected.state).unwrap()
  );
  assert_eq!(
    rows[0]["record"]["failure"],
    serde_json::to_value(expected.failure).unwrap()
  );
  server.shutdown().await.unwrap();
}
