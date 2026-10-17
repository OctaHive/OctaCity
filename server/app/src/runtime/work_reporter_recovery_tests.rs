//! Unknown process results and independently fenced outbox recovery.
use super::*;

pub(super) fn worker<S: FactoryRunStore>(
  fixture: &Fixture<S>,
  projector: Arc<dyn FactoryWorkStatusProjector>,
  owner: &str,
  claim_millis: u64,
) -> FactoryWorkStatusOutboxWorker<S> {
  FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector)]),
    FactoryReporterWorkerSettings::new(key(owner), claim_millis, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap()
}

#[tokio::test]
async fn lost_project_response_is_recovered_by_observation_after_restart_for_any_provider() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let (fixture, profile) = fixture(provider).await;
    let projector = hosted(&fixture, vec![profile.clone()]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let directory = fixture.root.path().join(provider);
    std::fs::write(
      directory.join("reporter-mode.json"),
      br#"{"lose_project_response":true}"#,
    )
    .unwrap();
    let first = worker(&fixture, projector, "reporting-before-restart", 10000);
    assert_eq!(first.run_once(time(120)).await.unwrap().retrying, 1, "{provider}");
    assert!(directory.join("reporter-applied").exists());
    assert_eq!(before.flow, fixture.store.factory_run_snapshot(run).await.unwrap().flow);
    drop(first);
    let restarted = worker(
      &fixture,
      hosted(&fixture, vec![profile]),
      "reporting-after-restart",
      10000,
    );
    fixture.clock.0.store(220, Ordering::SeqCst);
    assert_eq!(restarted.run_once(time(220)).await.unwrap().delivered, 1);
    assert_eq!(restarted.run_once(time(221)).await.unwrap().claimed, 0);
    let calls = calls(&directory);
    assert_eq!(
      operations(&calls),
      ["inspect", "inspect", "apply", "inspect", "inspect"]
    );
    assert_same_intent(&calls);
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.flow, after.flow);
    assert_eq!(before.run, after.run);
    assert_eq!(before.work, after.work);
    assert_eq!(fixture.runs().await.len(), 1);
  }
}
pub(super) fn calls(directory: &std::path::Path) -> Vec<serde_json::Value> {
  captured_requests(directory)
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect()
}
fn operations(calls: &[serde_json::Value]) -> Vec<&str> {
  calls
    .iter()
    .map(|call| call["message"]["command"]["operation"].as_str().unwrap())
    .collect()
}
fn assert_same_intent(calls: &[serde_json::Value]) {
  assert!(!calls.is_empty());
  for call in calls {
    assert_eq!(original_intent(call), original_intent(&calls[0]));
  }
}

#[tokio::test]
async fn an_invalid_observation_never_authorizes_a_project_even_when_project_responses_can_be_uncertain() {
  let (fixture, profile) = fixture("jira").await;
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let directory = fixture.root.path().join("jira");
  std::fs::write(
    directory.join("reporter-mode.json"),
    br#"{"lose_observation_response":true}"#,
  )
  .unwrap();
  let reporter = worker(&fixture, projector, "invalid-observation", 10000);
  assert_eq!(reporter.run_once(time(120)).await.unwrap().failed, 1);
  assert_eq!(operations(&calls(&directory)), ["inspect"]);
  assert!(!directory.join("reporter-applied").exists());
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
}

#[tokio::test]
async fn exhausted_confirmation_preserves_applied_effects_and_the_accepted_flow() {
  let (fixture, profile) = fixture("jira").await;
  let projector = hosted(&fixture, vec![profile.clone()]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let directory = fixture.root.path().join("jira");
  std::fs::write(
    directory.join("reporter-mode.json"),
    br#"{"lose_project_response":true}"#,
  )
  .unwrap();
  let first = worker(&fixture, projector, "lost-confirmation", 10000);
  assert_eq!(first.run_once(time(120)).await.unwrap().retrying, 1);
  std::fs::write(
    directory.join("reporter-mode.json"),
    br#"{"lose_observation_response":true}"#,
  )
  .unwrap();
  let restarted = worker(
    &fixture,
    hosted(&fixture, vec![profile]),
    "exhausted-confirmation",
    10000,
  );
  fixture.clock.0.store(220, Ordering::SeqCst);
  assert_eq!(restarted.run_once(time(220)).await.unwrap().failed, 1);
  assert_eq!(restarted.run_once(time(221)).await.unwrap().claimed, 0);
  let calls = calls(&directory);
  assert_eq!(operations(&calls), ["inspect", "inspect", "apply", "inspect"]);
  assert_same_intent(&calls);
  assert_eq!(
    std::fs::read_to_string(directory.join("reporter-applied")).unwrap(),
    calls[0]["message"]["command"]["payload"]["projection_id"]
      .as_str()
      .unwrap()
  );
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
  assert_eq!(before.work, after.work);
  assert_eq!(fixture.runs().await.len(), 1);
}

#[tokio::test]
async fn losing_the_outbox_fence_during_observation_prevents_the_old_owner_from_projecting() {
  let (fixture, profile) = fixture("gitlab").await;
  let projector = hosted(&fixture, vec![profile.clone()]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let directory = fixture.root.path().join("gitlab");
  std::fs::write(directory.join("reporter-mode.json"), br#"{"pause_observe":true}"#).unwrap();
  let first = worker(&fixture, projector, "expired-owner", 1000);
  let pending = tokio::spawn(async move { first.run_once(time(120)).await });
  wait_for_file(&directory.join("reporter-observing")).await;
  std::fs::write(directory.join("reporter-mode.json"), b"{}").unwrap();
  fixture.clock.0.store(1120, Ordering::SeqCst);
  let restarted = worker(&fixture, hosted(&fixture, vec![profile]), "new-owner", 3000);
  let recovery = tokio::spawn(async move { restarted.run_once(time(1120)).await });
  // The second owner holds the replacement claim while the first response is
  // still in flight. No reliance on store vector order or worker sleep timing.
  tokio::time::timeout(Duration::from_secs(3), async {
    while calls(&directory).len() < 2 {
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
  })
  .await
  .unwrap();
  std::fs::write(directory.join("reporter-release"), b"release").unwrap();
  assert!(pending.await.unwrap().is_err());
  assert_eq!(recovery.await.unwrap().unwrap().delivered, 1);
  let calls = calls(&directory);
  assert_eq!(
    operations(&calls),
    ["inspect", "inspect", "inspect", "apply", "inspect"]
  );
  assert_same_intent(&calls);
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
  assert_eq!(before.work, after.work);
  assert_eq!(fixture.runs().await.len(), 1);
}

#[tokio::test]
async fn applied_effects_survive_fence_loss_and_worker_restart_without_a_second_project() {
  for abort_old in [false, true] {
    let (fixture, profile) = fixture("custom-tracker").await;
    let projector = hosted(&fixture, vec![profile.clone()]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let directory = fixture.root.path().join("custom-tracker");
    std::fs::write(directory.join("reporter-mode.json"), br#"{"pause_after_apply":true}"#).unwrap();
    let first = worker(&fixture, projector, "old-applied-owner", 1000);
    let pending = tokio::spawn(async move { first.run_once(time(120)).await });
    wait_for_file(&directory.join("reporter-awaiting-response")).await;
    let pending = if abort_old {
      pending.abort();
      assert!(pending.await.unwrap_err().is_cancelled());
      None
    } else {
      Some(pending)
    };
    fixture.clock.0.store(1120, Ordering::SeqCst);
    let restarted = worker(
      &fixture,
      hosted(&fixture, vec![profile]),
      "recovered-applied-owner",
      3000,
    );
    assert_eq!(restarted.run_once(time(1120)).await.unwrap().delivered, 1);
    std::fs::write(directory.join("reporter-release"), b"release").unwrap();
    if let Some(pending) = pending {
      assert!(
        pending.await.unwrap().is_err(),
        "the stale response cannot settle a replacement claim"
      );
    }
    assert_eq!(restarted.run_once(time(1121)).await.unwrap().claimed, 0);
    let calls = calls(&directory);
    assert_eq!(
      operations(&calls),
      ["inspect", "inspect", "apply", "inspect", "inspect"]
    );
    assert_same_intent(&calls);
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.flow, after.flow);
    assert_eq!(before.run, after.run);
    assert_eq!(before.work, after.work);
    assert_eq!(fixture.runs().await.len(), 1);
  }
}
async fn wait_for_file(path: &std::path::Path) {
  tokio::time::timeout(Duration::from_secs(3), async {
    while !path.exists() {
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
  })
  .await
  .unwrap();
}
