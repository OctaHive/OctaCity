//! Normal startup resumes retained Work effects using original pins and current authority.
use super::*;

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn normal_work_reporter_recovery_preserves_original_deadline_target_and_grant() {
  for scenario in ["recover", "revoked", "replaced-parent", "expired"] {
    let configured = flow_with_reporting_bounds("opaque-reporter", None, 30000, 1000);
    let (fixture, profile) = fixture_with_flow("opaque-reporter", configured, None, None).await;
    let (fixture, database) = ordering::postgres::postgres_fixture(fixture).await;
    let (fixture, _) = managed_fixture_with_native_parent(fixture, profile, Some(exact("native-writer", 71))).await;
    let (deployment, config, sources) = installed_reporter(&fixture, &database.url()).await;
    let now = SystemFactoryExecutionClock.now().unwrap().unix_millis();
    // The expired case uses the existing trusted fixture clock to accept and attempt
    // an old operation. Normal startup must use its original deadline, not boot time.
    let base = if scenario == "expired" { now - 31000 } else { now };
    fixture.clock.0.store(base, Ordering::SeqCst);
    let run = admit_managed(&fixture).await;
    let projector = Arc::new(
      sources
        .reporter_projector(fixture.input.project_id(), fixture.clock.clone())
        .unwrap(),
    );
    accept_status_at(&fixture, projector.clone(), run, base).await;
    let accepted = fixture.store.factory_run_snapshot(run).await.unwrap();
    let directory = fixture.root.path().join("opaque-reporter");
    std::fs::write(
      directory.join("reporter-mode.json"),
      br#"{"lose_project_response":true}"#,
    )
    .unwrap();
    fixture.clock.0.store(base + 120, Ordering::SeqCst);
    let first = reporting_worker(&fixture, projector, "before-normal-restart", 10000);
    assert_eq!(
      first.run_once(time(base + 120)).await.unwrap().retrying,
      1,
      "{scenario}"
    );
    drop(first);
    let pending = fixture.store.factory_run_snapshot(run).await.unwrap();
    let operation = pending
      .outbox
      .iter()
      .find(|row| {
        row.kind.as_str() == "work.status.projection" && row.state == FactoryOutboxState::Pending && row.attempt == 1
      })
      .unwrap()
      .operation_id;
    let before = captured_requests(&directory);
    let calls: Vec<Value> = before.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    let operations: Vec<_> = calls
      .iter()
      .map(|call| call["message"]["command"]["operation"].as_str().unwrap())
      .collect();
    assert_eq!(operations, ["inspect", "inspect", "apply"]);
    if scenario == "revoked" {
      let connection = fixture.input.definition().connection.as_ref().unwrap();
      fixture
        .store
        .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id: connection.id,
            connection_version: connection.version,
            project_id: fixture.input.project_id(),
            expected_current_version: Some(connection.access_version),
            resources: vec![IntegrationResourceSelection {
              reference: fixture.input.definition().source.resource.clone(),
              purposes: vec![IntegrationPurpose::TaskRead],
            }],
          },
          idempotency_key: IdempotencyKey::new("revoke-before-recovery").unwrap(),
          published_at: time(now + 200),
        }))
        .await
        .unwrap();
    } else if scenario == "replaced-parent" {
      replace_installed_native_parent(deployment.path());
    }
    std::fs::write(directory.join("reporter-mode.json"), b"{}").unwrap();
    drop(sources);
    let runtime = crate::ServerRuntime::start(config).await.unwrap();
    let expected = if scenario == "recover" {
      FactoryOutboxState::Delivered
    } else {
      FactoryOutboxState::Failed
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let after = loop {
      let snapshot = fixture.store.factory_run_snapshot(run).await.unwrap();
      if snapshot
        .outbox
        .iter()
        .any(|row| row.operation_id == operation && row.state == expected)
      {
        break snapshot;
      }
      assert!(
        tokio::time::Instant::now() < deadline,
        "{scenario}: recovery must settle the original operation"
      );
      tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let settled = after
      .outbox
      .iter()
      .find(|row| row.operation_id == operation && row.state == expected)
      .unwrap();
    assert_eq!(settled.attempt, 1);
    assert_eq!(
      settled.failure,
      match scenario {
        "recover" => None,
        "revoked" => Some(FactoryOutboxFailure::Forbidden),
        _ => Some(FactoryOutboxFailure::Exhausted),
      },
      "{scenario}"
    );
    assert_eq!(after.work, accepted.work);
    assert_eq!(after.run, accepted.run);
    assert_eq!(after.flow, accepted.flow);
    assert_eq!(fixture.runs().await.len(), 1);
    let captured = captured_requests(&directory);
    let calls: Vec<Value> = captured
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect();
    let operations: Vec<_> = calls
      .iter()
      .map(|call| call["message"]["command"]["operation"].as_str().unwrap())
      .collect();
    if scenario == "recover" {
      assert_eq!(operations, ["inspect", "inspect", "apply", "inspect", "inspect"]);
    } else {
      assert_eq!(
        captured, before,
        "{scenario}: no fallback, renewed window or additional provider call"
      );
    }
    for call in &calls {
      let intent = original_intent(call);
      assert_eq!(intent, original_intent(&calls[0]));
      assert_eq!(intent["projection_id"], json!(operation));
      assert_eq!(intent["item_id"], "TASK-native");
      assert_eq!(intent["resource"]["resource_id"], "native-container");
    }
    runtime.shutdown().await.unwrap();
    drop(fixture);
    database.cleanup().await;
  }
}
