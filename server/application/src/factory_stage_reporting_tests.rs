//! Stage-local source effects through the ordinary graph, accepted records and reporting outbox.
use super::*;
use std::collections::BTreeMap;
#[path = "factory_stage_reporting_test_support.rs"]
mod support;
use support::*;

#[test]
fn stage_routes_queue_reporting_before_pipeline_completion_for_each_declared_outcome() {
  crate::factory_admission_tests::run_ready(async {
    for (route, _, status) in ROUTES {
      let (store, run, claim) = admitted(stage_definition(route)).await;
      let reporter = reporter();
      let executor = Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
        reference("reporter"),
        reporter.clone(),
        reported(),
      ))));
      let mut queued = None;
      for tick in 11..30 {
        // A new owner object on every pass exercises reconstruction from the journal.
        let owner = FactoryFlowRunner::new(
          store.clone(),
          executor.clone(),
          Arc::new(crate::factory_flow_tests::Inputs),
          crate::factory_node_test_support::clock(),
        );
        assert_eq!(
          owner.reconcile_flow(run, claim, at(tick)).await.unwrap(),
          FactoryFlowStep::Advanced
        );
        let snapshot = store.factory_run_snapshot(run).await.unwrap();
        if snapshot
          .outbox
          .iter()
          .any(|row| row.kind == key("work.status.projection"))
        {
          queued = Some(snapshot);
          break;
        }
      }
      let snapshot = queued.expect("a nonterminal stage must queue its configured source action");
      let records = &snapshot.flow.data.records;
      assert_eq!(records.len(), 2);
      let source = records
        .iter()
        .find(|r| r.input().node() == &key("inspect-ticket"))
        .unwrap();
      let action = records
        .iter()
        .find(|r| r.input().node() == &key(&format!("notify-{route}")))
        .unwrap();
      assert_eq!(source.metadata().unwrap()["outcome"], route);
      assert_eq!(action.input().sources().len(), 1);
      assert_eq!(action.input().sources()[0].node_attempt_id(), source.node_attempt_id());
      assert_eq!(action.input().sources()[0].digest(), source.digest().unwrap());
      assert!(!snapshot.run.state().is_terminal());
      assert!(
        !snapshot
          .lifecycle_checkpoints
          .iter()
          .any(|row| matches!(row.progress, FactoryLifecycleProgress::FlowResolved { .. }))
      );
      assert!(reporter.requests.lock().unwrap().is_empty());
      let row = snapshot
        .outbox
        .iter()
        .find(|row| row.kind == key("work.status.projection"))
        .unwrap();
      assert_eq!(row.input_digest, action.digest().unwrap());
      let worker = FactoryWorkStatusOutboxWorker::new(
        store.clone(),
        BTreeMap::from([(
          reference("reporter"),
          reporter.clone() as Arc<dyn FactoryWorkStatusProjector>,
        )]),
        FactoryReporterWorkerSettings::new(key("stage-reporter"), 100, 4).unwrap(),
        crate::factory_node_test_support::clock(),
      )
      .unwrap();
      assert_eq!(worker.run_once(at(40)).await.unwrap().delivered, 1);
      {
        let requests = reporter.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].status.as_str(), status);
        assert_eq!(requests[0].operation_id, row.operation_id);
        assert_eq!(requests[0].work_id, snapshot.work.id());
      }
      let after = store.factory_run_snapshot(run).await.unwrap();
      assert_eq!(after.work, snapshot.work);
      assert_eq!(after.run, snapshot.run);
      assert_eq!(after.flow, snapshot.flow);
    }
  });
}

#[test]
fn nested_stage_reporting_keeps_exact_child_provenance_across_outbox_retry_and_owner_restart() {
  crate::factory_admission_tests::run_ready(async {
    let child = stage_definition("ask-human");
    let child_reference = child.reference();
    let root = nested_definition(&child);
    let (store, run, claim) = admitted_closure(vec![root, child]).await;
    let unavailable = Arc::new(Reporter {
      requests: Mutex::new(vec![]),
      unavailable: true,
      scoped: false,
    });
    let executor = Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      reference("reporter"),
      unavailable.clone(),
      reported(),
    ))));
    let mut queued = None;
    for tick in 11..40 {
      let owner = FactoryFlowRunner::new(
        store.clone(),
        executor.clone(),
        Arc::new(crate::factory_flow_tests::Inputs),
        crate::factory_node_test_support::clock(),
      );
      assert_eq!(
        owner.reconcile_flow(run, claim, at(tick)).await.unwrap(),
        FactoryFlowStep::Advanced
      );
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      if snapshot
        .outbox
        .iter()
        .any(|row| row.kind == key("work.status.projection"))
      {
        queued = Some(snapshot);
        break;
      }
    }
    let snapshot = queued.expect("nested stage reporting must not wait for its child to return");
    assert_eq!(snapshot.flow.runs.len(), 2);
    assert_eq!(
      snapshot.flow.data.records.len(),
      2,
      "parent call and child tail are not complete"
    );
    let source = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.input().node() == &key("inspect-ticket"))
      .unwrap();
    let action = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.input().node() == &key("notify-ask-human"))
      .unwrap();
    assert_eq!(source.input().definition(), child_reference);
    assert_eq!(action.input().definition(), child_reference);
    assert_eq!(action.input().flow_run_id(), source.input().flow_run_id());
    assert_eq!(action.input().cycle_id(), source.input().cycle_id());
    assert_ne!(action.input().flow_run_id(), snapshot.admitted_flow.root_run().id());
    assert_eq!(action.input().sources()[0].digest(), source.digest().unwrap());
    assert_eq!(action.input().sources()[0].node_attempt_id(), source.node_attempt_id());
    assert_eq!(source.metadata().unwrap()["outcome"], "ask-human");
    let child_run = snapshot
      .flow
      .runs
      .iter()
      .find(|r| r.id() == action.input().flow_run_id())
      .unwrap();
    let parent = child_run.parent().unwrap();
    let caller = snapshot
      .flow
      .attempts
      .iter()
      .find(|a| a.id() == parent.node_attempt_id())
      .unwrap();
    assert_eq!(caller.node_key(), &key("inspect-ticket"));
    assert_eq!(caller.flow_run_id(), snapshot.admitted_flow.root_run().id());
    let row = snapshot
      .outbox
      .iter()
      .find(|row| row.kind == key("work.status.projection"))
      .unwrap();
    assert_eq!(row.input_digest, action.digest().unwrap());
    let settings = FactoryReporterWorkerSettings::new(key("nested-reporter"), 100, 4).unwrap();
    let first = FactoryWorkStatusOutboxWorker::new(
      store.clone(),
      BTreeMap::from([(
        reference("reporter"),
        unavailable as Arc<dyn FactoryWorkStatusProjector>,
      )]),
      settings.clone(),
      crate::factory_node_test_support::clock(),
    )
    .unwrap();
    assert_eq!(first.run_once(at(50)).await.unwrap().retrying, 1);
    let retried = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(retried.flow, snapshot.flow);
    let queued_retry = retried
      .outbox
      .iter()
      .find(|r| r.operation_id == row.operation_id && r.attempt == 1 && r.state == FactoryOutboxState::Pending)
      .unwrap();
    assert_eq!(queued_retry.input_digest, row.input_digest);
    assert_eq!(queued_retry.available_at, at(55));
    let success = reporter();
    let restarted = FactoryWorkStatusOutboxWorker::new(
      store.clone(),
      BTreeMap::from([(
        reference("reporter"),
        success.clone() as Arc<dyn FactoryWorkStatusProjector>,
      )]),
      settings,
      crate::factory_node_test_support::clock(),
    )
    .unwrap();
    assert_eq!(restarted.run_once(at(54)).await.unwrap().claimed, 0);
    assert_eq!(restarted.run_once(at(55)).await.unwrap().delivered, 1);
    {
      let requests = success.requests.lock().unwrap();
      assert_eq!(requests.len(), 1);
      assert_eq!(requests[0].operation_id, row.operation_id);
      assert_eq!(requests[0].status.as_str(), "context requested");
      assert_eq!(requests[0].work_id, snapshot.work.id());
    }
    let confirmed = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(confirmed.work, snapshot.work);
    assert_eq!(confirmed.run, snapshot.run);
    assert_eq!(confirmed.flow, snapshot.flow);
    // The enclosing graph continues normally; no stage result or action is rerun.
    let mut resolved = false;
    for tick in 60..90 {
      let owner = FactoryFlowRunner::new(
        store.clone(),
        executor.clone(),
        Arc::new(crate::factory_flow_tests::Inputs),
        crate::factory_node_test_support::clock(),
      );
      if owner.reconcile_flow(run, claim, at(tick)).await.unwrap() == FactoryFlowStep::Resolved(key("root-done")) {
        resolved = true;
        break;
      }
    }
    assert!(resolved);
    let finished = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(
      finished
        .flow
        .data
        .records
        .iter()
        .filter(|r| r.input().node() == &key("notify-ask-human"))
        .count(),
      1
    );
    assert_eq!(
      finished
        .outbox
        .iter()
        .filter(|r| r.kind == key("work.status.projection") && r.state == FactoryOutboxState::Pending && r.attempt == 0)
        .count(),
      1
    );
    assert_eq!(restarted.run_once(at(90)).await.unwrap().claimed, 0);
  });
}

#[test]
fn stage_and_pipeline_completion_without_a_selected_source_action_never_write() {
  crate::factory_admission_tests::run_ready(async {
    let (store, run, claim) = admitted(stage_definition("approved")).await;
    let reporter = reporter();
    let executor = Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      reference("reporter"),
      reporter.clone(),
      reported(),
    ))));
    let mut resolved = false;
    for tick in 11..35 {
      let owner = FactoryFlowRunner::new(
        store.clone(),
        executor.clone(),
        Arc::new(crate::factory_flow_tests::Inputs),
        crate::factory_node_test_support::clock(),
      );
      if owner.reconcile_flow(run, claim, at(tick)).await.unwrap() == FactoryFlowStep::Resolved(key("done")) {
        resolved = true;
        break;
      }
    }
    assert!(resolved);
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(snapshot.flow.data.records.len(), 2);
    assert!(
      snapshot
        .flow
        .data
        .records
        .iter()
        .all(|r| !r.input().node().as_str().starts_with("notify-"))
    );
    assert!(
      !snapshot
        .outbox
        .iter()
        .any(|row| row.kind == key("work.status.projection"))
    );
    let worker = FactoryWorkStatusOutboxWorker::new(
      store.clone(),
      BTreeMap::from([(
        reference("reporter"),
        reporter.clone() as Arc<dyn FactoryWorkStatusProjector>,
      )]),
      FactoryReporterWorkerSettings::new(key("silent-reporter"), 100, 4).unwrap(),
      crate::factory_node_test_support::clock(),
    )
    .unwrap();
    assert_eq!(worker.run_once(at(40)).await.unwrap().claimed, 0);
    assert!(
      reporter.requests.lock().unwrap().is_empty(),
      "the spelling approved grants no implicit effect"
    );
    assert_eq!(store.factory_run_snapshot(run).await.unwrap().flow, snapshot.flow);
  });
}
