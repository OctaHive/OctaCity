//! Ordinary analysis acceptance and durable Issue application have independent lifetimes.
use super::*;
use crate::factory::execution::tests::factory_flow_tests::Execution;
use octacity_server_domain::{IssueId, IssueVersion};
use octacity_server_store::testing;

#[test]
fn selected_form_queues_one_guarded_write_with_accepted_analysis_and_replay_never_reinvokes_the_model() {
  crate::factory::admission::tests::factory_admission_tests::run_ready(async {
    let fixture = testing::factory_issue_field_journal_fixture();
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "selected-fields")
      .audit()
      .clone();
    store
      .seed_factory_run(fixture.admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    let run = fixture.admission.run.id();
    let claim = FactoryRunClaimRecord::new(run, fixture.attempt.owner().clone(), fixture.attempt.claim());
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run,
        expected_version: fixture.admission.run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          run,
          AuditActorKind::Worker,
          None,
          "factory.claimed".parse().unwrap(),
          FactoryDigest::from_bytes([1; 32]),
          "accepted".parse().unwrap(),
          at(10),
        ),
      })
      .await
      .unwrap();
    let target = FactoryNodeTarget {
      flow_run_id: fixture.input.flow_run_id(),
      cycle_id: fixture.input.cycle_id(),
      node: fixture.input.node().clone(),
    };
    let input = fixture
      .input
      .clone()
      .with_issue_field_baseline(FlowIssueFieldBaseline {
        issue_id: IssueId::generate(),
        version: IssueVersion::new(1).unwrap(),
      })
      .unwrap();
    let execution = Arc::new(Execution::for_reports(
      &[(
        target.node.clone(),
        (
          fixture.schema.reference().clone(),
          serde_json::json!({"priority":"high"}),
        ),
      )]
      .into(),
    ));
    execution.clock.store(11, std::sync::atomic::Ordering::SeqCst);
    let runner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution)),
      crate::factory_node_test_support::clock(),
    );
    assert!(
      runner
        .run_once(run, claim.id, target.clone(), Some(fixture.input.clone()), at(11))
        .await
        .is_err(),
      "an opted-in agent cannot start without a controller-captured Issue baseline"
    );
    assert!(store.factory_run_snapshot(run).await.unwrap().flow.attempts.is_empty());
    assert_eq!(
      runner
        .run_once(run, claim.id, target.clone(), Some(input), at(11))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let mut accepted = None;
    for time in 12..16 {
      if let FactoryNodeStep::Completed(record) = runner
        .run_once(run, claim.id, target.clone(), None, at(time))
        .await
        .unwrap()
      {
        accepted = Some(record);
        break;
      }
    }
    let record = accepted.expect("analysis is accepted independently of Issue save availability");
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(snapshot.flow.data.records.len(), 1);
    let writes = snapshot
      .outbox
      .iter()
      .filter(|row| row.input_digest == record.digest().unwrap())
      .collect::<Vec<_>>();
    assert_eq!(
      writes.len(),
      1,
      "one durable pending write is committed with the accepted form"
    );
    assert_eq!(writes[0].state, FactoryOutboxState::Pending);
    let resumed = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(Pending),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      resumed.run_once(run, claim.id, target, None, at(16)).await.unwrap(),
      FactoryNodeStep::Completed(record)
    );
    assert_eq!(
      store.factory_run_snapshot(run).await.unwrap().outbox,
      snapshot.outbox,
      "accepted replay queues no second write"
    );
  });
}
