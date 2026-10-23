use super::*;
use crate::testing;

/// Uses a deterministic automatic policy over the original frozen count observation.
pub fn factory_profile_supersession_contract_fixture()
-> (PublishedFactoryAdmission, FlowNodeInput, FactoryClaimOwnership) {
  super::factory_profile_selection_policy_contract_fixture(serde_json::from_value(serde_json::json!({
    "mode":"rules", "rules":[{"when":{"operation":"exists","operand":{"source":"path","path":"/count"}},"candidate":"advanced"}],
    "otherwise":"baseline"
  })).unwrap())
}

/// Qualifies append-only replacement of an automatic choice and then a manual choice.
pub async fn verify_factory_profile_supersession<S: FactoryRunStore + FactoryRunControlStore>(
  store: &S,
  admission: &PublishedFactoryAdmission,
  input: &FlowNodeInput,
  ownership: &FactoryClaimOwnership,
) {
  let run = admission.run.id();
  let (initial, _) = super::profile_control::freeze_profile_selection(store, admission, input, ownership).await;
  assert_eq!(
    initial.flow.data.profile_selections[0].candidate(),
    Some(&key("advanced"))
  );
  let binding = admission
    .flow
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .profile_selection()
    .unwrap();
  let mut previous = initial;
  let mut receipts = vec![];
  for (index, name) in ["specialized", "baseline"].into_iter().enumerate() {
    let tick = 12 + index as i64 * 4;
    let request = testing::management_mutation_with_request(
      ApplyFactoryRunControl {
        run_id: run,
        expected_version: previous.run.version(),
        requested_at: time(tick),
        idempotency_key: IdempotencyKey::new(format!("override-{index}")).unwrap(),
        intent: FactoryRunControlIntent::SelectProfile {
          input_digest: input.digest().unwrap(),
          profile: binding.candidates[&key(name)].profile.reference().clone(),
        },
      },
      format!("operator-{index}"),
    );
    let accepted = store.apply_factory_run_control(request.clone()).await.unwrap();
    let recorded = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(
      recorded.flow, previous.flow,
      "intent cannot erase prior selection facts"
    );
    let control = recorded.controls.iter().max_by_key(|c| c.run_version).unwrap();
    let claim = FactoryRunClaimRecord::new(
      run,
      ownership.owner().clone(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([60 + index as u8; 32])),
        time(tick + 1),
        time(200),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run,
        expected_version: recorded.run.version(),
        record: claim.clone(),
        audit: fact(run, "factory.claimed", tick + 1),
      })
      .await
      .unwrap();
    let current = store.factory_run_snapshot(run).await.unwrap();
    let prior = previous
      .flow
      .data
      .profile_selections
      .iter()
      .max_by_key(|s| s.recorded_at())
      .unwrap();
    let choice = prior
      .choose(
        input,
        &admission.flow,
        binding.candidates[&key(name)].profile.reference(),
        control.id,
        time(tick + 2),
      )
      .unwrap();
    let mut transition = factory_flow_contract_transition(&current, &claim, tick + 2);
    transition.append.flow.data.profile_selections.push(choice.clone());
    store.commit_factory_run_transition(transition).await.unwrap();
    let after = store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(after.flow.data.profile_selections.len(), index + 2);
    for retained in &previous.flow.data.profile_selections {
      assert!(after.flow.data.profile_selections.contains(retained));
    }
    assert_eq!(choice.candidate(), Some(&key(name)));
    assert_eq!(choice.started_at(), time(11));
    assert_eq!(choice.deadline(), time(1011));
    assert!(after.flow.attempts.is_empty());
    assert!(after.flow.data.build_intents.is_empty());
    receipts.push((request, accepted));
    previous = after;
  }
  for (request, receipt) in receipts {
    let replay = store.apply_factory_run_control(request).await.unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.version, receipt.version);
    assert_eq!(previous, store.factory_run_snapshot(run).await.unwrap());
  }
}

/// Two operators share one precondition; late work cannot publish with renewed ownership.
pub async fn verify_factory_profile_selection_fencing<S: FactoryRunStore + FactoryRunControlStore>(
  store: &S,
  admission: &PublishedFactoryAdmission,
  input: &FlowNodeInput,
  ownership: &FactoryClaimOwnership,
) {
  let run = admission.run.id();
  let (before, old_claim) = super::profile_control::freeze_profile_selection(store, admission, input, ownership).await;
  let binding = admission
    .flow
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .profile_selection()
    .unwrap();
  let command = |name: &str, version, id: &str| {
    testing::management_mutation_with_request(
      ApplyFactoryRunControl {
        run_id: run,
        expected_version: version,
        requested_at: time(12),
        idempotency_key: IdempotencyKey::new(id).unwrap(),
        intent: FactoryRunControlIntent::SelectProfile {
          input_digest: input.digest().unwrap(),
          profile: binding.candidates[&key(name)].profile.reference().clone(),
        },
      },
      id,
    )
  };
  let one = command("specialized", before.run.version(), "operator-one");
  let two = command("baseline", before.run.version(), "operator-two");
  let (left, right) = race(
    store.apply_factory_run_control(one.clone()),
    store.apply_factory_run_control(two.clone()),
  )
  .await;
  assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
  let (winner, receipt) = if let Ok(receipt) = left {
    (one, receipt)
  } else {
    (two, right.unwrap())
  };
  let accepted = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(accepted.controls.len(), 1);
  assert_eq!(accepted.flow, before.flow);
  let current = command("advanced", accepted.run.version(), "operator-replaces-pending");
  store.apply_factory_run_control(current).await.unwrap();
  let accepted = store.factory_run_snapshot(run).await.unwrap();
  let stale_owner = factory_flow_contract_transition(&before, &old_claim, 13);
  assert!(store.commit_factory_run_transition(stale_owner).await.is_err());
  let claim = FactoryRunClaimRecord::new(
    run,
    ownership.owner().clone(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([70; 32])),
      time(13),
      time(200),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: accepted.run.version(),
      record: claim.clone(),
      audit: fact(run, "factory.claimed", 13),
    })
    .await
    .unwrap();
  let locked = store.factory_run_snapshot(run).await.unwrap();
  let oldest = accepted.controls.iter().min_by_key(|c| c.run_version).unwrap();
  let latest = accepted.controls.iter().max_by_key(|c| c.run_version).unwrap();
  let FactoryRunControlIntent::SelectProfile { profile, .. } = &oldest.intent else {
    panic!("expected a profile control")
  };
  let mut late = factory_flow_contract_transition(&locked, &claim, 14);
  late.append.flow.data.profile_selections.push(
    before.flow.data.profile_selections[0]
      .choose(input, &admission.flow, profile, oldest.id, time(14))
      .unwrap(),
  );
  assert!(
    store.commit_factory_run_transition(late).await.is_err(),
    "renewing a worker fence cannot authorize a superseded result"
  );
  assert_eq!(locked, store.factory_run_snapshot(run).await.unwrap());
  let mut automatic = factory_flow_contract_transition(&locked, &claim, 14);
  automatic
    .append
    .flow
    .data
    .profile_selections
    .push(FlowProfileSelectionRecord::select(input, &admission.flow, time(14)).unwrap());
  assert!(
    store.commit_factory_run_transition(automatic).await.is_err(),
    "a late automatic fact cannot replace an accepted manual intent"
  );
  let mut consume = factory_flow_contract_transition(&locked, &claim, 14);
  consume.append.flow.data.profile_selections.push(
    before.flow.data.profile_selections[0]
      .choose(
        input,
        &admission.flow,
        binding.candidates[&key("advanced")].profile.reference(),
        latest.id,
        time(14),
      )
      .unwrap(),
  );
  store.commit_factory_run_transition(consume).await.unwrap();
  let after = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(after.controls.len(), 2);
  assert_eq!(after.flow.data.profile_selections.len(), 2);
  assert_eq!(
    after
      .flow
      .data
      .profile_selections
      .iter()
      .max_by_key(|s| s.generation())
      .unwrap()
      .manual_control(),
    Some(latest.id)
  );
  let replay = store.apply_factory_run_control(winner).await.unwrap();
  assert_eq!(replay.version, receipt.version);
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(after, store.factory_run_snapshot(run).await.unwrap());
}

async fn race<A: std::future::Future, B: std::future::Future>(a: A, b: B) -> (A::Output, B::Output) {
  use std::{pin::pin, task::Poll};
  let mut a = pin!(a);
  let mut b = pin!(b);
  let mut first = None;
  let mut second = None;
  std::future::poll_fn(|cx| {
    if first.is_none()
      && let Poll::Ready(value) = a.as_mut().poll(cx)
    {
      first = Some(value);
    }
    if second.is_none()
      && let Poll::Ready(value) = b.as_mut().poll(cx)
    {
      second = Some(value);
    }
    if first.is_some() && second.is_some() {
      Poll::Ready((first.take().unwrap(), second.take().unwrap()))
    } else {
      Poll::Pending
    }
  })
  .await
}

fn selected_dispatch(
  snapshot: &FactoryRunSnapshot,
  input: &FlowNodeInput,
  claim: &FactoryRunClaimRecord,
  at: i64,
) -> CommitFactoryRunTransition {
  let selection = snapshot
    .flow
    .data
    .profile_selections
    .iter()
    .max_by_key(|s| s.generation())
    .unwrap();
  let definition = snapshot.admitted_flow.closure().definition(input.definition()).unwrap();
  let node = definition.node(input.node()).unwrap();
  let candidate = &node.profile_selection().unwrap().candidates[selection.candidate().unwrap()];
  let attempt = NodeAttempt::new(
    snapshot.admitted_flow.root_run(),
    snapshot.admitted_flow.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: input.node().clone(),
      node_kind: node.kind(),
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: node.budget(),
      started_at: time(at),
      deadline: selection.deadline(),
      execution: NodeExecutionIdentity::External(candidate.build.tool.clone()),
      ownership: FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
    },
  )
  .unwrap();
  let intent = FlowBuildIntent::selected(
    input.clone(),
    &snapshot.admitted_flow,
    attempt.clone(),
    BudgetUsage::default(),
    i64::from(snapshot.work.priority().get()),
    selection,
  )
  .unwrap();
  let mut dispatch = factory_flow_contract_transition(snapshot, claim, at);
  dispatch.append.flow.attempts.push(attempt);
  dispatch.append.flow.data.build_intents.push(intent);
  dispatch.budget = FactoryBudgetRecord::new(
    snapshot.run.id(),
    dispatch.next_run.version(),
    BudgetUsage {
      attempts: 1,
      ..Default::default()
    },
    time(at),
  );
  dispatch.current.budget_id = dispatch.budget.id;
  dispatch
}

/// Manual acceptance and ordinary intent commitment compete under one Run lock.
pub async fn verify_factory_profile_dispatch_race<S: FactoryRunStore + FactoryRunControlStore>(
  store: &S,
  admission: &PublishedFactoryAdmission,
  input: &FlowNodeInput,
  ownership: &FactoryClaimOwnership,
  dispatch_first: bool,
) {
  let run = admission.run.id();
  let (before, claim) = super::profile_control::freeze_profile_selection(store, admission, input, ownership).await;
  let binding = admission
    .flow
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .profile_selection()
    .unwrap();
  let command = testing::management_mutation_with_request(
    ApplyFactoryRunControl {
      run_id: run,
      expected_version: before.run.version(),
      requested_at: time(12),
      idempotency_key: IdempotencyKey::new("override-versus-dispatch").unwrap(),
      intent: FactoryRunControlIntent::SelectProfile {
        input_digest: input.digest().unwrap(),
        profile: binding.candidates[&key("specialized")].profile.reference().clone(),
      },
    },
    "override-versus-dispatch",
  );
  let dispatch = selected_dispatch(&before, input, &claim, 12);
  let operation = dispatch.append.flow.data.build_intents[0].operation_id().unwrap();
  let (manual, committed) = if dispatch_first {
    let (committed, manual) = race(
      store.commit_factory_run_transition(dispatch),
      store.apply_factory_run_control(command.clone()),
    )
    .await;
    (manual, committed)
  } else {
    race(
      store.apply_factory_run_control(command.clone()),
      store.commit_factory_run_transition(dispatch),
    )
    .await
  };
  assert_eq!(
    usize::from(manual.is_ok()) + usize::from(committed.is_ok()),
    1,
    "only one authority may win"
  );
  let after = store.factory_run_snapshot(run).await.unwrap();
  if committed.is_ok() {
    assert!(after.controls.is_empty());
    assert_eq!(after.flow.data.build_intents[0].operation_id().unwrap(), operation);
    let (mut late, context) = command.into_parts();
    late.expected_version = after.run.version();
    late.idempotency_key = IdempotencyKey::new("after-unknown-dispatch-response").unwrap();
    assert!(
      store
        .apply_factory_run_control(ManagementMutation::new(late, context))
        .await
        .is_err()
    );
  } else {
    assert!(after.flow.data.build_intents.is_empty());
    assert!(after.flow.attempts.is_empty());
    let fresh = FactoryRunClaimRecord::new(
      run,
      ownership.owner().clone(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([74; 32])),
        time(13),
        time(200),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run,
        expected_version: after.run.version(),
        record: fresh.clone(),
        audit: fact(run, "factory.claimed", 13),
      })
      .await
      .unwrap();
    let current = store.factory_run_snapshot(run).await.unwrap();
    assert!(
      store
        .commit_factory_run_transition(selected_dispatch(&current, input, &fresh, 14))
        .await
        .is_err(),
      "a renewed owner cannot dispatch the superseded automatic choice"
    );
  }
  let retained = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(
    retained.flow.data.profile_selections,
    before.flow.data.profile_selections
  );
}

/// An already retained raw selector failure supports its one configured automatic recovery.
pub fn factory_profile_recovery_race_contract_fixture()
-> (PublishedFactoryAdmission, FlowNodeInput, FactoryClaimOwnership) {
  super::factory_profile_selection_failure_contract_fixture(serde_json::from_value(serde_json::json!({"mode":"rules","rules":[{
    "when":{"operation":"compare","left":{"source":"path","path":"/complexity"},"comparison":"greater","right":{"source":"literal","value":2}},"candidate":"advanced"
  }],"otherwise":"baseline"})).unwrap(), FlowProfileSelectionFailure::ReserveProfile {candidate:key("specialized")})
}

/// A valid automatic publication and an operator choice race on the same pending generation.
pub async fn verify_factory_profile_automatic_race<S: FactoryRunStore + FactoryRunControlStore>(
  store: &S,
  admission: &PublishedFactoryAdmission,
  input: &FlowNodeInput,
  ownership: &FactoryClaimOwnership,
  automatic_first: bool,
) {
  let run = admission.run.id();
  let (before, claim) = super::profile_control::freeze_profile_selection(store, admission, input, ownership).await;
  let pending = &before.flow.data.profile_selections[0];
  assert!(pending.needs_recovery());
  let choices = super::factory_profile_selection_capabilities(&admission.flow);
  let proposal = pending
    .recover_pending(input, &admission.flow, Some(&choices), time(12))
    .unwrap();
  assert_eq!(proposal.candidate(), Some(&key("specialized")));
  let mut automatic = factory_flow_contract_transition(&before, &claim, 12);
  automatic.append.flow.data.profile_selections.push(proposal.clone());
  let profile = admission
    .flow
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .profile_selection()
    .unwrap()
    .candidates[&key("baseline")]
    .profile
    .reference()
    .clone();
  let manual = testing::management_mutation_with_request(
    ApplyFactoryRunControl {
      run_id: run,
      expected_version: before.run.version(),
      idempotency_key: IdempotencyKey::new("manual-versus-automatic").unwrap(),
      requested_at: time(12),
      intent: FactoryRunControlIntent::SelectProfile {
        input_digest: input.digest().unwrap(),
        profile: profile.clone(),
      },
    },
    "manual-versus-automatic",
  );
  let (operator, worker) = if automatic_first {
    let (worker, operator) = race(
      store.commit_factory_run_transition(automatic),
      store.apply_factory_run_control(manual.clone()),
    )
    .await;
    (operator, worker)
  } else {
    race(
      store.apply_factory_run_control(manual.clone()),
      store.commit_factory_run_transition(automatic),
    )
    .await
  };
  assert_eq!(usize::from(operator.is_ok()) + usize::from(worker.is_ok()), 1);
  let after = store.factory_run_snapshot(run).await.unwrap();
  assert!(after.flow.data.profile_selections.contains(pending));
  if worker.is_ok() {
    assert!(after.controls.is_empty());
    assert_eq!(
      after
        .flow
        .data
        .current_profile_selection(input.digest().unwrap())
        .unwrap(),
      &proposal
    );
    let (mut reviewed, context) = manual.into_parts();
    reviewed.expected_version = after.run.version();
    reviewed.requested_at = time(13);
    store
      .apply_factory_run_control(ManagementMutation::new(reviewed, context))
      .await
      .unwrap();
  }
  let accepted = store.factory_run_snapshot(run).await.unwrap();
  let new_claim = FactoryRunClaimRecord::new(
    run,
    ownership.owner().clone(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([76; 32])),
      time(14),
      time(200),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: accepted.run.version(),
      record: new_claim.clone(),
      audit: fact(run, "factory.claimed", 14),
    })
    .await
    .unwrap();
  let current = store.factory_run_snapshot(run).await.unwrap();
  let mut late = factory_flow_contract_transition(&current, &new_claim, 15);
  late.append.flow.data.profile_selections.push(
    pending
      .recover_pending(input, &admission.flow, Some(&choices), time(15))
      .unwrap(),
  );
  assert!(
    store.commit_factory_run_transition(late).await.is_err(),
    "a late automatic result cannot overtake an accepted manual intent"
  );
  let control = current.controls.iter().max_by_key(|c| c.run_version).unwrap();
  let previous = current
    .flow
    .data
    .current_profile_selection(input.digest().unwrap())
    .unwrap();
  let mut chosen = factory_flow_contract_transition(&current, &new_claim, 15);
  chosen.append.flow.data.profile_selections.push(
    previous
      .choose(input, &admission.flow, &profile, control.id, time(15))
      .unwrap(),
  );
  store.commit_factory_run_transition(chosen).await.unwrap();
  let retained = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(
    retained
      .flow
      .data
      .current_profile_selection(input.digest().unwrap())
      .unwrap()
      .candidate(),
    Some(&key("baseline"))
  );
  assert_eq!(
    retained.flow.data.profile_selections.len(),
    if worker.is_ok() { 3 } else { 2 }
  );
  assert!(retained.flow.data.build_intents.is_empty());
  assert!(retained.flow.attempts.is_empty());
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn profile_supersession_preserves_automatic_and_manual_history() {
    crate::test_support::run_ready(
      async {
        let (admission, input, ownership) = factory_profile_supersession_contract_fixture();
        let store = testing::InMemoryFactoryConfigurationStore::new();
        let audit = testing::management_mutation_with_request((), "supersession-admission")
          .audit()
          .clone();
        store
          .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
          .unwrap();
        verify_factory_profile_supersession(&store, &admission, &input, &ownership).await;
      },
      "profile supersession is ready",
    );
  }

  #[test]
  fn concurrent_profile_controls_and_late_results_share_one_fence() {
    crate::test_support::run_ready(
      async {
        let (admission, input, ownership) = factory_profile_supersession_contract_fixture();
        let store = testing::InMemoryFactoryConfigurationStore::new();
        let audit = testing::management_mutation_with_request((), "fenced-admission")
          .audit()
          .clone();
        store
          .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
          .unwrap();
        verify_factory_profile_selection_fencing(&store, &admission, &input, &ownership).await;
      },
      "profile fencing is ready",
    );
  }

  #[test]
  fn manual_override_and_build_commitment_have_one_winner() {
    crate::test_support::run_ready(
      async {
        for first in [false, true] {
          let (admission, input, ownership) = factory_profile_supersession_contract_fixture();
          let store = testing::InMemoryFactoryConfigurationStore::new();
          let audit = testing::management_mutation_with_request((), "dispatch-admission")
            .audit()
            .clone();
          store
            .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
            .unwrap();
          verify_factory_profile_dispatch_race(&store, &admission, &input, &ownership, first).await;
        }
      },
      "profile dispatch race is ready",
    );
  }

  #[test]
  fn manual_and_automatic_profile_publications_have_one_winner() {
    crate::test_support::run_ready(
      async {
        for first in [false, true] {
          let (admission, input, ownership) = factory_profile_recovery_race_contract_fixture();
          let store = testing::InMemoryFactoryConfigurationStore::new();
          let audit = testing::management_mutation_with_request((), "automatic-race-admission")
            .audit()
            .clone();
          store
            .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
            .unwrap();
          verify_factory_profile_automatic_race(&store, &admission, &input, &ownership, first).await;
        }
      },
      "automatic profile race is ready",
    );
  }
}
