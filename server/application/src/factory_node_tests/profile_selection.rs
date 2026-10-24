use super::*;
use octacity_server_store::testing;

use octacity_server_domain::Timestamp;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
use octacity_server_store::testing::factory_profile_selection_contract_fixture as fixture;
pub(crate) struct NoExecution;
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
pub(crate) async fn start(
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
        .unwrap_err()
        .classification(),
      ApplicationFailure::CapabilityUnavailable
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

#[test]
fn ordered_rules_retain_the_first_matching_profile_without_execution() {
  crate::factory_admission_tests::run_ready(async {
    let policy = serde_json::from_value(serde_json::json!({
      "mode":"rules", "rules":[
        {"when":{"operation":"compare","left":{"source":"path","path":"/count"},"comparison":"greater","right":{"source":"literal","value":2}},"candidate":"advanced"},
        {"when":{"operation":"exists","operand":{"source":"path","path":"/count"}},"candidate":"specialized"}
      ],"otherwise":"baseline"
    })).unwrap();
    let (admission, input, ownership) = testing::factory_profile_selection_policy_contract_fixture(policy);
    assert_eq!(input.payload().value(), &serde_json::json!({"count":3}));
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
    let before = store.factory_run_snapshot(run).await.unwrap();
    let choice = &before.flow.data.profile_selections[0];
    assert_eq!(choice.candidate(), Some(&key("advanced")));
    assert_eq!(choice.input_digest(), input.digest().unwrap());
    let bytes = serde_json::to_value(choice).unwrap();
    assert_eq!(bytes["rules"], serde_json::json!({"kind":"matched","rule":0}));
    assert_eq!(
      FlowProfileSelectionRecord::restore(
        &serde_json::to_vec(choice).unwrap(),
        &input,
        &admission.flow,
        choice.digest().unwrap()
      )
      .unwrap(),
      *choice
    );
    assert!(before.flow.attempts.is_empty());
    assert!(before.flow.data.build_intents.is_empty());
    assert_eq!(
      runner
        .run_once(run, claim.id, target, Some(input), at(12))
        .await
        .unwrap_err()
        .classification(),
      ApplicationFailure::CapabilityUnavailable
    );
    assert_eq!(before, store.factory_run_snapshot(run).await.unwrap());
  });
}

#[test]
fn the_flow_owner_retains_one_reserve_or_manual_disposition_without_execution() {
  crate::factory_admission_tests::run_ready(async {
    for (catalog, installed, reserve, expected) in [
      (true, true, true, Some("specialized")),
      (true, false, true, None),
      (false, false, true, None),
      (false, false, false, None),
    ] {
      let policy = serde_json::from_value(serde_json::json!({"mode":"rules","rules":[{
        "when":{"operation":"exists","operand":{"source":"path","path":"/complexity"}},"candidate":"advanced"
      }]}))
      .unwrap();
      let failure = if reserve {
        FlowProfileSelectionFailure::ReserveProfile {
          candidate: key("specialized"),
        }
      } else {
        FlowProfileSelectionFailure::ManualSelection
      };
      let (admission, input, ownership) = testing::factory_profile_selection_failure_contract_fixture(policy, failure);
      let run = admission.run.id();
      let project = admission.work.subject().project_id();
      let choices = testing::factory_profile_selection_capabilities(&admission.flow);
      let (store, claim) = start(admission, ownership).await;
      store.seed_project(project).unwrap();
      if installed {
        store.enable_project(project, choices).unwrap();
      }
      let mut owner = FactoryFlowRunner::new(
        store.clone(),
        Arc::new(NoExecution),
        Arc::new(FrozenInput {
          input,
          calls: std::sync::atomic::AtomicUsize::new(0),
        }),
        crate::factory_node_test_support::clock(),
      );
      if catalog {
        owner = owner.with_profile_catalog(store.clone());
      }
      assert_eq!(
        owner.reconcile_flow(run, claim.id, at(11)).await.unwrap(),
        FactoryFlowStep::Advanced
      );
      let before = store.factory_run_snapshot(run).await.unwrap();
      assert_eq!(before.flow.data.profile_selections.len(), 1);
      let choice = &before.flow.data.profile_selections[0];
      assert_eq!(choice.candidate().map(FactoryKey::as_str), expected);
      assert_eq!(
        choice.recovery(),
        Some(if expected.is_some() {
          FlowProfileSelectionRecovery::ReserveProfile
        } else if reserve {
          FlowProfileSelectionRecovery::ReserveUnavailable
        } else {
          FlowProfileSelectionRecovery::ManualSelection
        })
      );
      assert_eq!(
        choice.rules(),
        Some(&FlowProfileRuleOutcome::Failed {
          reason: FlowPredicateFailure::MissingInput
        })
      );
      assert!(before.flow.attempts.is_empty());
      assert!(before.flow.data.build_intents.is_empty());
      assert_eq!(before.budgets.last().unwrap().usage, BudgetUsage::default());
      store.disable_project(project).unwrap();
      let next = owner.reconcile_flow(run, claim.id, at(12)).await;
      if expected.is_some() {
        assert_eq!(
          next.unwrap_err().classification(),
          ApplicationFailure::CapabilityUnavailable
        );
      } else {
        assert_eq!(next.unwrap(), FactoryFlowStep::Waiting);
      }
      assert_eq!(
        store.factory_run_snapshot(run).await.unwrap(),
        before,
        "eligibility checks cannot reselect or replenish limits"
      );
    }
  });
}

#[test]
fn revoked_profile_capability_blocks_the_owner_without_reselecting_or_relaxing_enforcement() {
  crate::factory_admission_tests::run_ready(async {
    let policy=serde_json::from_value(serde_json::json!({"mode":"rules","rules":[{"when":{"operation":"exists","operand":{"source":"path","path":"/count"}},"candidate":"advanced"}],"otherwise":"baseline"})).unwrap();
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
    store.enable_project(project, choices.clone()).unwrap();
    let inputs = Arc::new(FrozenInput {
      input,
      calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      inputs.clone(),
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    assert_eq!(
      owner.reconcile_flow(run, claim.id, at(11)).await.unwrap(),
      FactoryFlowStep::Advanced
    );
    let before = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(
      before.flow.data.profile_selections[0].candidate(),
      Some(&key("advanced"))
    );
    for enforcement_missing in [false, true] {
      let mut revoked = FactoryConfigurationChoices::try_new(choices.entries()).unwrap();
      if enforcement_missing {
        for (alias, cap) in choices.role_profile_capabilities() {
          let mut cap = cap.clone();
          cap.local_permissions = LocalPermissionCeiling::try_new(FactoryPermissionSet::deny_all(), vec![]).unwrap();
          revoked = revoked.with_role_profile_capability(alias.clone(), cap).unwrap();
        }
      }
      store.enable_project(project, revoked).unwrap();
      let error = owner
        .reconcile_flow(run, claim.id, at(12))
        .await
        .expect_err("a frozen selection is not a current capability grant");
      assert_eq!(error.classification(), ApplicationFailure::CapabilityUnavailable);
      assert_eq!(
        store.factory_run_snapshot(run).await.unwrap(),
        before,
        "revocation cannot choose the reserve or relax permissions"
      );
    }
    store.enable_project(project, choices).unwrap();
    assert_eq!(
      owner.reconcile_flow(run, claim.id, at(13)).await.unwrap(),
      FactoryFlowStep::Advanced
    );
    let dispatched = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(
      dispatched.flow.data.profile_selections,
      before.flow.data.profile_selections
    );
    assert_eq!(dispatched.flow.data.inputs, before.flow.data.inputs);
    assert_eq!(dispatched.flow.data.build_intents.len(), 1);
    assert_eq!(inputs.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
  });
}

#[test]
fn live_manual_choice_is_consumed_once_before_the_new_owner_commits_dispatch() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = fixture(None);
    let run = admission.run.id();
    let project = admission.work.subject().project_id();
    let choices = testing::factory_profile_selection_capabilities(&admission.flow);
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
    let (store, claim) = start(admission, ownership.clone()).await;
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
    let pending = store.factory_run_snapshot(run).await.unwrap();
    let command = ApplyFactoryRunControl {
      run_id: run,
      expected_version: pending.run.version(),
      idempotency_key: IdempotencyKey::new("live-choice").unwrap(),
      requested_at: at(12),
      intent: FactoryRunControlIntent::SelectProfile {
        input_digest: input.digest().unwrap(),
        profile,
      },
    };
    let request = testing::management_mutation_with_request(command, "live-choice");
    let accepted = store.apply_factory_run_control(request.clone()).await.unwrap();
    let recorded = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(recorded.flow, pending.flow);
    assert!(
      owner
        .run_once(run, claim.id, target.clone(), None, at(13))
        .await
        .is_err()
    );
    let successor = FactoryRunClaimRecord::new(
      run,
      ownership.owner().clone(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([52; 32])),
        at(13),
        at(200),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run,
        expected_version: recorded.run.version(),
        record: successor.clone(),
        audit: FactoryAuditFact::new(
          run,
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([52; 32]),
          key("accepted"),
          at(13),
        ),
      })
      .await
      .unwrap();
    let restarted = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    assert_eq!(
      restarted
        .run_once(run, successor.id, target.clone(), None, at(14))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let consumed = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(consumed.flow.data.inputs, vec![input]);
    assert_eq!(
      consumed.flow.data.profile_selections.len(),
      2,
      "retain the original wait alongside consumption"
    );
    let selected = consumed
      .flow
      .data
      .profile_selections
      .iter()
      .find(|s| s.candidate().is_some())
      .unwrap();
    assert_eq!(selected.candidate(), Some(&key("specialized")));
    assert_eq!(selected.started_at(), at(11));
    assert_eq!(selected.deadline(), at(1011));
    assert!(consumed.flow.attempts.is_empty());
    assert!(consumed.flow.data.build_intents.is_empty());
    assert_eq!(
      consumed
        .budgets
        .iter()
        .find(|b| b.id == consumed.current.budget_id)
        .unwrap()
        .usage,
      BudgetUsage::default()
    );
    assert_eq!(
      restarted
        .run_once(run, successor.id, target, None, at(15))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let dispatched = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(
      dispatched.flow.data.profile_selections,
      consumed.flow.data.profile_selections
    );
    assert_eq!(dispatched.flow.data.build_intents.len(), 1);
    let receipt = store.apply_factory_run_control(request).await.unwrap();
    assert_eq!(
      receipt.disposition,
      octacity_server_store::MutationDisposition::Replayed
    );
    assert_eq!(receipt.version, accepted.version);
    assert_eq!(dispatched, store.factory_run_snapshot(run).await.unwrap());
  });
}

#[test]
fn the_owner_consumes_only_the_latest_undispatched_profile_override() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = fixture(Some("advanced"));
    let run = admission.run.id();
    let project = admission.work.subject().project_id();
    let choices = testing::factory_profile_selection_capabilities(&admission.flow);
    let binding = admission
      .flow
      .closure()
      .definition(input.definition())
      .unwrap()
      .node(input.node())
      .unwrap()
      .profile_selection()
      .unwrap()
      .clone();
    let (store, old_claim) = start(admission, ownership.clone()).await;
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
      .run_once(run, old_claim.id, target.clone(), Some(input.clone()), at(11))
      .await
      .unwrap();
    for name in ["specialized", "baseline"] {
      let current = store.factory_run_snapshot(run).await.unwrap();
      store
        .apply_factory_run_control(testing::management_mutation_with_request(
          ApplyFactoryRunControl {
            run_id: run,
            expected_version: current.run.version(),
            idempotency_key: IdempotencyKey::new(name).unwrap(),
            requested_at: at(12),
            intent: FactoryRunControlIntent::SelectProfile {
              input_digest: input.digest().unwrap(),
              profile: binding.candidates[&key(name)].profile.reference().clone(),
            },
          },
          name,
        ))
        .await
        .unwrap();
    }
    let accepted = store.factory_run_snapshot(run).await.unwrap();
    let newest = accepted.controls.iter().max_by_key(|c| c.run_version).unwrap().id;
    let claim = FactoryRunClaimRecord::new(
      run,
      ownership.owner().clone(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([73; 32])),
        at(13),
        at(200),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run,
        expected_version: accepted.run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          run,
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([73; 32]),
          key("accepted"),
          at(13),
        ),
      })
      .await
      .unwrap();
    let restarted = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    assert_eq!(
      restarted
        .run_once(run, claim.id, target.clone(), None, at(14))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let after = store.factory_run_snapshot(run).await.unwrap();
    let selected = after
      .flow
      .data
      .profile_selections
      .iter()
      .max_by_key(|s| s.generation())
      .unwrap();
    assert_eq!(selected.candidate(), Some(&key("baseline")));
    assert_eq!(selected.manual_control(), Some(newest));
    assert_eq!(after.flow.data.profile_selections.len(), 2);
    assert_eq!(after.controls.len(), 2);
    assert_eq!(
      restarted.run_once(run, claim.id, target, None, at(15)).await.unwrap(),
      FactoryNodeStep::Advanced
    );
    let dispatched = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(
      dispatched.flow.data.profile_selections,
      after.flow.data.profile_selections
    );
    assert_eq!(dispatched.controls, after.controls);
    assert_eq!(dispatched.flow.data.build_intents.len(), 1);
  });
}

#[test]
fn selected_profile_commits_one_build_intent_with_the_original_input_and_deadline() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = fixture(Some("specialized"));
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
    let runner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(NoExecution),
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    runner
      .run_once(run, claim.id, target.clone(), Some(input.clone()), at(11))
      .await
      .unwrap();
    assert_eq!(
      runner.run_once(run, claim.id, target, None, at(20)).await.unwrap(),
      FactoryNodeStep::Advanced
    );
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(snapshot.flow.attempts.len(), 1);
    assert_eq!(snapshot.flow.data.inputs, vec![input.clone()]);
    assert_eq!(snapshot.flow.data.profile_selections.len(), 1);
    let intent = &snapshot.flow.data.build_intents[0];
    assert_eq!(intent.input(), &input);
    assert_eq!(intent.node().deadline(), at(1011));
    assert_eq!(
      intent.selection_digest(),
      Some(snapshot.flow.data.profile_selections[0].digest().unwrap())
    );
    let builds = Arc::new(crate::factory_node_test_support::Builds::new_at(at(31)));
    builds.lose_response.store(true, std::sync::atomic::Ordering::SeqCst);
    let adapter = FactoryNodeBuildAdapter::new(
      builds.clone(),
      Arc::new(crate::factory_node_test_support::Outputs(None)),
    );
    let schema = snapshot.admitted_flow.data_schema(input.payload().schema()).unwrap();
    assert!(
      adapter
        .observe_or_dispatch(
          intent,
          &snapshot.admitted_flow,
          schema,
          FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
          at(31)
        )
        .await
        .is_err()
    );
    assert_eq!(
      builds.requests.lock().unwrap().len(),
      1,
      "dispatch accepted before its acknowledgement was lost"
    );
    let successor = FactoryRunClaimRecord::new(
      run,
      claim.owner.clone(),
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
        expected_version: snapshot.run.version(),
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
    // No catalog is installed after takeover: committed dispatch must only observe the original operation.
    let restarted = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(adapter),
      crate::factory_node_test_support::clock(),
    );
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    assert_eq!(
      restarted
        .run_once(run, successor.id, target, None, at(150))
        .await
        .unwrap(),
      FactoryNodeStep::Waiting
    );
    let after = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(after.flow.data.build_intents, snapshot.flow.data.build_intents);
    assert_eq!(
      after.flow.data.profile_selections,
      snapshot.flow.data.profile_selections
    );
    assert_eq!(after.flow.data.inputs, snapshot.flow.data.inputs);
    assert_eq!(after.flow.attempts, snapshot.flow.attempts);
    assert_eq!(after.flow.data.build_executions.len(), 1);
    assert_eq!(
      after.flow.data.build_executions[0].build_id(),
      builds.acceptance.build_id
    );
    assert_eq!(
      builds.requests.lock().unwrap().len(),
      1,
      "takeover must observe, never dispatch twice"
    );
    let (retry_attempt, retry_job) = crate::factory_node_test_support::complete_ordinary_retry(&builds);
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let mut document = crate::factory_node_test_support::document(
      &builds,
      &intent.profile().result_output,
      schema.reference(),
      br#"{"count":9}"#.to_vec(),
      intent.profile().result_tool().clone(),
    );
    document.plugin = intent.profile().plugin.clone();
    let outputs = FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: intent.profile().clone(),
        input_digest: input.digest().unwrap(),
        permissions: intent.permissions().clone(),
        budget: intent.budget(),
        deadline: intent.node().deadline(),
      },
      documents: vec![document],
      usage: BudgetUsage {
        attempts: 1,
        output_bytes: 11,
        ..Default::default()
      },
      verified_at: at(151),
      fresh_until: at(1000),
    };
    let completed_owner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(FactoryNodeBuildAdapter::new(
        builds.clone(),
        Arc::new(crate::factory_node_test_support::Outputs(Some(outputs))),
      )),
      crate::factory_node_test_support::clock(),
    );
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    let FactoryNodeStep::Completed(record) = completed_owner
      .run_once(run, successor.id, target, None, at(152))
      .await
      .unwrap()
    else {
      panic!("ordinary retry must complete the original selected operation");
    };
    let producer = &record.build_provenance().unwrap().outputs()[0].producer;
    assert_eq!(producer.attempt_id(), retry_attempt);
    assert_eq!(producer.job_id(), retry_job);
    let done = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(done.flow.data.build_intents, snapshot.flow.data.build_intents);
    assert_eq!(done.flow.data.profile_selections, snapshot.flow.data.profile_selections);
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
  });
}

fn pooled_fixture() -> (PublishedFactoryAdmission, FlowNodeInput, FactoryClaimOwnership) {
  let (mut admission, old_input, ownership) = fixture(Some("specialized"));
  let root = &admission.flow.closure().definitions()[0];
  let mut node = serde_json::to_value(root.nodes()[0].clone().with_phase_pool(key("shared.ready"))).unwrap();
  node["outcomes"].as_array_mut().unwrap().push(
    serde_json::to_value(FlowOutcomeDefinition::new(
      key("provider_accepts"),
      FlowOutcomeKind::Success,
      old_input.payload().schema().clone(),
    ))
    .unwrap(),
  );
  let mut transitions = root.transitions().to_vec();
  transitions.push(FlowTransition::new(
    old_input.node().clone(),
    key("provider_accepts"),
    FlowTransitionTarget::Terminal(root.terminals()[0].key().clone()),
  ));
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes: vec![serde_json::from_value(node).unwrap()],
    transitions,
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  let flow = FlowRun::root(&admission.run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let policy = PhasePoolPolicy {
    phase: key("shared.ready"),
    order: vec![
      PhasePoolOrder::Severity,
      PhasePoolOrder::ProjectPriority,
      PhasePoolOrder::Age,
    ],
    max_wip: 1,
    max_project_wip: 1,
    budget: admission.flow.limits().budget(),
  };
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
  .with_pool_settings(
    [(
      key("shared.ready"),
      FlowPoolSettings {
        policy: policy.clone(),
        project_priority: 0,
        capabilities: Default::default(),
        selection: FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!({"severity":"low","dependencies":[]}),
        })
        .unwrap(),
      },
    )]
    .into(),
  )
  .unwrap();
  let input = FlowNodeInput::new(
    &admission.work,
    &admission.flow,
    admission.flow.root_run(),
    admission.flow.initial_cycle(),
    old_input.node().clone(),
    old_input.payload().clone(),
  )
  .unwrap()
  .with_context(old_input.context().unwrap().clone(), vec![])
  .unwrap();
  (admission, input, ownership)
}

#[test]
fn selected_profile_uses_the_existing_phase_pool_before_allocating_its_build() {
  crate::factory_admission_tests::run_ready(async {
    let (admission, input, ownership) = pooled_fixture();
    let policy = admission.flow.pool_settings()[&key("shared.ready")].policy.clone();
    let run = admission.run.id();
    let project = admission.work.subject().project_id();
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
      inputs.clone(),
      crate::factory_node_test_support::clock(),
    )
    .with_profile_catalog(store.clone());
    assert_eq!(
      owner.reconcile_flow(run, claim.id, at(11)).await.unwrap(),
      FactoryFlowStep::Advanced
    );
    let FactoryFlowStep::Ready(ready) = owner.reconcile_flow(run, claim.id, at(12)).await.unwrap() else {
      panic!("a selected profile must wait in its ordinary configured phase pool");
    };
    let waiting = store.factory_run_snapshot(run).await.unwrap();
    assert!(waiting.flow.attempts.is_empty());
    assert!(waiting.flow.data.build_intents.is_empty());
    assert_eq!(waiting.flow.data.profile_selections.len(), 1);
    let selected = store
      .select_phase_ready(SelectPhasePool {
        policy_digest: policy.digest(),
        request_id: FactoryDigest::from_bytes([3; 32]),
        owner: key("pool.owner"),
        observed_at: at(101),
        expires_at: at(200),
        capabilities: Default::default(),
        limit: 1,
      })
      .await
      .unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].entry, *ready);
    let successor = selected[0].run_claim();
    assert_eq!(
      owner.reconcile_flow(run, successor.id, at(102)).await.unwrap(),
      FactoryFlowStep::Advanced
    );
    let dispatched = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(
      dispatched.flow.data.profile_selections,
      waiting.flow.data.profile_selections
    );
    assert_eq!(dispatched.flow.data.build_intents.len(), 1);
    assert_eq!(inputs.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    let intent = &dispatched.flow.data.build_intents[0];
    assert!(
      FlowNodeRecord::new(
        intent.input(),
        intent.node(),
        &dispatched.admitted_flow.closure().definitions()[0],
        FlowRecordObservation {
          outcome: key("provider_accepts"),
          payload: intent.input().payload().clone(),
          producer: intent.node().execution().clone(),
          observed_at: at(103)
        }
      )
      .is_err(),
      "a selected Build must not choose another declared semantic outcome"
    );
  });
}

#[path = "profile_selection_waits.rs"]
mod waits;

#[path = "profile_selection_recovery.rs"]
mod recovery;
