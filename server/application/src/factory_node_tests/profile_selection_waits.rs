//! Bounded profile waits use ordinary owner/control and execution contracts.
use super::*;

#[test]
fn expired_profile_wait_is_non_actionable_without_renewing_its_input_or_budget() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = fixture(None);
    let run = admission.run.id();
    let ownership = FactoryClaimOwnership::new(
      ownership.owner().clone(),
      FactoryClaim::new(ownership.claim().fence(), at(10), at(2000)).unwrap(),
    );
    let (store, claim) = start(admission, ownership).await;
    let inputs = Arc::new(FrozenInput {
      input,
      calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      inputs.clone(),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      owner.reconcile_flow(run, claim.id, at(11)).await.unwrap(),
      FactoryFlowStep::Advanced
    );
    let pending = store.factory_run_snapshot(run).await.unwrap();
    for time in [12, 1000] {
      let restarted = FactoryFlowRunner::new(
        store.clone(),
        Arc::new(NoExecution),
        inputs.clone(),
        crate::factory_node_test_support::clock(),
      );
      assert_eq!(
        restarted.reconcile_flow(run, claim.id, at(time)).await.unwrap(),
        FactoryFlowStep::Waiting
      );
    }
    let error = owner
      .reconcile_flow(run, claim.id, at(1011))
      .await
      .expect_err("expired selection cannot remain actionable");
    assert_eq!(error.classification(), ApplicationFailure::Invalid);
    assert_eq!(pending, store.factory_run_snapshot(run).await.unwrap());
    assert_eq!(inputs.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
  });
}

#[test]
fn exhausted_run_budget_cannot_create_a_profile_wait_or_take_a_reserve() {
  crate::factory_admission_tests::run_ready(async {
    let policy = serde_json::from_value(serde_json::json!({"mode":"rules","rules":[{
      "when":{"operation":"exists","operand":{"source":"path","path":"/missing"}},"candidate":"advanced"
    }]}))
    .unwrap();
    let (admission, input, ownership) = testing::factory_profile_selection_failure_contract_fixture(
      policy,
      FlowProfileSelectionFailure::ReserveProfile {
        candidate: key("specialized"),
      },
    );
    let run = admission.run.id();
    let project = admission.work.subject().project_id();
    let choices = testing::factory_profile_selection_capabilities(&admission.flow);
    let (store, claim) = start(admission, ownership).await;
    store.seed_project(project).unwrap();
    store.enable_project(project, choices).unwrap();
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    // Existing retained budget observations are authoritative independently of node completions.
    let mut cost = testing::factory_flow_contract_transition(&snapshot, &claim, 11);
    cost.budget = FactoryBudgetRecord::new(
      run,
      cost.next_run.version(),
      BudgetUsage {
        tokens: snapshot.admitted_flow.limits().budget().max_tokens(),
        ..Default::default()
      },
      at(11),
    );
    cost.current.budget_id = cost.budget.id;
    store.commit_factory_run_transition(cost).await.unwrap();
    let exhausted = store.factory_run_snapshot(run).await.unwrap();
    let owner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    assert!(
      owner
        .run_once(run, claim.id, target, Some(input), at(12))
        .await
        .is_err(),
      "an exhausted hard budget cannot be turned into a reserve choice or actionable wait"
    );
    assert_eq!(exhausted, store.factory_run_snapshot(run).await.unwrap());
  });
}

#[test]
fn cancelling_a_profile_wait_releases_run_wip_without_allocating_or_stopping_execution() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = fixture(None);
    let run = admission.run.id();
    let profile = admission
      .flow
      .closure()
      .definition(input.definition())
      .unwrap()
      .node(input.node())
      .unwrap()
      .profile_selection()
      .unwrap()
      .candidates[&key("specialized")]
      .profile
      .reference()
      .clone();
    let (store, claim) = start(admission, ownership).await;
    let inputs = Arc::new(FrozenInput {
      input: input.clone(),
      calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      inputs.clone(),
      crate::factory_node_test_support::clock(),
    );
    owner.reconcile_flow(run, claim.id, at(11)).await.unwrap();
    let pending = store.factory_run_snapshot(run).await.unwrap();
    let resumed = store
      .claim_factory_runs(ClaimFactoryRuns::new(key("replacement"), at(101), at(200), 1).unwrap())
      .await
      .unwrap();
    assert_eq!(resumed.len(), 1);
    assert_eq!(
      resumed[0].wip_usage.active_runs(),
      1,
      "a manual wait retains its enclosing Run slot"
    );
    assert_eq!(
      resumed[0].wip_usage.active_nodes(),
      0,
      "no execution slot exists before a profile is chosen"
    );
    let cancel = ApplyFactoryRunControl {
      run_id: run,
      expected_version: pending.run.version(),
      idempotency_key: IdempotencyKey::new("cancel-profile-wait").unwrap(),
      requested_at: at(102),
      intent: FactoryRunControlIntent::Cancel,
    };
    store
      .apply_factory_run_control(testing::management_mutation_with_request(cancel, "cancel-profile-wait"))
      .await
      .unwrap();
    let claimed = store
      .claim_factory_runs(ClaimFactoryRuns::new(key("canceller"), at(103), at(200), 1).unwrap())
      .await
      .unwrap();
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      inputs,
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      owner.reconcile_flow(run, claimed[0].record.id, at(104)).await.unwrap(),
      FactoryFlowStep::Cancelled
    );
    let cancelled = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(cancelled.run.state(), FactoryRunState::Cancelled);
    assert_eq!(cancelled.flow, pending.flow);
    assert_eq!(cancelled.budgets.last().unwrap().usage, BudgetUsage::default());
    let late = ApplyFactoryRunControl {
      run_id: run,
      expected_version: cancelled.run.version(),
      idempotency_key: IdempotencyKey::new("late-profile-choice").unwrap(),
      requested_at: at(105),
      intent: FactoryRunControlIntent::SelectProfile {
        input_digest: input.digest().unwrap(),
        profile,
      },
    };
    assert!(
      store
        .apply_factory_run_control(testing::management_mutation_with_request(late, "late-profile-choice"))
        .await
        .is_err()
    );
    assert_eq!(
      owner.reconcile_flow(run, claimed[0].record.id, at(106)).await.unwrap(),
      FactoryFlowStep::Cancelled
    );
    assert_eq!(cancelled, store.factory_run_snapshot(run).await.unwrap());
    assert!(
      store
        .claim_factory_runs(ClaimFactoryRuns::new(key("observer"), at(107), at(200), 1).unwrap())
        .await
        .unwrap()
        .is_empty()
    );
  });
}

#[test]
fn expired_profile_choices_cannot_wait_in_or_reenter_the_phase_pool() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = pooled_fixture();
    let run = admission.run.id();
    let project = admission.work.subject().project_id();
    let policy = admission.flow.pool_settings()[&key("shared.ready")].policy.clone();
    let choices = testing::factory_profile_selection_capabilities(&admission.flow);
    let ownership = FactoryClaimOwnership::new(
      ownership.owner().clone(),
      FactoryClaim::new(ownership.claim().fence(), at(10), at(2000)).unwrap(),
    );
    let (store, claim) = start(admission, ownership).await;
    store.seed_project(project).unwrap();
    store.enable_project(project, choices).unwrap();
    let inputs = Arc::new(FrozenInput {
      input,
      calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      inputs,
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    owner.reconcile_flow(run, claim.id, at(11)).await.unwrap();
    assert!(matches!(
      owner.reconcile_flow(run, claim.id, at(12)).await.unwrap(),
      FactoryFlowStep::Ready(_)
    ));
    let before = store.factory_run_snapshot(run).await.unwrap();
    assert!(
      owner.reconcile_flow(run, claim.id, at(1011)).await.is_err(),
      "queue publication cannot keep an expired choice actionable"
    );
    assert_eq!(before, store.factory_run_snapshot(run).await.unwrap());
    let leased = store
      .select_phase_ready(SelectPhasePool {
        policy_digest: policy.digest(),
        request_id: FactoryDigest::from_bytes([5; 32]),
        owner: key("late-pool-worker"),
        observed_at: at(2001),
        expires_at: at(2100),
        capabilities: Default::default(),
        limit: 1,
      })
      .await
      .unwrap();
    assert!(
      leased.is_empty(),
      "an old ready entry cannot acquire capacity after its profile deadline"
    );
  });
}

#[test]
fn an_expired_undispatched_profile_releases_its_phase_pool_reservation() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = pooled_fixture();
    let run = admission.run.id();
    let project = admission.work.subject().project_id();
    let policy = admission.flow.pool_settings()[&key("shared.ready")].policy.clone();
    let choices = testing::factory_profile_selection_capabilities(&admission.flow);
    let (store, claim) = start(admission, ownership).await;
    store.seed_project(project).unwrap();
    store.enable_project(project, choices).unwrap();
    let inputs = Arc::new(FrozenInput {
      input,
      calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      inputs,
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    owner.reconcile_flow(run, claim.id, at(11)).await.unwrap();
    assert!(matches!(
      owner.reconcile_flow(run, claim.id, at(12)).await.unwrap(),
      FactoryFlowStep::Ready(_)
    ));
    let leased = store
      .select_phase_ready(SelectPhasePool {
        policy_digest: policy.digest(),
        request_id: FactoryDigest::from_bytes([6; 32]),
        owner: key("pool-worker"),
        observed_at: at(101),
        expires_at: at(2000),
        capabilities: Default::default(),
        limit: 1,
      })
      .await
      .unwrap();
    assert_eq!(leased.len(), 1);
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    assert!(octacity_server_store::phase_pool_selection_active(
      &leased[0],
      &snapshot,
      at(1010)
    ));
    assert!(
      !octacity_server_store::phase_pool_selection_active(&leased[0], &snapshot, at(1011)),
      "no execution exists to retain capacity after the selection deadline"
    );
    assert!(snapshot.flow.attempts.is_empty());
    assert!(snapshot.flow.data.build_intents.is_empty());
    assert_eq!(snapshot.budgets.last().unwrap().usage, BudgetUsage::default());
  });
}

#[test]
fn a_dispatched_execution_failure_never_reenters_manual_or_reserve_selection() {
  crate::factory_admission_tests::run_ready(async {
    for failure in [
      FlowProfileSelectionFailure::ManualSelection,
      FlowProfileSelectionFailure::ReserveProfile {
        candidate: key("specialized"),
      },
    ] {
      let (admission, input, ownership) = testing::factory_profile_selection_failure_contract_fixture(
        FlowProfileSelectionPolicy::Manual {
          candidate: Some(key("advanced")),
        },
        failure,
      );
      let run = admission.run.id();
      let project = admission.work.subject().project_id();
      let choices = testing::factory_profile_selection_capabilities(&admission.flow);
      let (store, claim) = start(admission, ownership).await;
      store.seed_project(project).unwrap();
      store.enable_project(project, choices).unwrap();
      let target = FactoryNodeTarget {
        flow_run_id: input.flow_run_id(),
        cycle_id: input.cycle_id(),
        node: input.node().clone(),
      };
      let executor = Arc::new(crate::factory_node_failure_tests::FailedExecution {
        ids: (
          octacity_server_domain::BuildId::generate(),
          octacity_server_domain::AttemptId::generate(),
          octacity_server_domain::JobId::generate(),
        ),
      });
      let owner = FactoryNodeRunner::new(store.clone(), executor, crate::factory_node_test_support::clock())
        .with_profile_catalog(store.clone());
      owner
        .run_once(run, claim.id, target.clone(), Some(input), at(11))
        .await
        .unwrap();
      owner
        .run_once(run, claim.id, target.clone(), None, at(12))
        .await
        .unwrap();
      let committed = store.factory_run_snapshot(run).await.unwrap();
      assert_eq!(
        owner
          .run_once(run, claim.id, target.clone(), None, at(13))
          .await
          .unwrap(),
        FactoryNodeStep::ExecutionStopped
      );
      let stopped = store.factory_run_snapshot(run).await.unwrap();
      assert_eq!(
        stopped.flow.data.profile_selections,
        committed.flow.data.profile_selections
      );
      assert_eq!(stopped.flow.data.build_intents, committed.flow.data.build_intents);
      assert_eq!(stopped.flow.data.stops[0].reason(), FlowNodeStopReason::ExecutionFailed);
      let usage = stopped
        .budgets
        .iter()
        .find(|row| row.id == stopped.current.budget_id)
        .unwrap()
        .usage;
      assert_eq!(usage.attempts, 1);
      assert_eq!(usage.tokens, 23);
      let restarted = FactoryNodeRunner::new(
        store.clone(),
        Arc::new(NoExecution),
        crate::factory_node_test_support::clock(),
      );
      assert_eq!(
        restarted.run_once(run, claim.id, target, None, at(14)).await.unwrap(),
        FactoryNodeStep::ExecutionStopped
      );
      assert_eq!(
        stopped,
        store.factory_run_snapshot(run).await.unwrap(),
        "recovery cannot choose a different profile or charge failed execution again"
      );
    }
  });
}

#[test]
fn profile_revalidation_cannot_allocate_an_attempt_after_its_deadline() {
  struct DeadlineClock(std::sync::atomic::AtomicBool);
  impl FactoryExecutionClock for DeadlineClock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(at(if self.0.swap(true, std::sync::atomic::Ordering::SeqCst) {
        1011
      } else {
        1000
      }))
    }
  }
  crate::factory_admission_tests::run_ready(async {
    for manual_response in [false, true] {
      let (admission, input, ownership) = fixture(if manual_response { None } else { Some("specialized") });
      let profile = admission.flow.closure().definitions()[0].nodes()[0]
        .profile_selection()
        .unwrap()
        .candidates[&key("specialized")]
        .profile
        .reference()
        .clone();
      let run = admission.run.id();
      let project = admission.work.subject().project_id();
      let choices = testing::factory_profile_selection_capabilities(&admission.flow);
      let ownership = FactoryClaimOwnership::new(
        ownership.owner().clone(),
        FactoryClaim::new(ownership.claim().fence(), at(10), at(2000)).unwrap(),
      );
      let (store, mut claim) = start(admission, ownership).await;
      store.seed_project(project).unwrap();
      store.enable_project(project, choices).unwrap();
      let target = FactoryNodeTarget {
        flow_run_id: input.flow_run_id(),
        cycle_id: input.cycle_id(),
        node: input.node().clone(),
      };
      let initial = FactoryNodeRunner::new(
        store.clone(),
        Arc::new(NoExecution),
        crate::factory_node_test_support::clock(),
      );
      initial
        .run_once(run, claim.id, target.clone(), Some(input.clone()), at(11))
        .await
        .unwrap();
      if manual_response {
        let pending = store.factory_run_snapshot(run).await.unwrap();
        store
          .apply_factory_run_control(testing::management_mutation_with_request(
            ApplyFactoryRunControl {
              run_id: run,
              expected_version: pending.run.version(),
              idempotency_key: IdempotencyKey::new("deadline-choice").unwrap(),
              requested_at: at(12),
              intent: FactoryRunControlIntent::SelectProfile {
                input_digest: input.digest().unwrap(),
                profile,
              },
            },
            "deadline-choice",
          ))
          .await
          .unwrap();
        claim = store
          .claim_factory_runs(ClaimFactoryRuns::new(key("deadline-owner"), at(13), at(2000), 1).unwrap())
          .await
          .unwrap()
          .remove(0)
          .record;
      }
      let before = store.factory_run_snapshot(run).await.unwrap();
      let owner = FactoryNodeRunner::new(
        store.clone(),
        Arc::new(NoExecution),
        Arc::new(DeadlineClock(std::sync::atomic::AtomicBool::new(false))),
      )
      .with_profile_catalog(store.clone());
      assert!(
        owner.run_once(run, claim.id, target, None, at(1000)).await.is_err(),
        "catalog revalidation cannot commit a new execution at the exclusive deadline"
      );
      assert_eq!(
        before,
        store.factory_run_snapshot(run).await.unwrap(),
        "the expired result spends no attempt or reservation"
      );
    }
  });
}
