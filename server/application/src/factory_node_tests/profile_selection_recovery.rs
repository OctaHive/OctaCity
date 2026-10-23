//! Technical retries retain their committed choice rather than evaluating selection again.
use super::*;

fn retry_fixture(
  policy: FlowProfileSelectionPolicy,
  pooled: bool,
) -> (PublishedFactoryAdmission, FlowNodeInput, FactoryClaimOwnership) {
  let (mut admission, previous, ownership) = if pooled {
    pooled_fixture()
  } else {
    testing::factory_profile_selection_policy_contract_fixture(policy.clone())
  };
  let root = &admission.flow.closure().definitions()[0];
  let mut node = serde_json::to_value(root.nodes()[0].clone().with_execution_retries(1).unwrap()).unwrap();
  node["profile_selection"]["policy"] = serde_json::to_value(policy).unwrap();
  node["profile_selection"]["candidates"]["specialized"]["build"]["task_digest"] =
    serde_json::to_value(FactoryDigest::from_bytes([94; 32])).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes: vec![serde_json::from_value(node).unwrap()],
    transitions: root.transitions().to_vec(),
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  let flow = FlowRun::root(&admission.run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  admission.flow = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    admission.flow.limits().clone(),
    flow,
    cycle,
    admission.flow.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(admission.flow.data_schemas().to_vec())
  .unwrap()
  .with_pool_settings(admission.flow.pool_settings().clone())
  .unwrap();
  let input = FlowNodeInput::new(
    &admission.work,
    &admission.flow,
    admission.flow.root_run(),
    admission.flow.initial_cycle(),
    previous.node().clone(),
    previous.payload().clone(),
  )
  .unwrap()
  .with_context(previous.context().unwrap().clone(), vec![])
  .unwrap();
  (admission, input, ownership)
}

#[test]
fn a_confirmed_profile_node_retry_keeps_the_committed_choice_without_selecting_again() {
  crate::factory_admission_tests::run_ready(async {
    let rules: FlowProfileSelectionPolicy = serde_json::from_value(serde_json::json!({"mode":"rules","rules":[{"when":{"operation":"compare","left":{"source":"observation_time"},"comparison":"less_or_equal","right":{"source":"literal","value":20}},"candidate":"advanced"}],"otherwise":"specialized"})).unwrap();
    for pooled in [false, true] {
      for policy in [FlowProfileSelectionPolicy::Manual { candidate: None }, rules.clone()] {
        let (admission, input, ownership) = retry_fixture(policy, pooled);
        let run = admission.run.id();
        let project = admission.work.subject().project_id();
        let choices = testing::factory_profile_selection_capabilities(&admission.flow);
        let profile = admission.flow.closure().definitions()[0].nodes()[0]
          .profile_selection()
          .unwrap()
          .candidates[&key("advanced")]
          .profile
          .reference()
          .clone();
        let (store, claim) = start(admission, ownership).await;
        store.seed_project(project).unwrap();
        store.enable_project(project, choices).unwrap();
        let inputs = Arc::new(FrozenInput {
          input: input.clone(),
          calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let failed = Arc::new(crate::factory_node_failure_tests::FailedExecution {
          ids: (
            octacity_server_domain::BuildId::generate(),
            octacity_server_domain::AttemptId::generate(),
            octacity_server_domain::JobId::generate(),
          ),
        });
        let owner = FactoryFlowRunner::new(
          store.clone(),
          failed,
          inputs.clone(),
          crate::factory_node_test_support::clock(),
        )
        .with_profile_catalog(store.clone());
        owner.reconcile_flow(run, claim.id, at(11)).await.unwrap();
        let pending = store.factory_run_snapshot(run).await.unwrap();
        store
          .apply_factory_run_control(testing::management_mutation_with_request(
            ApplyFactoryRunControl {
              run_id: run,
              expected_version: pending.run.version(),
              idempotency_key: IdempotencyKey::new("original-choice").unwrap(),
              requested_at: at(12),
              intent: FactoryRunControlIntent::SelectProfile {
                input_digest: input.digest().unwrap(),
                profile: profile.clone(),
              },
            },
            "original-choice",
          ))
          .await
          .unwrap();
        let mut claim = store
          .claim_factory_runs(ClaimFactoryRuns::new(key("first-owner"), at(13), at(20), 1).unwrap())
          .await
          .unwrap()
          .remove(0)
          .record;
        owner.reconcile_flow(run, claim.id, at(14)).await.unwrap();
        if pooled {
          assert!(matches!(
            owner.reconcile_flow(run, claim.id, at(15)).await.unwrap(),
            FactoryFlowStep::Ready(_)
          ));
          claim = lease_pool(&store, run, 21).await;
          assert_eq!(
            owner.reconcile_flow(run, claim.id, at(22)).await.unwrap(),
            FactoryFlowStep::Advanced
          );
          // A queued automatic choice may consume the newer manual control first.
          owner.reconcile_flow(run, claim.id, at(23)).await.unwrap();
          assert_eq!(store.factory_run_snapshot(run).await.unwrap().flow.attempts.len(), 1);
          assert_eq!(
            owner.reconcile_flow(run, claim.id, at(24)).await.unwrap(),
            FactoryFlowStep::Waiting
          );
        } else {
          owner.reconcile_flow(run, claim.id, at(15)).await.unwrap();
          owner.reconcile_flow(run, claim.id, at(16)).await.unwrap();
        }
        let stopped = store.factory_run_snapshot(run).await.unwrap();
        assert_eq!(stopped.flow.data.stops[0].reason(), FlowNodeStopReason::ExecutionFailed);
        store
          .apply_factory_run_control(testing::management_mutation_with_request(
            ApplyFactoryRunControl {
              run_id: run,
              expected_version: stopped.run.version(),
              idempotency_key: IdempotencyKey::new("retry-selected-stage").unwrap(),
              requested_at: at(25),
              intent: FactoryRunControlIntent::RetryNode {
                node_attempt_id: stopped.flow.attempts[0].id(),
              },
            },
            "retry-selected-stage",
          ))
          .await
          .unwrap();
        let claim = store
          .claim_factory_runs(ClaimFactoryRuns::new(key("retry-owner"), at(26), at(40), 1).unwrap())
          .await
          .unwrap()
          .remove(0)
          .record;
        let owner = FactoryFlowRunner::new(
          store.clone(),
          Arc::new(NoExecution),
          inputs,
          crate::factory_node_test_support::clock(),
        )
        .with_profile_catalog(store.clone());
        assert_eq!(
          owner.reconcile_flow(run, claim.id, at(30)).await.unwrap(),
          FactoryFlowStep::Advanced
        );
        if pooled {
          assert!(matches!(
            owner.reconcile_flow(run, claim.id, at(31)).await.unwrap(),
            FactoryFlowStep::Ready(_)
          ));
          let selected = lease_pool(&store, run, 41).await;
          assert_eq!(
            owner.reconcile_flow(run, selected.id, at(42)).await.unwrap(),
            FactoryFlowStep::Advanced
          );
        }
        let retried = store.factory_run_snapshot(run).await.unwrap();
        assert_eq!(
          retried.flow.attempts.len(),
          2,
          "retry allocates execution, not another profile wait"
        );
        assert_eq!(
          retried.flow.data.profile_selections,
          stopped.flow.data.profile_selections
        );
        let first = &stopped.flow.data.build_intents[0];
        let retry = retried
          .flow
          .data
          .build_intents
          .iter()
          .find(|intent| intent.node().retry_of() == Some(first.node().id()))
          .unwrap();
        assert_eq!(retry.selected_profile(), Some(&profile));
        assert_eq!(retry.profile(), first.profile());
        assert_eq!(retry.selection_digest(), first.selection_digest());
        assert_eq!(
          FlowBuildIntent::restore(
            &serde_json::to_vec(retry).unwrap(),
            retry.input(),
            &retried.admitted_flow,
            retry.node(),
            retry.operation_id().unwrap()
          )
          .unwrap(),
          *retry
        );
        assert_eq!(retry.input(), &input.retry().unwrap());
        assert_ne!(retry.operation_id().unwrap(), first.operation_id().unwrap());
        assert_eq!(retried.flow.data.stops, stopped.flow.data.stops);
        assert_eq!(retry.usage_before().tokens, 23);
      }
    }
  });
}

async fn lease_pool(
  store: &testing::InMemoryFactoryConfigurationStore,
  run: FactoryRunId,
  observed: i64,
) -> FactoryRunClaimRecord {
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  let policy = &snapshot.admitted_flow.pool_settings()[&key("shared.ready")].policy;
  let selected = store
    .select_phase_ready(SelectPhasePool {
      policy_digest: policy.digest(),
      request_id: FactoryDigest::content_sha256(&observed.to_le_bytes()),
      owner: key("pool-owner"),
      observed_at: at(observed),
      expires_at: at(observed + 50),
      capabilities: Default::default(),
      limit: 1,
    })
    .await
    .unwrap();
  assert_eq!(
    selected.len(),
    1,
    "a confirmed retry retains its profile while acquiring fresh pool capacity"
  );
  selected[0].run_claim()
}

#[test]
fn the_common_journal_rejects_provenance_from_an_unselected_compatible_profile() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = retry_fixture(
      FlowProfileSelectionPolicy::Manual {
        candidate: Some(key("advanced")),
      },
      false,
    );
    let project = admission.work.subject().project_id();
    let run = admission.run.id();
    let choices = testing::factory_profile_selection_capabilities(&admission.flow);
    let (store, claim) = start(admission, ownership).await;
    store.seed_project(project).unwrap();
    store.enable_project(project, choices).unwrap();
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    let owner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    owner
      .run_once(run, claim.id, target.clone(), Some(input.clone()), at(11))
      .await
      .unwrap();
    owner.run_once(run, claim.id, target, None, at(12)).await.unwrap();
    let before = store.factory_run_snapshot(run).await.unwrap();
    let intent = &before.flow.data.build_intents[0];
    let definition = before.admitted_flow.closure().definition(input.definition()).unwrap();
    let schema = before.admitted_flow.data_schema(input.payload().schema()).unwrap();
    let build = octacity_server_domain::BuildId::generate();
    let attempt = octacity_server_domain::AttemptId::generate();
    let job = octacity_server_domain::JobId::generate();
    let execution = FlowBuildExecution::new(intent, build, attempt, vec![job]).unwrap();
    let output = FlowVerifiedBuildOutput {
      name: intent.profile().result_output.clone(),
      output_kind: EvidenceOutputKind::Report,
      schema: schema.reference().clone(),
      artifact: FactoryArtifactReference::new(
        octacity_server_domain::ArtifactId::generate(),
        FactoryDigest::from_bytes([95; 32]),
        11,
      )
      .unwrap(),
      producer: EvidenceProducer::new(
        build,
        attempt,
        job,
        intent.profile().result_tool().clone(),
        intent.profile().plugin.clone(),
      ),
      verified_at: at(13),
      fresh_until: at(90),
      data: None,
    };
    let record = FlowNodeRecord::new(
      &input,
      intent.node(),
      definition,
      FlowRecordObservation {
        outcome: key("observed"),
        payload: input.payload().clone(),
        producer: intent.node().execution().clone(),
        observed_at: at(13),
      },
    )
    .unwrap();
    let completion = |profile| {
      let record = record
        .clone()
        .with_build_provenance(
          FlowBuildProvenance::new(profile, vec![output.clone()]).unwrap(),
          definition,
        )
        .unwrap();
      let mut transition = testing::factory_flow_contract_transition(&before, &claim, 13);
      transition.append.flow.data.build_executions.push(execution.clone());
      transition.append.flow.completions.push(
        record
          .completion(
            intent.node(),
            definition,
            FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
            BudgetUsage::default(),
            at(13),
          )
          .unwrap(),
      );
      transition.append.flow.data.records.push(record);
      transition
    };
    let mut other = intent.profile().clone();
    other.task_digest = definition
      .node(input.node())
      .unwrap()
      .profile_selection()
      .unwrap()
      .candidates[&key("specialized")]
      .build
      .task_digest;
    assert!(
      store.commit_factory_run_transition(completion(other)).await.is_err(),
      "a pinned compatible candidate cannot replace the committed execution profile in its result"
    );
    assert_eq!(before, store.factory_run_snapshot(run).await.unwrap());
    store
      .commit_factory_run_transition(completion(intent.profile().clone()))
      .await
      .unwrap();
    assert_eq!(
      store.factory_run_snapshot(run).await.unwrap().flow.data.records.len(),
      1
    );
  });
}
