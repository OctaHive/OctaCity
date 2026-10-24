use super::*;
use octacity_server_factory::*;
use octacity_server_store::{PublishedFactoryAdmission, testing as store_testing};

fn selector_binding(
  plan: crate::FactoryDecisionSignalRequestPlan,
  project_id: octacity_server_domain::ProjectId,
  provider: Arc<CountingProvider>,
) -> crate::FactoryProfileSelector {
  crate::FactoryProfileSelector {
    selection_policy: plan.profile.policy().clone(),
    project_id,
    plan,
    baseline: key("baseline"),
    provider,
    credential_profile: None,
    project_permissions: FactoryPermissionSet::deny_all(),
    local_permissions: LocalPermissionCeiling::fully_enforced(FactoryPermissionSet::deny_all()),
  }
  .pin_policy()
  .unwrap()
}

fn fixture() -> (
  PublishedFactoryAdmission,
  FlowNodeInput,
  FactoryClaimOwnership,
  crate::FactoryDecisionSignalRequestPlan,
) {
  fixture_with_failure(FlowProfileSelectionFailure::ManualSelection)
}

fn fixture_with_failure(
  failure: FlowProfileSelectionFailure,
) -> (
  PublishedFactoryAdmission,
  FlowNodeInput,
  FactoryClaimOwnership,
  crate::FactoryDecisionSignalRequestPlan,
) {
  let template = provider_request(DecisionSignalPurpose::Routing, DecisionSignalMode::BoundedControl);
  let mut plan = signal_plan_fixture(&template);
  plan.profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::ProfileSelection,
    provider: template.provider().clone(),
    adapter: template.adapter().clone(),
    model: template.model().clone(),
    question_set: template.question_set().clone(),
    policy: template.policy().clone(),
    mode: template.mode(),
    fallback: template.fallback(),
    budget: template.request().budget(),
    routes: None,
  })
  .unwrap();
  let domain = DecisionSignalChoices::try_new(["baseline", "specialized", "advanced"].map(key).into()).unwrap();
  plan.questions = vec![
    DecisionSignalQuestion::new(
      exact("profiles", "v1", 21),
      FactoryText::new("Choose an eligible profile from the declared criteria.").unwrap(),
      DecisionSignalQuestionDomain::FiniteChoice(domain.clone()),
      DecisionSignalQuestionCriteria::FiniteChoice(
        domain
          .as_slice()
          .iter()
          .map(|c| {
            DecisionSignalChoiceCriterion::new(
              c.clone(),
              FactoryText::new(format!("Use {c} for its declared role.")).unwrap(),
            )
          })
          .collect(),
      ),
    )
    .unwrap(),
  ];
  plan.consuming_question = key("profiles");
  plan.threshold = DecisionSignalThreshold::new(
    DecisionSignalPurpose::ProfileSelection,
    template.probability_semantics().clone(),
    exact("profile.calibration", "v1", 22),
    DecisionSignalProbability::new(600_000).unwrap(),
    DecisionSignalProbability::new(100_000).unwrap(),
  )
  .unwrap();
  plan.capability = DecisionSignalProviderCapability::new(
    template.provider().clone(),
    template.adapter().clone(),
    template.model().clone(),
    template.probability_semantics().clone(),
    template.data_handling().clone(),
    vec![DecisionSignalPurpose::ProfileSelection],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1024, 20, 24).unwrap(),
    budget(),
  )
  .unwrap();
  let criteria = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("{\"count\":42}").unwrap(),
    plan.questions.clone(),
    plan.consuming_question.clone(),
    None,
  )
  .unwrap();
  plan.threshold = plan
    .threshold
    .qualify_profile_selection(&plan.profile, &criteria)
    .unwrap();
  let sample = provider_request(
    DecisionSignalPurpose::ProfileSelection,
    DecisionSignalMode::BoundedControl,
  );
  let policy = selector_binding(
    plan.clone(),
    octacity_server_domain::ProjectId::generate(),
    Arc::new(CountingProvider::successful(&sample, "right")),
  )
  .selection_policy;
  let (admission, input, ownership) = store_testing::factory_profile_signal_contract_fixture(
    FlowProfileSelectionPolicy::DecisionApi {
      policy,
      input: plan.state_projection.clone(),
    },
    failure,
    BudgetUsage {
      attempts: 0,
      elapsed_millis: 100,
      tokens: 10,
      cost_micro_units: 100,
      output_bytes: 100,
    },
  );
  (admission, input, ownership, plan)
}

#[test]
fn profile_request_is_frozen_and_reserved_before_consuming_a_receipt() {
  let (admission, input, _, plan) = fixture();
  let pending =
    FlowProfileSelectionRecord::select(&input, &admission.flow, Timestamp::from_unix_millis(11).unwrap()).unwrap();
  let request = plan
    .prepare_profile_selection(&input, &admission.flow, &pending)
    .unwrap();
  assert_eq!(
    request,
    plan
      .prepare_profile_selection(&input, &admission.flow, &pending)
      .unwrap()
  );
  assert_eq!(request.input().state_document().as_str(), "{\"count\":42}");
  let prepared = pending
    .prepare_signal(
      &input,
      &admission.flow,
      request.clone(),
      key("baseline"),
      Timestamp::from_unix_millis(12).unwrap(),
    )
    .unwrap();
  let receipt = consume_profile_selection_signal(
    DecisionSignalReceiptId::generate(),
    &request,
    DecisionSignalProviderObservation::Result(result(&request, "specialized")),
    key("baseline"),
  )
  .unwrap();
  assert!(
    prepared
      .consume_signal(
        &input,
        &admission.flow,
        receipt.clone(),
        Timestamp::from_unix_millis(14).unwrap()
      )
      .is_err()
  );
  let reserved = prepared
    .reserve_signal(Timestamp::from_unix_millis(13).unwrap())
    .unwrap();
  let selected = reserved
    .consume_signal(
      &input,
      &admission.flow,
      receipt,
      Timestamp::from_unix_millis(14).unwrap(),
    )
    .unwrap();
  assert_eq!(selected.candidate(), Some(&key("specialized")));
  assert_eq!(selected.deadline(), pending.deadline());
  assert_eq!(selected.started_at(), pending.started_at());
  assert_eq!(
    FlowProfileSelectionRecord::restore(
      &serde_json::to_vec(&selected).unwrap(),
      &input,
      &admission.flow,
      selected.digest().unwrap()
    )
    .unwrap(),
    selected
  );
  assert!(
    reserved
      .reserve_signal(Timestamp::from_unix_millis(14).unwrap())
      .is_err()
  );
  let extended = DecisionSignalProviderRequest::new(
    request.request().clone(),
    &plan.profile,
    &plan.capability,
    request.input().clone(),
    plan.threshold.clone(),
    pending.deadline(),
  )
  .unwrap();
  assert!(
    pending
      .prepare_signal(
        &input,
        &admission.flow,
        extended,
        key("baseline"),
        Timestamp::from_unix_millis(12).unwrap()
      )
      .is_err(),
    "a retained request cannot extend its original call window to the enclosing stage deadline"
  );
  let (other, other_input, _, other_plan) = fixture();
  let other_pending =
    FlowProfileSelectionRecord::select(&other_input, &other.flow, Timestamp::from_unix_millis(11).unwrap()).unwrap();
  let other_request = other_plan
    .prepare_profile_selection(&other_input, &other.flow, &other_pending)
    .unwrap();
  assert!(
    pending
      .prepare_signal(
        &other_input,
        &other.flow,
        other_request,
        key("baseline"),
        Timestamp::from_unix_millis(12).unwrap()
      )
      .is_err(),
    "a selection cannot borrow another stage's request even when its plan and candidates match"
  );
}

#[test]
fn profile_selection_runs_through_the_common_fenced_journal_before_execution() {
  use crate::*;
  use octacity_server_store::FactoryRunStore;
  run_ready(async {
    let (admission, input, ownership, plan) = fixture();
    let pending =
      FlowProfileSelectionRecord::select(&input, &admission.flow, Timestamp::from_unix_millis(11).unwrap()).unwrap();
    let request = plan
      .prepare_profile_selection(&input, &admission.flow, &pending)
      .unwrap();
    let provider = Arc::new(CountingProvider::successful(&request, "specialized"));
    let (store, claim) = crate::factory_node_tests::profile_selection::start(admission.clone(), ownership).await;
    let clock = crate::factory_node_test_support::clock();
    let selector = Arc::new(
      FactoryProfileSelectors::new(
        store.clone(),
        vec![selector_binding(plan, input.subject().project_id(), provider.clone())],
        clock.clone(),
      )
      .unwrap(),
    );
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    for at in [11, 12, 13] {
      let runner = FactoryNodeRunner::new(
        store.clone(),
        Arc::new(crate::factory_node_tests::profile_selection::NoExecution),
        clock.clone(),
      )
      .with_profile_selectors(selector.clone());
      assert_eq!(
        runner
          .run_once(
            input.factory_run_id(),
            claim.id,
            target.clone(),
            Some(input.clone()),
            Timestamp::from_unix_millis(at).unwrap()
          )
          .await
          .unwrap_or_else(|error| panic!("profile selection at {at}: {error:?}")),
        FactoryNodeStep::Advanced
      );
    }
    let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    assert!(snapshot.flow.attempts.is_empty());
    assert!(snapshot.flow.data.build_intents.is_empty());
    let selected = snapshot
      .flow
      .data
      .current_profile_selection(input.digest().unwrap())
      .unwrap();
    assert_eq!(selected.candidate(), Some(&key("specialized")));
    assert_eq!(selected.signal().unwrap().request(), &request);
    assert_eq!(provider.dispatches(), 1);
    assert_eq!(selected.signal().unwrap().receipt().unwrap().usage().tokens, 10);
    let usage = snapshot
      .budgets
      .iter()
      .find(|b| b.id == snapshot.current.budget_id)
      .unwrap()
      .usage;
    assert_eq!(usage.tokens, 10, "selection cost belongs to the ordinary Run budget");
    assert_eq!(
      usage.attempts, 0,
      "provider selection does not spend an execution attempt"
    );
    store.seed_project(input.subject().project_id()).unwrap();
    store
      .enable_project(
        input.subject().project_id(),
        store_testing::factory_profile_selection_capabilities(&admission.flow),
      )
      .unwrap();

    let runner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(crate::factory_node_tests::profile_selection::NoExecution),
      clock,
    )
    .with_profile_catalog(store.clone())
    .with_profile_selectors(selector);
    assert_eq!(
      runner
        .run_once(
          input.factory_run_id(),
          claim.id,
          target,
          None,
          Timestamp::from_unix_millis(14).unwrap()
        )
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let dispatched = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    assert_eq!(dispatched.flow.data.build_intents.len(), 1);
    assert_eq!(dispatched.flow.data.build_intents[0].usage_before().tokens, 10);
  });
}

#[test]
fn a_failed_profile_receipt_reaches_manual_recovery_without_another_query() {
  use crate::*;
  use octacity_server_store::FactoryRunStore;
  run_ready(async {
    for scenario in ["unavailable", "unknown", "undeclared", "confidence"] {
      let (admission, input, ownership, plan) = fixture();
      let pending =
        FlowProfileSelectionRecord::select(&input, &admission.flow, Timestamp::from_unix_millis(11).unwrap()).unwrap();
      let request = plan
        .prepare_profile_selection(&input, &admission.flow, &pending)
        .unwrap();
      let provider = Arc::new(CountingProvider::failing(&request));
      match scenario {
        "unknown" => {
          *provider.result.lock().unwrap() = Some(Err(
            DecisionSignalProviderFailure::unobserved(
              request.id(),
              request.provider().clone(),
              request.adapter().clone(),
              request.model().clone(),
              DecisionSignalState::Unavailable,
            )
            .unwrap(),
          ))
        }
        "undeclared" => *provider.result.lock().unwrap() = Some(Ok(result(&request, "not-a-profile"))),
        "confidence" => {
          let mut low = serde_json::to_value(result(&request, "specialized")).unwrap();
          low["answers"][0]["value"]["FiniteChoice"]["selected_probability"] = serde_json::json!(500_000);
          *provider.result.lock().unwrap() = Some(Ok(serde_json::from_value(low).unwrap()));
        }
        _ => (),
      }
      let (store, claim) = crate::factory_node_tests::profile_selection::start(admission.clone(), ownership).await;
      let clock = crate::factory_node_test_support::clock();
      let selector = Arc::new(
        FactoryProfileSelectors::new(
          store.clone(),
          vec![selector_binding(plan, input.subject().project_id(), provider.clone())],
          clock.clone(),
        )
        .unwrap(),
      );
      let target = FactoryNodeTarget {
        flow_run_id: input.flow_run_id(),
        cycle_id: input.cycle_id(),
        node: input.node().clone(),
      };
      for at in [11, 12, 13, 14] {
        let runner = FactoryNodeRunner::new(
          store.clone(),
          Arc::new(crate::factory_node_tests::profile_selection::NoExecution),
          clock.clone(),
        )
        .with_profile_selectors(selector.clone());
        assert_eq!(
          runner
            .run_once(
              input.factory_run_id(),
              claim.id,
              target.clone(),
              Some(input.clone()),
              Timestamp::from_unix_millis(at).unwrap()
            )
            .await
            .unwrap(),
          FactoryNodeStep::Advanced
        );
      }
      let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
      let selection = snapshot
        .flow
        .data
        .current_profile_selection(input.digest().unwrap())
        .unwrap();
      assert_eq!(
        selection.recovery(),
        Some(FlowProfileSelectionRecovery::ManualSelection)
      );
      if scenario == "unknown" {
        assert_eq!(
          serde_json::to_value(selection.signal().unwrap()).unwrap()["unobservable"],
          serde_json::json!(true)
        );
        assert!(selection.signal().unwrap().receipt().is_none());
      } else {
        let receipt = selection.signal().unwrap().receipt().unwrap();
        assert_eq!(
          receipt.state(),
          match scenario {
            "undeclared" => DecisionSignalState::Invalid,
            "confidence" => DecisionSignalState::Completed,
            _ => DecisionSignalState::Unavailable,
          }
        );
      }
      let restarted = FactoryNodeRunner::new(
        store.clone(),
        Arc::new(crate::factory_node_tests::profile_selection::NoExecution),
        clock,
      )
      .with_profile_selectors(selector);
      assert_eq!(
        restarted
          .run_once(
            input.factory_run_id(),
            claim.id,
            target,
            None,
            Timestamp::from_unix_millis(15).unwrap()
          )
          .await
          .unwrap(),
        FactoryNodeStep::Waiting
      );
      assert_eq!(provider.dispatches(), 1);
      assert!(snapshot.flow.attempts.is_empty());
    }
  });
}

#[test]
fn profile_requests_cannot_exceed_the_declared_selection_allowance() {
  let (admission, input, _, mut plan) = fixture();
  let pending =
    FlowProfileSelectionRecord::select(&input, &admission.flow, Timestamp::from_unix_millis(11).unwrap()).unwrap();
  plan.call_budget = BudgetLimit::new(1, 100, 11, 100, 100).unwrap();
  assert!(
    plan
      .prepare_profile_selection(&input, &admission.flow, &pending)
      .is_err()
  );
}

#[test]
fn an_unobservable_advisory_signal_keeps_the_pinned_baseline() {
  for mode in [DecisionSignalMode::Shadow, DecisionSignalMode::Advisory] {
    let (admission, input, _, mut plan) = fixture();
    let mut profile = serde_json::to_value(&plan.profile).unwrap();
    profile["mode"] = serde_json::to_value(mode).unwrap();
    plan.profile = serde_json::from_value(profile).unwrap();
    let pending =
      FlowProfileSelectionRecord::select(&input, &admission.flow, Timestamp::from_unix_millis(11).unwrap()).unwrap();
    let request = plan
      .prepare_profile_selection(&input, &admission.flow, &pending)
      .unwrap();
    let uncertain = pending
      .prepare_signal(
        &input,
        &admission.flow,
        request,
        key("baseline"),
        Timestamp::from_unix_millis(12).unwrap(),
      )
      .unwrap()
      .reserve_signal(Timestamp::from_unix_millis(13).unwrap())
      .unwrap()
      .mark_signal_unobservable(Timestamp::from_unix_millis(14).unwrap())
      .unwrap();
    assert_eq!(uncertain.candidate(), Some(&key("baseline")));
    assert!(!uncertain.needs_recovery());
    assert_eq!(
      uncertain.signal_usage().tokens,
      10,
      "unknown usage retains the reservation"
    );
  }
}

#[test]
fn profile_dispatch_rechecks_budget_after_the_request_was_prepared() {
  use crate::*;
  use octacity_server_store::*;
  run_ready(async {
    let (admission, input, ownership, plan) = fixture();
    let pending =
      FlowProfileSelectionRecord::select(&input, &admission.flow, Timestamp::from_unix_millis(11).unwrap()).unwrap();
    let request = plan
      .prepare_profile_selection(&input, &admission.flow, &pending)
      .unwrap();
    let provider = Arc::new(CountingProvider::successful(&request, "specialized"));
    let (store, claim) = crate::factory_node_tests::profile_selection::start(admission, ownership).await;
    let clock = crate::factory_node_test_support::clock();
    let selectors = Arc::new(
      FactoryProfileSelectors::new(
        store.clone(),
        vec![selector_binding(plan, input.subject().project_id(), provider.clone())],
        clock.clone(),
      )
      .unwrap(),
    );
    let runner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(crate::factory_node_tests::profile_selection::NoExecution),
      clock,
    )
    .with_profile_selectors(selectors.clone());
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    for at in [11, 12] {
      runner
        .run_once(
          input.factory_run_id(),
          claim.id,
          target.clone(),
          Some(input.clone()),
          Timestamp::from_unix_millis(at).unwrap(),
        )
        .await
        .unwrap();
    }
    let before = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    let mut cost = store_testing::factory_flow_contract_transition(&before, &claim, 13);
    cost.budget = FactoryBudgetRecord::new(
      input.factory_run_id(),
      cost.next_run.version(),
      BudgetUsage {
        tokens: before.admitted_flow.limits().budget().max_tokens(),
        ..Default::default()
      },
      Timestamp::from_unix_millis(13).unwrap(),
    );
    cost.current.budget_id = cost.budget.id;
    store.commit_factory_run_transition(cost).await.unwrap();
    assert!(
      selectors
        .advance(
          &before,
          &input,
          before
            .flow
            .data
            .current_profile_selection(input.digest().unwrap())
            .unwrap(),
          Timestamp::from_unix_millis(14).unwrap()
        )
        .await
        .is_err()
    );
    assert_eq!(
      provider.dispatches(),
      0,
      "fresh budget authority is required before external I/O"
    );
  });
}

#[test]
fn a_manual_choice_after_restart_fences_the_reserved_profile_operation() {
  use crate::*;
  use octacity_server_store::*;
  run_ready(async {
    let (admission, input, ownership, plan) = fixture();
    let at = Timestamp::from_unix_millis(11).unwrap();
    let pending = FlowProfileSelectionRecord::select(&input, &admission.flow, at).unwrap();
    let request = plan
      .prepare_profile_selection(&input, &admission.flow, &pending)
      .unwrap();
    let prepared = pending
      .prepare_signal(&input, &admission.flow, request.clone(), key("baseline"), at)
      .unwrap();
    let reserved = prepared.reserve_signal(at).unwrap();
    let (store, claim) = crate::factory_node_tests::profile_selection::start(admission.clone(), ownership).await;
    let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    let mut freeze = store_testing::factory_flow_contract_transition(&snapshot, &claim, 11);
    freeze.append.flow.data.inputs.push(input.clone());
    freeze
      .append
      .flow
      .data
      .profile_selections
      .extend([pending, prepared, reserved]);
    store.commit_factory_run_transition(freeze).await.unwrap();
    let before = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    let profile = &admission.flow.closure().definitions()[0].nodes()[0]
      .profile_selection()
      .unwrap()
      .candidates[&key("advanced")]
      .profile;
    store
      .apply_factory_run_control(store_testing::management_mutation_with_request(
        ApplyFactoryRunControl {
          run_id: input.factory_run_id(),
          expected_version: before.run.version(),
          idempotency_key: IdempotencyKey::new("manual-after-selector-crash").unwrap(),
          requested_at: Timestamp::from_unix_millis(12).unwrap(),
          intent: FactoryRunControlIntent::SelectProfile {
            input_digest: input.digest().unwrap(),
            profile: profile.reference().clone(),
          },
        },
        "manual-after-selector-crash",
      ))
      .await
      .unwrap();
    store.seed_project(input.subject().project_id()).unwrap();
    store
      .enable_project(
        input.subject().project_id(),
        store_testing::factory_profile_selection_capabilities(&admission.flow),
      )
      .unwrap();
    let claim = store
      .claim_factory_runs(
        ClaimFactoryRuns::new(
          key("profile-restarted"),
          Timestamp::from_unix_millis(13).unwrap(),
          Timestamp::from_unix_millis(100).unwrap(),
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap()
      .pop()
      .unwrap()
      .record;
    let runner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(crate::factory_node_tests::profile_selection::NoExecution),
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    runner
      .run_once(
        input.factory_run_id(),
        claim.id,
        target,
        None,
        Timestamp::from_unix_millis(13).unwrap(),
      )
      .await
      .unwrap();
    let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    let chosen = snapshot
      .flow
      .data
      .current_profile_selection(input.digest().unwrap())
      .unwrap();
    assert_eq!(chosen.candidate(), Some(&key("advanced")));
    assert!(chosen.signal().unwrap().unobservable());
    let late = consume_profile_selection_signal(
      DecisionSignalReceiptId::generate(),
      &request,
      DecisionSignalProviderObservation::Result(result(&request, "specialized")),
      key("baseline"),
    )
    .unwrap();
    assert!(
      chosen
        .consume_signal(&input, &admission.flow, late, Timestamp::from_unix_millis(14).unwrap())
        .is_err()
    );
    assert!(snapshot.flow.attempts.is_empty());
  });
}

#[test]
fn a_missing_selector_uses_the_existing_stage_failure_policy() {
  use crate::*;
  use octacity_server_store::FactoryRunStore;
  run_ready(async {
    for reserve in [false, true] {
      let (admission, input, ownership, _) = fixture_with_failure(if reserve {
        FlowProfileSelectionFailure::ReserveProfile {
          candidate: key("specialized"),
        }
      } else {
        FlowProfileSelectionFailure::ManualSelection
      });
      let choices = store_testing::factory_profile_selection_capabilities(&admission.flow);
      let (store, claim) = crate::factory_node_tests::profile_selection::start(admission, ownership).await;
      store.seed_project(input.subject().project_id()).unwrap();
      store.enable_project(input.subject().project_id(), choices).unwrap();
      let runner = FactoryNodeRunner::new(
        store.clone(),
        Arc::new(crate::factory_node_tests::profile_selection::NoExecution),
        crate::factory_node_test_support::clock(),
      )
      .with_profile_catalog(store.clone());
      let target = FactoryNodeTarget {
        flow_run_id: input.flow_run_id(),
        cycle_id: input.cycle_id(),
        node: input.node().clone(),
      };
      for at in [11, 12, 13] {
        assert_eq!(
          runner
            .run_once(
              input.factory_run_id(),
              claim.id,
              target.clone(),
              Some(input.clone()),
              Timestamp::from_unix_millis(at).unwrap()
            )
            .await
            .unwrap(),
          FactoryNodeStep::Advanced
        );
      }
      let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
      let selection = snapshot
        .flow
        .data
        .current_profile_selection(input.digest().unwrap())
        .unwrap();
      assert_eq!(selection.candidate(), reserve.then(|| key("specialized")).as_ref());
      assert_eq!(
        serde_json::to_value(selection).unwrap()["selector_error"],
        serde_json::json!("unavailable")
      );
      assert!(snapshot.flow.attempts.is_empty());
    }
  });
}

#[test]
fn an_installed_selector_cannot_retarget_a_pinned_policy() {
  use crate::*;
  let (admission, input, _, plan) = fixture();
  let pending =
    FlowProfileSelectionRecord::select(&input, &admission.flow, Timestamp::from_unix_millis(11).unwrap()).unwrap();
  let request = plan
    .prepare_profile_selection(&input, &admission.flow, &pending)
    .unwrap();
  let mut selector = FactoryProfileSelector {
    selection_policy: plan.profile.policy().clone(),
    project_id: input.subject().project_id(),
    plan,
    baseline: key("baseline"),
    provider: Arc::new(CountingProvider::successful(&request, "specialized")),
    credential_profile: None,
    project_permissions: FactoryPermissionSet::deny_all(),
    local_permissions: LocalPermissionCeiling::fully_enforced(FactoryPermissionSet::deny_all()),
  }
  .pin_policy()
  .unwrap();
  selector.validate().unwrap();
  let mut changed = serde_json::to_value(&selector.plan.profile).unwrap();
  changed["mode"] = serde_json::json!("shadow");
  selector.plan.profile = serde_json::from_value(changed).unwrap();
  assert!(
    selector.validate().is_err(),
    "the same installed policy cannot silently change earlier Runs' rollout"
  );
}

#[test]
fn an_unresolved_profile_call_keeps_its_run_budget_reservation() {
  use octacity_server_store::*;
  run_ready(async {
    let (admission, input, ownership, plan) = fixture();
    let at = Timestamp::from_unix_millis(11).unwrap();
    let pending = FlowProfileSelectionRecord::select(&input, &admission.flow, at).unwrap();
    let request = plan
      .prepare_profile_selection(&input, &admission.flow, &pending)
      .unwrap();
    let prepared = pending
      .prepare_signal(&input, &admission.flow, request, key("baseline"), at)
      .unwrap();
    let reserved = prepared.reserve_signal(at).unwrap();
    let (store, claim) = crate::factory_node_tests::profile_selection::start(admission, ownership).await;
    let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    let mut freeze = store_testing::factory_flow_contract_transition(&snapshot, &claim, 11);
    freeze.append.flow.data.inputs.push(input.clone());
    freeze
      .append
      .flow
      .data
      .profile_selections
      .extend([pending, prepared, reserved]);
    store.commit_factory_run_transition(freeze).await.unwrap();
    let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    let mut spent = store_testing::factory_flow_contract_transition(&snapshot, &claim, 12);
    spent.budget = FactoryBudgetRecord::new(
      input.factory_run_id(),
      spent.next_run.version(),
      BudgetUsage {
        tokens: snapshot.admitted_flow.limits().budget().max_tokens(),
        ..Default::default()
      },
      Timestamp::from_unix_millis(12).unwrap(),
    );
    spent.current.budget_id = spent.budget.id;
    assert!(
      store.commit_factory_run_transition(spent).await.is_err(),
      "an unresolved external operation holds its ceiling until measured settlement or explicit uncertainty"
    );
  });
}

#[test]
fn profile_selection_cost_counts_against_its_flow_before_the_next_stage() {
  use octacity_server_store::*;
  run_ready(async {
    let (mut admission, original, ownership, plan) = fixture();
    let root = admission.flow.closure().definition(original.definition()).unwrap();
    let mut second = serde_json::to_value(&root.nodes()[0]).unwrap();
    second["key"] = serde_json::json!("second_stage");
    second["input_binding"] = serde_json::json!({
      "sources": {"result": {"source": "predecessor", "view": "payload", "path": ""}},
      "mapping": {"value": {"mapping": "pointer", "path": "/result"}}
    });
    let mut execution = serde_json::to_value(root.execution()).unwrap();
    execution["budget"]["max_tokens"] = serde_json::json!(190);
    execution["budget"]["max_output_bytes"] = serde_json::json!(20_000);
    let mut limits = serde_json::to_value(admission.flow.limits()).unwrap();
    limits["budget"]["max_output_bytes"] = serde_json::json!(20_000);
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: root.reference().id(),
      version: root.reference().version(),
      input_schema: root.input_schema().cloned(),
      entry: root.entry().clone(),
      nodes: vec![root.nodes()[0].clone(), serde_json::from_value(second).unwrap()],
      transitions: vec![
        FlowTransition::new(
          root.entry().clone(),
          key("observed"),
          FlowTransitionTarget::Node(key("second_stage")),
        ),
        FlowTransition::new(
          key("second_stage"),
          key("observed"),
          FlowTransitionTarget::Terminal(key("done")),
        ),
      ],
      terminals: root.terminals().to_vec(),
      context_projections: root.context_projections().to_vec(),
      data_projections: root.data_projections().to_vec(),
      execution: serde_json::from_value(execution).unwrap(),
    })
    .unwrap();
    let flow = FlowRun::root(&admission.run, definition.reference()).unwrap();
    let cycle = WorkflowCycle::initial(&flow).unwrap();
    admission.flow = AdmittedFlow::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      serde_json::from_value(limits).unwrap(),
      flow,
      cycle,
      admission.flow.wip_limits(),
    )
    .unwrap()
    .with_data_schemas(admission.flow.data_schemas().to_vec())
    .unwrap();
    let inputs = [original.node().clone(), key("second_stage")].map(|node| {
      FlowNodeInput::new(
        &admission.work,
        &admission.flow,
        admission.flow.root_run(),
        admission.flow.initial_cycle(),
        node,
        original.payload().clone(),
      )
      .unwrap()
      .with_context(original.context().unwrap().clone(), vec![])
      .unwrap()
    });
    let time = |at| Timestamp::from_unix_millis(at).unwrap();
    let signal_records = |input: &FlowNodeInput, at| {
      let pending = FlowProfileSelectionRecord::select(input, &admission.flow, time(at)).unwrap();
      let request = plan
        .prepare_profile_selection(input, &admission.flow, &pending)
        .unwrap();
      let prepared = pending
        .prepare_signal(input, &admission.flow, request.clone(), key("baseline"), time(at + 1))
        .unwrap();
      let reserved = prepared.reserve_signal(time(at + 2)).unwrap();
      let receipt = consume_profile_selection_signal(
        DecisionSignalReceiptId::generate(),
        &request,
        DecisionSignalProviderObservation::Result(result(&request, "specialized")),
        key("baseline"),
      )
      .unwrap();
      let selected = reserved
        .consume_signal(input, &admission.flow, receipt, time(at + 3))
        .unwrap();
      [pending, prepared, reserved, selected]
    };
    let first_signal = signal_records(&inputs[0], 11);
    let second_signal = signal_records(&inputs[1], 16);
    let first_signal_usage = first_signal[3].signal_usage();
    let definition = admission.flow.closure().definition(inputs[0].definition()).unwrap();
    let node = definition.node(inputs[0].node()).unwrap();
    let attempt = NodeAttempt::new(
      admission.flow.root_run(),
      admission.flow.initial_cycle(),
      definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: node.key().clone(),
        node_kind: node.kind(),
        number: NodeAttemptNumber::new(1).unwrap(),
        input_digest: inputs[0].digest().unwrap(),
        budget: node.budget(),
        started_at: time(14),
        deadline: first_signal[3].deadline(),
        execution: NodeExecutionIdentity::External(
          node.profile_selection().unwrap().candidates[&key("specialized")]
            .build
            .plugin
            .clone(),
        ),
        ownership: ownership.clone(),
      },
    )
    .unwrap();
    let completions = [90, 80].map(|tokens| {
      NodeAttemptCompletion::new(
        &attempt,
        definition,
        NodeAttemptCompletionInput {
          outcome: key("observed"),
          output_schema: original.payload().schema().clone(),
          output_digest: FactoryDigest::content_sha256(br#"{"count":42}"#),
          ownership: ownership.clone(),
          usage: BudgetUsage {
            tokens,
            ..Default::default()
          },
          observed_at: time(15),
        },
      )
      .unwrap()
    });
    let (store, _) = crate::factory_node_tests::profile_selection::start(admission, ownership).await;
    let mut snapshot = store.factory_run_snapshot(inputs[0].factory_run_id()).await.unwrap();
    snapshot.flow.attempts.push(attempt);
    snapshot.flow.completions.push(completions[0].clone());
    snapshot.flow.data.inputs.extend(inputs.clone());
    snapshot.flow.data.profile_selections.extend(first_signal);
    let budget = snapshot
      .budgets
      .iter_mut()
      .find(|b| b.id == snapshot.current.budget_id)
      .unwrap();
    budget.usage = first_signal_usage
      .checked_add(BudgetUsage {
        attempts: 1,
        tokens: 90,
        ..Default::default()
      })
      .unwrap();
    assert!(
      !factory_profile_wait_is_open(&snapshot, &inputs[1], time(16)),
      "10 selection + 90 execution leaves only 90 of the Flow's 190 tokens; the next 100-token stage must not start"
    );
    // At the exact boundary, 80 execution + 10 prior selection leaves 100 for stage two.
    // Its own pending/settled selection is part of that stage reservation, never an extra charge.
    snapshot.flow.completions[0] = completions[1].clone();
    for record in second_signal {
      let usage = first_signal_usage
        .checked_add(BudgetUsage {
          attempts: 1,
          tokens: 80,
          ..Default::default()
        })
        .unwrap()
        .checked_add(record.signal_usage())
        .unwrap();
      snapshot.flow.data.profile_selections.push(record);
      snapshot
        .budgets
        .iter_mut()
        .find(|b| b.id == snapshot.current.budget_id)
        .unwrap()
        .usage = usage;
      assert!(
        factory_profile_wait_is_open(&snapshot, &inputs[1], time(20)),
        "the current stage's selection and its retained revisions must not be counted twice"
      );
    }
  });
}

#[test]
fn a_late_profile_response_retains_timeout_and_uses_stage_recovery() {
  use crate::*;
  use octacity_server_store::FactoryRunStore;
  struct Clock(Arc<MutableClock>);
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      DecisionSignalClock::now(self.0.as_ref()).map_err(|_| ApplicationError::unavailable())
    }
  }
  run_ready(async {
    let (admission, input, ownership, plan) = fixture();
    let ownership = FactoryClaimOwnership::new(
      ownership.owner().clone(),
      FactoryClaim::new(
        ownership.claim().fence(),
        ownership.claim().claimed_at(),
        Timestamp::from_unix_millis(200).unwrap(),
      )
      .unwrap(),
    );
    let pending =
      FlowProfileSelectionRecord::select(&input, &admission.flow, Timestamp::from_unix_millis(11).unwrap()).unwrap();
    let request = plan
      .prepare_profile_selection(&input, &admission.flow, &pending)
      .unwrap();
    let time = Arc::new(MutableClock::new(11));
    let provider = Arc::new(CountingProvider::successful_with_completion_time(
      &request,
      "specialized",
      time.clone(),
      request.deadline().unix_millis(),
    ));
    let clock: Arc<dyn FactoryExecutionClock> = Arc::new(Clock(time));
    let (store, claim) = crate::factory_node_tests::profile_selection::start(admission, ownership).await;
    let selectors = Arc::new(
      FactoryProfileSelectors::new(
        store.clone(),
        vec![selector_binding(plan, input.subject().project_id(), provider.clone())],
        clock.clone(),
      )
      .unwrap(),
    );
    let runner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(crate::factory_node_tests::profile_selection::NoExecution),
      clock,
    )
    .with_profile_selectors(selectors);
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    for at in [11, 12, 13, 14] {
      runner
        .run_once(
          input.factory_run_id(),
          claim.id,
          target.clone(),
          Some(input.clone()),
          Timestamp::from_unix_millis(at).unwrap(),
        )
        .await
        .unwrap();
    }
    let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    let selected = snapshot
      .flow
      .data
      .current_profile_selection(input.digest().unwrap())
      .unwrap();
    assert_eq!(selected.recovery(), Some(FlowProfileSelectionRecovery::ManualSelection));
    assert_eq!(selected.deadline(), pending.deadline());
    let receipt = selected.signal().unwrap().receipt().unwrap();
    assert_eq!(receipt.state(), DecisionSignalState::TimedOut);
    assert_eq!(receipt.usage().tokens, 10);
    assert_eq!(provider.dispatches(), 1);
    assert!(snapshot.flow.attempts.is_empty());
  });
}
