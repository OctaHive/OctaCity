use super::*;
use octacity_server_store::testing;

use octacity_server_domain::Timestamp;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
use octacity_server_store::testing::factory_profile_selection_contract_fixture as fixture;
struct NoExecution;
#[async_trait]
impl FactoryNodeExecutor for NoExecution {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    panic!("profile selection cannot call a Build or provider")
  }
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    panic!("profile selection owns no external execution")
  }
}
async fn start(
  admission: PublishedFactoryAdmission,
  ownership: FactoryClaimOwnership,
) -> (Arc<testing::InMemoryFactoryConfigurationStore>, FactoryRunClaimRecord) {
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  let audit = testing::management_mutation_with_request((), "profile-owner")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  let claim = FactoryRunClaimRecord::new(admission.run.id(), ownership.owner().clone(), ownership.claim());
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: admission.run.id(),
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        admission.run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        claim.claim.claimed_at(),
      ),
    })
    .await
    .unwrap();
  (store, claim)
}
#[test]
fn manual_selection_waits_without_execution_attempts_builds_or_provider_calls() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = fixture(None);
    let run = admission.run.id();
    let (store, claim) = start(admission, ownership).await;
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    let runner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      runner
        .run_once(run, claim.id, target.clone(), Some(input.clone()), at(11))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let before = store.factory_run_snapshot(run).await.unwrap();
    assert!(
      before.flow.attempts.is_empty(),
      "waiting for a profile must not allocate an execution attempt"
    );
    assert!(before.flow.data.build_intents.is_empty());
    assert_eq!(
      before.flow.data.record_count(),
      2,
      "freeze the input and its pending selection together"
    );
    assert_eq!(
      runner
        .run_once(run, claim.id, target, Some(input), at(12))
        .await
        .unwrap(),
      FactoryNodeStep::Waiting
    );
    let after = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before, after, "polling cannot append rows or spend attempts");
  });
}

#[test]
fn configured_manual_choice_retains_the_exact_candidate_before_any_execution() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = fixture(Some("specialized"));
    let run = admission.run.id();
    let (store, claim) = start(admission.clone(), ownership).await;
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    let runner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      runner
        .run_once(run, claim.id, target.clone(), Some(input.clone()), at(11))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    let choice = &snapshot.flow.data.profile_selections[0];
    assert_eq!(choice.candidate(), Some(&key("specialized")));
    assert_eq!(choice.input_digest(), input.digest().unwrap());
    assert_eq!(choice.deadline(), at(1011));
    let restored = FlowProfileSelectionRecord::restore(
      &serde_json::to_vec(choice).unwrap(),
      &input,
      &admission.flow,
      choice.digest().unwrap(),
    )
    .unwrap();
    assert_eq!(&restored, choice);
    assert!(snapshot.flow.attempts.is_empty());
    assert!(snapshot.flow.data.build_intents.is_empty());
    assert_eq!(
      runner
        .run_once(run, claim.id, target, Some(input), at(12))
        .await
        .unwrap(),
      FactoryNodeStep::Waiting
    );
    assert_eq!(snapshot, store.factory_run_snapshot(run).await.unwrap());
  });
}

struct FrozenInput {
  input: FlowNodeInput,
  calls: std::sync::atomic::AtomicUsize,
}
#[async_trait]
impl FactoryFlowInputPreparer for FrozenInput {
  async fn prepare(&self, _: &FactoryRunSnapshot, _: &FactoryNodeTarget) -> Result<FlowNodeInput, ApplicationError> {
    self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(self.input.clone())
  }
}
#[test]
fn the_flow_owner_reuses_pending_selection_and_context_after_takeover() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = fixture(None);
    let run = admission.run.id();
    let (store, claim) = start(admission, ownership.clone()).await;
    let inputs = Arc::new(FrozenInput {
      input,
      calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let runner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      inputs.clone(),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      runner.reconcile_flow(run, claim.id, at(11)).await.unwrap(),
      FactoryFlowStep::Advanced
    );
    let before = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(
      runner.reconcile_flow(run, claim.id, at(12)).await.unwrap(),
      FactoryFlowStep::Waiting
    );
    assert_eq!(
      inputs.calls.load(std::sync::atomic::Ordering::SeqCst),
      1,
      "waiting cannot re-resolve already retained Context"
    );
    let successor = FactoryRunClaimRecord::new(
      run,
      ownership.owner().clone(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
        at(101),
        at(200),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run,
        expected_version: before.run.version(),
        record: successor.clone(),
        audit: FactoryAuditFact::new(
          run,
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([2; 32]),
          key("accepted"),
          at(101),
        ),
      })
      .await
      .unwrap();
    let restarted = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      inputs.clone(),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      restarted.reconcile_flow(run, successor.id, at(150)).await.unwrap(),
      FactoryFlowStep::Waiting
    );
    let after = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.flow, after.flow);
    assert_eq!(inputs.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(after.flow.data.profile_selections[0].deadline(), at(1011));
    assert!(after.flow.attempts.is_empty());
  });
}

#[test]
fn the_selection_journal_cannot_shift_the_original_stage_deadline() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = fixture(None);
    let run = admission.run.id();
    let (store, claim) = start(admission.clone(), ownership).await;
    let before = store.factory_run_snapshot(run).await.unwrap();
    let mut request = testing::factory_flow_contract_transition(&before, &claim, 11);
    request
      .append
      .flow
      .data
      .profile_selections
      .push(FlowProfileSelectionRecord::manual(&input, &admission.flow, at(12)).unwrap());
    request.append.flow.data.inputs.push(input);
    assert!(
      store.commit_factory_run_transition(request).await.is_err(),
      "selection must use its authoritative journal commit time"
    );
    assert_eq!(before, store.factory_run_snapshot(run).await.unwrap());
  });
}
