//! Projection persistence is exercised through the same public store port as workers.
use super::*;

#[test]
fn action_projection_persists_exact_targets_and_reports_unfinished_backfill() {
  crate::factory_flow_tests::run_ready(async {
    let (store, snapshot) = pending(human_fixture()).await;
    let query = || FactoryActionProjectionQuery::new(snapshot.run.id(), at(12), None, 1).unwrap();
    let missing = store.read_factory_action_projection(query()).await.unwrap();
    assert_eq!(missing.coverage, FactoryActionProjectionCoverage::Incomplete);
    assert!(missing.items.is_empty());
    let batch = store.factory_action_projection_work(1).await.unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].run_id, snapshot.run.id());
    assert_eq!(batch.items[0].source_version, snapshot.run.version());
    store
      .refresh_factory_action_projection(batch.items[0], at(12))
      .await
      .unwrap();
    let restarted = store.clone();
    let page = restarted.read_factory_action_projection(query()).await.unwrap();
    assert_eq!(page.coverage, FactoryActionProjectionCoverage::Complete);
    assert_eq!(page.observed_at, Some(at(12)));
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].node_attempt_id, snapshot.flow.attempts[0].id());
    assert_eq!(page.items[0].contract, snapshot.flow.runs[0].definition());
    assert_eq!(page.items[0].run_version, snapshot.run.version());
    assert!(
      restarted
        .factory_action_projection_work(1)
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert_eq!(
      restarted.factory_run_snapshot(snapshot.run.id()).await.unwrap(),
      snapshot,
      "index maintenance cannot progress execution"
    );
  });
}

#[test]
fn action_projection_worker_resumes_bounded_backfill_without_execution_ownership() {
  crate::factory_flow_tests::run_ready(async {
    let (store, snapshot) = pending(human_fixture()).await;
    let second = human_fixture();
    store
      .seed_factory_run(
        second.clone(),
        FactoryDigest::from_bytes([9; 32]),
        testing::management_mutation(()).audit(),
      )
      .unwrap();
    let worker = crate::FactoryActionProjectionWorker::new(store.clone());
    assert!(worker.run_once(at(20), 0).await.is_err());
    assert!(worker.run_once(at(20), 101).await.is_err());
    let first = worker.run_once(at(20), 1).await.unwrap();
    assert_eq!(first.completed, 1);
    assert!(first.has_more);
    let restarted = crate::FactoryActionProjectionWorker::new(store.clone());
    let last = restarted.run_once(at(21), 1).await.unwrap();
    assert_eq!(last.completed, 1);
    assert!(!last.has_more);
    assert_eq!(restarted.run_once(at(22), 1).await.unwrap().completed, 0);
    for id in [snapshot.run.id(), second.run.id()] {
      let query = FactoryActionProjectionQuery::new(id, at(22), None, 1).unwrap();
      assert_eq!(
        store.read_factory_action_projection(query).await.unwrap().coverage,
        FactoryActionProjectionCoverage::Complete
      );
    }
    assert_eq!(store.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
    assert_eq!(
      store.factory_run_snapshot(second.run.id()).await.unwrap().run,
      second.run
    );
  });
}

pub(super) async fn assert_materialized_actions(
  store: &testing::InMemoryFactoryConfigurationStore,
  snapshot: &FactoryRunSnapshot,
  observed: i64,
  expected: &[NodeAttemptId],
) {
  let work = store
    .factory_action_projection_work(100)
    .await
    .unwrap()
    .items
    .into_iter()
    .find(|row| row.run_id == snapshot.run.id())
    .unwrap();
  assert_eq!(work.source_version, snapshot.run.version());
  store
    .refresh_factory_action_projection(work, at(observed))
    .await
    .unwrap();
  let query = FactoryActionProjectionQuery::new(snapshot.run.id(), at(observed), None, 1).unwrap();
  let page = store.read_factory_action_projection(query).await.unwrap();
  assert_eq!(page.coverage, FactoryActionProjectionCoverage::Complete);
  assert_eq!(page.run_version, snapshot.run.version());
  assert_eq!(
    page.items.iter().map(|row| row.node_attempt_id).collect::<Vec<_>>(),
    expected
  );
  assert_eq!(page.next, None, "retired historical targets cannot affect continuation");
}

#[test]
fn action_projection_revalidates_expiry_and_rejects_forged_read_bounds() {
  crate::factory_flow_tests::run_ready(async {
    let (store, snapshot) = pending(human_fixture()).await;
    let work = store.factory_action_projection_work(1).await.unwrap().items[0];
    store.refresh_factory_action_projection(work, at(12)).await.unwrap();
    let deadline = snapshot.flow.attempts[0].deadline().unix_millis();
    for (time, coverage, count) in [
      (11, FactoryActionProjectionCoverage::Incomplete, 0),
      (deadline, FactoryActionProjectionCoverage::Complete, 1),
      (deadline + 1, FactoryActionProjectionCoverage::Complete, 0),
    ] {
      let page = store
        .read_factory_action_projection(
          FactoryActionProjectionQuery::new(snapshot.run.id(), at(time), None, 1).unwrap(),
        )
        .await
        .unwrap();
      assert_eq!(page.coverage, coverage);
      assert_eq!(page.items.len(), count);
      assert_eq!(page.next, None);
    }
    let after = store
      .read_factory_action_projection(
        FactoryActionProjectionQuery::new(snapshot.run.id(), at(12), Some(snapshot.flow.attempts[0].id()), 1).unwrap(),
      )
      .await
      .unwrap();
    assert!(after.items.is_empty());
    for limit in [0, 101] {
      assert!(store.factory_action_projection_work(limit).await.is_err());
      assert!(
        store
          .read_factory_action_projection(FactoryActionProjectionQuery {
            run_id: snapshot.run.id(),
            observed_at: at(12),
            after: None,
            limit
          })
          .await
          .is_err()
      );
    }
    assert!(matches!(
      store.refresh_factory_action_projection(work, at(11)).await,
      Err(StoreError::Conflict { .. })
    ));
    assert_eq!(store.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
  });
}
