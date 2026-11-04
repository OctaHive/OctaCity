use super::*;
use crate::testing;

/// Shared atomic live-selection contract; the caller seeds the supplied admission.
pub async fn verify_factory_profile_control<S: FactoryRunStore + FactoryRunControlStore>(
  store: &S,
  admission: &PublishedFactoryAdmission,
  input: &FlowNodeInput,
  ownership: &FactoryClaimOwnership,
) -> (ManagementMutation<ApplyFactoryRunControl>, FactoryRunControlOutcome) {
  let run = admission.run.id();
  let (pending, _) = freeze_profile_selection(store, admission, input, ownership).await;
  let binding = admission
    .flow
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .profile_selection()
    .unwrap();
  let intent = FactoryRunControlIntent::SelectProfile {
    input_digest: input.digest().unwrap(),
    profile: binding.candidates[&key("specialized")].profile.reference().clone(),
  };
  let request = ApplyFactoryRunControl {
    run_id: run,
    expected_version: pending.run.version(),
    idempotency_key: IdempotencyKey::new("manual-profile").unwrap(),
    intent,
    requested_at: time(12),
  };
  let invalid_intents = [
    FactoryRunControlIntent::SelectProfile {
      input_digest: FactoryDigest::from_bytes([231; 32]),
      profile: binding.candidates[&key("specialized")].profile.reference().clone(),
    },
    FactoryRunControlIntent::SelectProfile {
      input_digest: input.digest().unwrap(),
      profile: FactoryRoleProfileRef {
        digest: FactoryDigest::from_bytes([231; 32]),
        ..binding.candidates[&key("specialized")].profile.reference().clone()
      },
    },
  ];
  for intent in invalid_intents {
    assert!(
      store
        .apply_factory_run_control(testing::management_mutation_with_request(
          ApplyFactoryRunControl {
            intent,
            ..request.clone()
          },
          "invalid-profile"
        ))
        .await
        .is_err()
    );
    assert_eq!(pending, store.factory_run_snapshot(run).await.unwrap());
  }
  for (version, at) in [(admission.run.version(), 12), (pending.run.version(), 1012)] {
    assert!(
      store
        .apply_factory_run_control(testing::management_mutation_with_request(
          ApplyFactoryRunControl {
            expected_version: version,
            requested_at: time(at),
            ..request.clone()
          },
          "stale-profile"
        ))
        .await
        .is_err()
    );
  }
  let mutation = testing::management_mutation_with_request(request.clone(), "select-profile");
  let accepted = store.apply_factory_run_control(mutation.clone()).await.unwrap();
  let recorded = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(accepted.version.get(), pending.run.version().get() + 1);
  assert_eq!(
    recorded.flow, pending.flow,
    "acceptance records intent, not owner consumption"
  );
  assert!(
    recorded.current_claim.is_none(),
    "accepted control fences the old owner"
  );
  assert_eq!(recorded.controls.len(), 1);
  assert_eq!(recorded.controls[0].intent, request.intent);
  let audit = store
    .list_factory_run_diagnostics(
      ListFactoryRunDiagnostics::new(run, FactoryRunDiagnosticKind::Audit, None, 100).unwrap(),
    )
    .await
    .unwrap();
  assert!(audit.items.iter().any(|row| matches!(row, FactoryRunDiagnosticRecord::Audit(a) if a.operation.as_str()=="select-factory-profile" && a.actor_kind==mutation.audit().actor().kind)));
  assert!(
    store
      .apply_factory_run_control(testing::management_mutation_with_request(
        ApplyFactoryRunControl {
          expected_version: recorded.run.version(),
          idempotency_key: IdempotencyKey::new("duplicate-choice").unwrap(),
          ..request.clone()
        },
        "duplicate-profile"
      ))
      .await
      .is_err()
  );
  let successor = FactoryRunClaimRecord::new(
    run,
    ownership.owner().clone(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([52; 32])),
      time(13),
      time(200),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: recorded.run.version(),
      record: successor.clone(),
      audit: fact(run, "factory.claimed", 13),
    })
    .await
    .unwrap();
  let current = store.factory_run_snapshot(run).await.unwrap();
  let pending_selection = &pending.flow.data.profile_selections[0];
  let profile = &binding.candidates[&key("specialized")].profile.reference().clone();
  let mut consume = factory_flow_contract_transition(&current, &successor, 14);
  consume.append.flow.data.profile_selections.push(
    pending_selection
      .choose(input, &admission.flow, profile, recorded.controls[0].id, time(14))
      .unwrap(),
  );
  let mut forged = consume.clone();
  forged.append.flow.data.profile_selections = vec![
    pending_selection
      .choose(
        input,
        &admission.flow,
        profile,
        FactoryDigest::from_bytes([233; 32]),
        time(14),
      )
      .unwrap(),
  ];
  assert!(
    store.commit_factory_run_transition(forged).await.is_err(),
    "an owner cannot invent operator authorization"
  );
  assert_eq!(current, store.factory_run_snapshot(run).await.unwrap());
  store.commit_factory_run_transition(consume).await.unwrap();
  let consumed = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(consumed.flow.data.profile_selections.len(), 2);
  let choice = consumed
    .flow
    .data
    .profile_selections
    .iter()
    .find(|s| s.manual_control().is_some())
    .unwrap();
  assert_eq!(choice.candidate(), Some(&key("specialized")));
  assert_eq!(choice.started_at(), time(11));
  assert_eq!(choice.deadline(), time(1011));
  assert_eq!(choice.manual_control(), Some(recorded.controls[0].id));
  assert!(consumed.flow.attempts.is_empty());
  assert!(consumed.flow.data.build_intents.is_empty());
  let mut replay = request;
  replay.requested_at = time(2000);
  let receipt = store
    .apply_factory_run_control(testing::management_mutation_with_request(replay, "lost-response"))
    .await
    .unwrap();
  assert_eq!(receipt.disposition, MutationDisposition::Replayed);
  assert_eq!(receipt.version, accepted.version);
  assert_eq!(consumed, store.factory_run_snapshot(run).await.unwrap());
  (mutation, accepted)
}

/// Retains a selection through the same owner journal in every backend contract.
pub async fn freeze_profile_selection<S: FactoryRunStore>(
  store: &S,
  admission: &PublishedFactoryAdmission,
  input: &FlowNodeInput,
  ownership: &FactoryClaimOwnership,
) -> (FactoryRunSnapshot, FactoryRunClaimRecord) {
  let run = admission.run.id();
  let claim = FactoryRunClaimRecord::new(run, ownership.owner().clone(), ownership.claim());
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: fact(run, "factory.claimed", 10),
    })
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  let mut freeze = factory_flow_contract_transition(&snapshot, &claim, 11);
  freeze.append.flow.data.inputs.push(input.clone());
  freeze
    .append
    .flow
    .data
    .profile_selections
    .push(FlowProfileSelectionRecord::select(input, &admission.flow, time(11)).unwrap());
  store.commit_factory_run_transition(freeze).await.unwrap();
  (store.factory_run_snapshot(run).await.unwrap(), claim)
}

/// Closed waits cannot be revived by control acceptance, even before owner polling.
pub async fn verify_factory_profile_wait_controls<S: FactoryRunStore + FactoryRunControlStore>(
  store: &S,
  admission: &PublishedFactoryAdmission,
  input: &FlowNodeInput,
  ownership: &FactoryClaimOwnership,
) {
  let (pending, claim) = freeze_profile_selection(store, admission, input, ownership).await;
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
  let choice = |snapshot: &FactoryRunSnapshot, name: &str, at| {
    testing::management_mutation_with_request(
      ApplyFactoryRunControl {
        run_id: run,
        expected_version: snapshot.run.version(),
        idempotency_key: IdempotencyKey::new(name).unwrap(),
        requested_at: time(at),
        intent: FactoryRunControlIntent::SelectProfile {
          input_digest: input.digest().unwrap(),
          profile: profile.clone(),
        },
      },
      name,
    )
  };
  assert!(
    store
      .apply_factory_run_control(choice(&pending, "choice-at-deadline", 1011))
      .await
      .is_err(),
    "the exact deadline is already outside the execution window"
  );
  assert_eq!(pending, store.factory_run_snapshot(run).await.unwrap());
  let mut cost = factory_flow_contract_transition(&pending, &claim, 12);
  cost.budget = FactoryBudgetRecord::new(
    run,
    cost.next_run.version(),
    BudgetUsage {
      tokens: admission.flow.limits().budget().max_tokens(),
      ..Default::default()
    },
    time(12),
  );
  cost.current.budget_id = cost.budget.id;
  store.commit_factory_run_transition(cost).await.unwrap();
  let exhausted = store.factory_run_snapshot(run).await.unwrap();
  assert!(
    store
      .apply_factory_run_control(choice(&exhausted, "choice-without-budget", 13))
      .await
      .is_err(),
    "a choice cannot revive an exhausted hard budget"
  );
  assert_eq!(exhausted, store.factory_run_snapshot(run).await.unwrap());
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn profile_wait_controls_reject_expiry_and_budget_exhaustion() {
    crate::test_support::run_ready(
      async {
        let (admission, input, ownership) = super::super::factory_profile_selection_contract_fixture(None);
        let store = testing::InMemoryFactoryConfigurationStore::new();
        let audit = testing::management_mutation_with_request((), "admit-bounded-profile")
          .audit()
          .clone();
        store.seed_factory_run(admission.clone(), digest(1), &audit).unwrap();
        verify_factory_profile_wait_controls(&store, &admission, &input, &ownership).await;
      },
      "bounded profile controls are ready",
    );
  }
  #[test]
  fn live_profile_control_uses_the_shared_atomic_run_contract() {
    crate::test_support::run_ready(
      async {
        let (admission, input, ownership) = super::super::factory_profile_selection_contract_fixture(None);
        let store = testing::InMemoryFactoryConfigurationStore::new();
        let audit = testing::management_mutation_with_request((), "admit-profile")
          .audit()
          .clone();
        store
          .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
          .unwrap();
        verify_factory_profile_control(&store, &admission, &input, &ownership).await;
      },
      "live profile control is ready",
    );
  }
}
