//! Late validation execution controls use the same owner, journal and operator boundary as every other node.
use crate::factory_flow_tests::{Execution, FactoryTestBuildSeeder, Inputs, configured_fixture_from_template};
use crate::factory_node_test_support::fixtures::{key, supplied_base_flow_template};
use crate::factory_requirements_tests::claim_run;
use crate::issue_test_support as issues;
use crate::*;
use async_trait::async_trait;
use octacity_server_factory::*;
use octacity_server_store::{FactoryRunStore, IdempotencyKey, IssueExecutionStore, testing};
use std::sync::{
  Arc, Weak,
  atomic::{AtomicBool, Ordering},
};
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
struct LateExecution {
  execution: Weak<Execution>,
  cancel: bool,
  started: AtomicBool,
}
#[async_trait]
impl FactoryTestBuildSeeder for LateExecution {
  async fn seed(&self, request: &CreateFactoryBuild, acceptance: &FactoryBuildAcceptance) {
    if request.causality.profile.node == key("protected_validation") && !self.started.swap(true, Ordering::SeqCst) {
      *self
        .execution
        .upgrade()
        .unwrap()
        .by_build(acceptance.build_id)
        .state
        .lock()
        .unwrap() = if self.cancel {
        octacity_server_orchestrator::BuildState::Running
      } else {
        octacity_server_orchestrator::BuildState::Failed
      };
    }
  }
}
#[test]
fn late_validation_retry_preserves_exact_inputs_and_runs_review_only_after_success() {
  crate::factory_flow_tests::run_ready(verify_late_control(false));
}
#[test]
fn late_validation_cancellation_preserves_history_and_cannot_dispatch_review() {
  crate::factory_flow_tests::run_ready(verify_late_control(true));
}
async fn verify_late_control(cancel: bool) {
  let mut template = supplied_base_flow_template();
  for node in &mut template.nodes {
    node.pool = None;
    if node.key == key("protected_validation") {
      node.max_execution_retries = 1;
    }
  }
  let (admission, execution) = configured_fixture_from_template(template, "/accepted");
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  let execution = Arc::new_cyclic(|weak| {
    execution.seeder = Some(Arc::new(LateExecution {
      execution: weak.clone(),
      cancel,
      started: AtomicBool::new(false),
    }));
    execution
  });
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  let audit = testing::management_mutation_with_request((), "late-validation-control")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  let issue_store = issues::manual_issue_store(admission.work.subject().project_id()).await;
  let issue = AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(IssueHandlers::new(
      issue_store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    )),
  )
  .handle_command(
    &crate::factory_admission_tests::context(),
    CreateIssueCommand {
      intent: CreateIssueIntent {
        project_id: admission.work.subject().project_id(),
        planning: Default::default(),
        content: IssueContent {
          title: "Existing execution".into(),
          description: String::new(),
          acceptance: None,
          priority: None,
        },
      },
      idempotency_key: "issue".parse().unwrap(),
      created_at: at(1),
    },
  )
  .await
  .unwrap()
  .issue;
  let association = issue_store
    .associate_issue_work(testing::management_mutation(
      octacity_server_store::AssociateIssueWork {
        project_id: issue.project_id,
        issue_id: issue.id,
        request_id: None,
        admission: admission.clone(),
      },
    ))
    .await
    .unwrap()
    .association;
  let mut claim = claim_run(store.as_ref(), admission.run.id(), 10).await;
  let mut held = None;
  let mut terminal = None;
  for time in 31..180 {
    execution.clock.store(time, Ordering::SeqCst);
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone())),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    let step = crate::factory_node_test_support::reconcile_admission_then_observe(
      &owner,
      store.as_ref(),
      admission.run.id(),
      claim.id,
      at(time),
    )
    .await
    .unwrap_or_else(|error| panic!("owner at {time}, controlled {}: {error:?}", held.is_some()));
    let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    if held.is_none() && step == FactoryFlowStep::Waiting {
      let input = snapshot
        .flow
        .data
        .inputs
        .iter()
        .find(|input| input.node() == &key("protected_validation"))
        .unwrap()
        .clone();
      assert!(
        !snapshot
          .flow
          .data
          .inputs
          .iter()
          .any(|input| input.node() == &key("review"))
      );
      let intent = snapshot
        .flow
        .data
        .build_intents
        .iter()
        .find(|intent| intent.input() == &input)
        .unwrap();
      let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
      let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
      if cancel {
        FactoryControlHandlers::new(store.clone())
          .execute_management_command(
            &context,
            &grant,
            CancelFactoryRunCommand {
              run_id: admission.run.id(),
              expected_version: snapshot.run.version(),
              idempotency_key: IdempotencyKey::new("late-cancel").unwrap(),
              requested_at: at(time),
            },
          )
          .await
          .unwrap();
      } else {
        assert_eq!(snapshot.flow.data.stops.len(), 1);
        FactoryControlHandlers::new(store.clone())
          .execute_management_command(
            &context,
            &grant,
            RetryFactoryNodeCommand {
              run_id: admission.run.id(),
              node_attempt_id: intent.node().id(),
              expected_version: snapshot.run.version(),
              idempotency_key: IdempotencyKey::new("late-retry").unwrap(),
              requested_at: at(time),
            },
          )
          .await
          .unwrap();
      }
      held = Some(input);
      claim = claim_run(store.as_ref(), admission.run.id(), time).await;
    }
    if let FactoryFlowStep::Resolved(outcome) = step {
      terminal = Some(outcome);
      break;
    }
    if cancel && snapshot.run.state() == FactoryRunState::Cancelled {
      break;
    }
  }
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(snapshot.run.id(), association.run_id);
  assert_eq!(snapshot.run.work_id(), association.work_id);
  assert_eq!(
    issue_store
      .issue_work_association(issue.project_id, admission.work.id())
      .await
      .unwrap(),
    Some(association)
  );
  let held = held.expect("the late execution must be controlled after both immutable inputs were selected");
  let mut validations = snapshot
    .flow
    .data
    .build_intents
    .iter()
    .filter(|intent| intent.input().node() == &key("protected_validation"))
    .collect::<Vec<_>>();
  validations.sort_by_key(|intent| intent.input().generation());
  if cancel {
    assert_eq!(snapshot.run.state(), FactoryRunState::Cancelled);
    assert_eq!(validations.len(), 1);
    assert!(
      !snapshot
        .flow
        .data
        .records
        .iter()
        .any(|record| record.input().node() == &key("review") || record.input().node() == &key("protected_validation"))
    );
  } else {
    assert_eq!(terminal, Some(key("verified_candidate")));
    assert_eq!(validations.len(), 2);
    assert_eq!(validations[0].input(), &held);
    let retried = validations[1].input();
    assert_eq!(retried.payload(), held.payload());
    assert_eq!(retried.context(), held.context());
    assert_eq!(retried.sources(), held.sources());
    assert_eq!(retried.subject(), held.subject());
    assert_ne!(
      retried.digest().unwrap(),
      held.digest().unwrap(),
      "the retry has a fresh execution generation"
    );
    assert_ne!(
      validations[0].operation_id().unwrap(),
      validations[1].operation_id().unwrap()
    );
    assert_eq!(
      snapshot
        .flow
        .data
        .inputs
        .iter()
        .filter(|input| input.node() == &key("implementation"))
        .count(),
      1
    );
    assert_eq!(
      snapshot
        .flow
        .data
        .inputs
        .iter()
        .filter(|input| input.node() == &key("protected_tests"))
        .count(),
      1
    );
    assert_eq!(
      snapshot
        .flow
        .data
        .inputs
        .iter()
        .filter(|input| input.node() == &key("review"))
        .count(),
      1
    );
  }
}
