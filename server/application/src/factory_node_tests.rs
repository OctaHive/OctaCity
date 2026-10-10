use crate::*;
use async_trait::async_trait;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::sync::Arc;

struct Pending;
#[async_trait]
impl FactoryNodeExecutor for Pending {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    Ok(FactoryNodeExecutionStep::Waiting { execution: None })
  }
}
#[test]
fn unknown_nodes_persist_their_full_intent_before_execution_and_resume_without_rebuilding_it() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
    let store = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
    let audit = octacity_server_store::testing::management_mutation_with_request((), "node-owner")
      .audit()
      .clone();
    store
      .seed_factory_run(fixture.admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    let run = &fixture.admission.run;
    let claim = FactoryRunClaimRecord::new(run.id(), fixture.attempt.owner().clone(), fixture.attempt.claim());
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run.id(),
        expected_version: run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          run.id(),
          AuditActorKind::Worker,
          None,
          FactoryKey::new("factory.claimed").unwrap(),
          FactoryDigest::from_bytes([1; 32]),
          FactoryKey::new("accepted").unwrap(),
          claim.claim.claimed_at(),
        ),
      })
      .await
      .unwrap();
    let target = FactoryNodeTarget {
      flow_run_id: fixture.input.flow_run_id(),
      cycle_id: fixture.input.cycle_id(),
      node: fixture.input.node().clone(),
    };
    let runner = FactoryNodeRunner::new(store.clone(), Arc::new(Pending));
    assert_eq!(
      runner
        .run_once(run.id(), claim.id, target.clone(), Some(fixture.input.clone()), at(11))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let frozen = store.factory_run_snapshot(run.id()).await.unwrap();
    assert_eq!(frozen.flow.data.inputs, vec![fixture.input.clone()]);
    assert_eq!(frozen.flow.data.build_intents.len(), 1);
    assert_eq!(frozen.flow.data.build_intents[0].priority(), 1);
    let operation = frozen.flow.data.build_intents[0].operation_id().unwrap();
    let restarted = FactoryNodeRunner::new(store.clone(), Arc::new(Pending));
    assert_eq!(
      restarted
        .run_once(run.id(), claim.id, target, None, at(12))
        .await
        .unwrap(),
      FactoryNodeStep::Waiting
    );
    let restored = store.factory_run_snapshot(run.id()).await.unwrap();
    assert_eq!(restored.flow.data.build_intents[0].operation_id().unwrap(), operation);
    assert_eq!(restored.flow.attempts.len(), 1);
  });
}

#[test]
fn the_common_owner_dispatches_an_ordinary_build_only_after_its_intent_is_durable() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
    let store = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
    let audit = octacity_server_store::testing::management_mutation_with_request((), "build-owner")
      .audit()
      .clone();
    store
      .seed_factory_run(fixture.admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    let run = &fixture.admission.run;
    let claim = FactoryRunClaimRecord::new(run.id(), fixture.attempt.owner().clone(), fixture.attempt.claim());
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run.id(),
        expected_version: run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          run.id(),
          AuditActorKind::Worker,
          None,
          FactoryKey::new("factory.claimed").unwrap(),
          FactoryDigest::from_bytes([1; 32]),
          FactoryKey::new("accepted").unwrap(),
          claim.claim.claimed_at(),
        ),
      })
      .await
      .unwrap();
    let builds = Arc::new(crate::factory_node_test_support::Builds::new());
    let adapter = Arc::new(FactoryNodeBuildAdapter::new(
      builds.clone(),
      Arc::new(crate::factory_node_test_support::Outputs(None)),
    ));
    let runner = FactoryNodeRunner::new(store.clone(), adapter);
    let target = FactoryNodeTarget {
      flow_run_id: fixture.input.flow_run_id(),
      cycle_id: fixture.input.cycle_id(),
      node: fixture.input.node().clone(),
    };
    runner
      .run_once(run.id(), claim.id, target.clone(), Some(fixture.input), at(11))
      .await
      .unwrap();
    assert!(builds.requests.lock().unwrap().is_empty());
    assert_eq!(
      runner.run_once(run.id(), claim.id, target, None, at(31)).await.unwrap(),
      FactoryNodeStep::Waiting
    );
    let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
    assert_eq!(snapshot.flow.data.build_executions.len(), 1);
    assert_eq!(
      snapshot.flow.data.build_executions[0].operation_id(),
      snapshot.flow.data.build_intents[0].operation_id().unwrap()
    );
    assert_eq!(
      builds.requests.lock().unwrap()[0].operation_id,
      snapshot.flow.data.build_intents[0].operation_id().unwrap()
    );
  });
}
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
