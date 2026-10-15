//! Trusted action projections are canonical request data, including during replay.
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{AttemptId, BuildId, JobId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::*;
use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};

struct Runs(FactoryRunSnapshot);
#[async_trait]
impl FactoryRunStore for Runs {
  async fn factory_run_snapshot(&self, id: FactoryRunId) -> Result<FactoryRunSnapshot, StoreError> {
    assert_eq!(id, self.0.run.id());
    Ok(self.0.clone())
  }
  async fn list_factory_run_diagnostics(
    &self,
    _: ListFactoryRunDiagnostics,
  ) -> Result<FactoryRunDiagnosticPage, StoreError> {
    unreachable!()
  }
  async fn claim_factory_run(&self, _: ClaimFactoryRun) -> Result<ClaimFactoryRunOutcome, StoreError> {
    unreachable!()
  }
  async fn claim_factory_runs(&self, _: ClaimFactoryRuns) -> Result<Vec<ClaimedFactoryRun>, StoreError> {
    unreachable!()
  }
  async fn commit_factory_run_transition(
    &self,
    _: CommitFactoryRunTransition,
  ) -> Result<CommitFactoryRunTransitionOutcome, StoreError> {
    unreachable!()
  }
  async fn claim_factory_outbox(&self, _: ClaimFactoryOutbox) -> Result<Vec<ClaimedFactoryOutbox>, StoreError> {
    unreachable!()
  }
  async fn settle_factory_outbox(&self, _: SettleFactoryOutbox) -> Result<FactoryOutboxRecord, StoreError> {
    unreachable!()
  }
}
struct Accepted {
  operation: FactoryDigest,
  value: AcceptedFactoryBuild,
  reads: AtomicUsize,
}
#[async_trait]
impl FactoryBuildAcceptanceStore for Accepted {
  async fn factory_build_for_operation(
    &self,
    operation: FactoryDigest,
  ) -> Result<Option<AcceptedFactoryBuild>, StoreError> {
    assert_eq!(operation, self.operation);
    self.reads.fetch_add(1, Ordering::SeqCst);
    Ok(Some(self.value.clone()))
  }
  async fn accept_factory_build(&self, _: AcceptFactoryBuild) -> Result<AcceptedFactoryBuild, StoreError> {
    panic!("replay must not create another Build")
  }
}
struct Compiler(bool);
#[async_trait]
impl FactoryBuildMaterializer for Compiler {
  fn action_input(
    &self,
    intent: &FlowBuildIntent,
    admitted: &AdmittedFlow,
  ) -> Result<Option<serde_json::Value>, OrdinaryBuildApplicationError> {
    if !self.0 {
      return Ok(None);
    }
    let plan = PromotionPlan::freeze(admitted, intent.input()).map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
    Ok(Some(
      serde_json::json!({"plan":plan,"plan_digest":plan.digest().unwrap()}),
    ))
  }
  async fn materialize(
    &self,
    _: &FlowBuildIntent,
    _: &AdmittedFlow,
    _: FactoryClaimOwnership,
    _: Timestamp,
  ) -> Result<AcceptFactoryBuild, OrdinaryBuildApplicationError> {
    panic!("replay must not compile or publish inputs")
  }
}

#[test]
fn ordinary_build_replay_accepts_only_the_exact_installed_action_projection() {
  crate::factory_admission_tests::run_ready(async {
    let (work, admitted, _, input, attempt) = crate::factory_node_test_support::fixtures::promotion_execution_fixture();
    let intent = FlowBuildIntent::new(input, &admitted, attempt, Default::default(), 7).unwrap();
    let store = octacity_server_store::testing::InMemoryFactoryConfigurationStore::new();
    let run = FactoryRun::admitted(intent.input().factory_run_id(), &work);
    let audit = octacity_server_store::testing::management_mutation_with_request((), "action-projection-replay")
      .audit()
      .clone();
    store
      .seed_factory_run(
        PublishedFactoryAdmission {
          work,
          run: run.clone(),
          flow: admitted.clone(),
          admitted_at: Timestamp::from_unix_millis(1).unwrap(),
        },
        FactoryDigest::from_bytes([33; 32]),
        &audit,
      )
      .unwrap();
    let mut snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
    snapshot.flow.data.build_intents.push(intent.clone());
    let mut request = crate::factory_node_build::factory_build_request(
      &intent,
      FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
    )
    .unwrap();
    request.causality.action_input = Compiler(true).action_input(&intent, &admitted).unwrap();
    let accepted = Arc::new(Accepted {
      operation: request.operation_id,
      value: AcceptedFactoryBuild {
        build_id: BuildId::generate(),
        attempt_id: AttemptId::generate(),
        job_ids: vec![JobId::generate()],
        effective_policy_digest: intent.permissions().digest(),
      },
      reads: AtomicUsize::new(0),
    });
    let runs = Arc::new(Runs(snapshot));
    let service = FactoryOrdinaryBuildService::new(
      accepted.clone(),
      runs.clone(),
      Arc::new(Compiler(true)),
      crate::factory_node_test_support::clock(),
    );
    let replay = service
      .create_factory_build(request.clone())
      .await
      .expect("trusted action projection survives ordinary Build replay without another publication");
    assert_eq!(replay.build_id, accepted.value.build_id);
    assert_eq!(accepted.reads.load(Ordering::SeqCst), 1);
    let mut forged = request.clone();
    forged.causality.action_input.as_mut().unwrap()["plan_digest"] =
      serde_json::json!(FactoryDigest::from_bytes([88; 32]));
    assert!(matches!(
      service.create_factory_build(forged).await,
      Err(OrdinaryBuildApplicationError::Conflict)
    ));
    let unconfigured = FactoryOrdinaryBuildService::new(
      accepted.clone(),
      runs,
      Arc::new(Compiler(false)),
      crate::factory_node_test_support::clock(),
    );
    assert!(matches!(
      unconfigured.create_factory_build(request).await,
      Err(OrdinaryBuildApplicationError::Conflict)
    ));
    assert_eq!(
      accepted.reads.load(Ordering::SeqCst),
      1,
      "unknown action data is rejected before storage recovery"
    );
  });
}
