use octacity_server_domain::Timestamp;

use super::*;

fn timestamp(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).expect("fixture timestamp is valid")
}

fn fence(byte: u8) -> FactoryClaimFence {
  FactoryClaimFence::new(FactoryDigest::from_bytes([byte; 32]))
}

fn budget() -> BudgetLimit {
  BudgetLimit::new(5, 100, 100, 100, 100).expect("fixture budget is valid")
}

fn guard_with(
  presented_fence: FactoryClaimFence,
  observed_at: Timestamp,
  usage: BudgetUsage,
  wip_usage: FactoryWipUsage,
) -> FactoryDecisionGuard {
  FactoryDecisionGuard::new(
    FactoryClaim::new(fence(1), timestamp(100), timestamp(200)).expect("fixture claim is valid"),
    presented_fence,
    observed_at,
    FactoryDecisionResources::new(
      budget(),
      usage,
      FactoryWipLimits::new(3, 3).expect("fixture WIP limits are valid"),
      wip_usage,
    ),
    FactoryReworkStatus::new(0, 1, DecisionOutcome::Escalate).expect("fixture rework status is valid"),
  )
}

fn guard() -> FactoryDecisionGuard {
  guard_with(
    fence(1),
    timestamp(150),
    BudgetUsage::default(),
    FactoryWipUsage::new(1, 0),
  )
}

fn snapshot(
  state: FactoryRunState,
  progress: FactoryLifecycleProgress,
  signal: DecisionSignalProgress,
) -> FactoryLifecycleSnapshot {
  FactoryLifecycleSnapshot::new(state, progress, signal, guard(), false)
}

fn snapshot_with_rework(
  state: FactoryRunState,
  progress: FactoryLifecycleProgress,
  completed_cycles: u16,
  max_cycles: u16,
  exhausted_outcome: DecisionOutcome,
) -> FactoryLifecycleSnapshot {
  let rework =
    FactoryReworkStatus::new(completed_cycles, max_cycles, exhausted_outcome).expect("fixture rework status is valid");
  let guard = FactoryDecisionGuard::new(
    FactoryClaim::new(fence(1), timestamp(100), timestamp(200)).expect("fixture claim is valid"),
    fence(1),
    timestamp(150),
    FactoryDecisionResources::new(
      budget(),
      BudgetUsage::default(),
      FactoryWipLimits::new(3, 3).expect("fixture WIP limits are valid"),
      FactoryWipUsage::new(1, 0),
    ),
    rework,
  );
  FactoryLifecycleSnapshot::new(state, progress, DecisionSignalProgress::Disabled, guard, false)
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key is valid")
}

#[test]
fn admitted_run_starts_the_implementation_stage() {
  assert_eq!(
    decide_next_action(&snapshot(
      FactoryRunState::Admitted,
      FactoryLifecycleProgress::Admitted,
      DecisionSignalProgress::Disabled,
    )),
    Ok(FactoryNextAction::CreateStageAttempt(
      FactoryStageTarget::Implementation,
    ))
  );
}

#[test]
fn stage_progression_is_program_owned_and_typed() {
  let cases = [
    (
      FactoryRunState::Implementing,
      FactoryStageTarget::Implementation,
      FactoryStageProgress::AttemptCreated,
      FactoryNextAction::CreateBuild(FactoryStageTarget::Implementation),
    ),
    (
      FactoryRunState::Implementing,
      FactoryStageTarget::Implementation,
      FactoryStageProgress::BuildSucceeded,
      FactoryNextAction::CaptureCandidate,
    ),
    (
      FactoryRunState::Implementing,
      FactoryStageTarget::Implementation,
      FactoryStageProgress::CandidateCaptured,
      FactoryNextAction::CreateStageAttempt(FactoryStageTarget::Validation),
    ),
    (
      FactoryRunState::Validating,
      FactoryStageTarget::Validation,
      FactoryStageProgress::BuildSucceeded,
      FactoryNextAction::ConstructEvidence,
    ),
    (
      FactoryRunState::Validating,
      FactoryStageTarget::Validation,
      FactoryStageProgress::EvidenceConstructed,
      FactoryNextAction::PlanEvaluations,
    ),
    (
      FactoryRunState::Reworking,
      FactoryStageTarget::Rework,
      FactoryStageProgress::RetryableFailure,
      FactoryNextAction::Wait(FactoryWaitReason::RetryApproval(FactoryStageTarget::Rework)),
    ),
    (
      FactoryRunState::Reworking,
      FactoryStageTarget::Rework,
      FactoryStageProgress::RetryRequested,
      FactoryNextAction::RetryStage(FactoryStageTarget::Rework),
    ),
  ];

  for (state, target, progress, expected) in cases {
    assert_eq!(
      decide_next_action(&snapshot(
        state,
        FactoryLifecycleProgress::Stage { target, progress },
        DecisionSignalProgress::Disabled,
      )),
      Ok(expected)
    );
  }
}

#[test]
fn invalid_state_progress_pairs_and_stage_progress_are_rejected() {
  for subject in [
    snapshot(
      FactoryRunState::Validating,
      FactoryLifecycleProgress::Admitted,
      DecisionSignalProgress::Disabled,
    ),
    snapshot(
      FactoryRunState::Validating,
      FactoryLifecycleProgress::Stage {
        target: FactoryStageTarget::Validation,
        progress: FactoryStageProgress::CandidateCaptured,
      },
      DecisionSignalProgress::Disabled,
    ),
  ] {
    assert_eq!(
      decide_next_action(&subject),
      Err(FactoryError::InvalidLifecycle {
        state: FactoryRunState::Validating,
      })
    );
  }
}

#[test]
fn stale_or_expired_claims_never_produce_an_action() {
  let stale = FactoryLifecycleSnapshot::new(
    FactoryRunState::Admitted,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::Disabled,
    guard_with(
      fence(2),
      timestamp(150),
      BudgetUsage::default(),
      FactoryWipUsage::new(1, 0),
    ),
    false,
  );
  let expired = FactoryLifecycleSnapshot::new(
    FactoryRunState::Admitted,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::Disabled,
    guard_with(
      fence(1),
      timestamp(200),
      BudgetUsage::default(),
      FactoryWipUsage::new(1, 0),
    ),
    false,
  );

  assert_eq!(decide_next_action(&stale), Err(FactoryError::StaleClaim));
  assert_eq!(decide_next_action(&expired), Err(FactoryError::ExpiredClaim));
}

#[test]
fn every_exhausted_hard_budget_escalates_before_dispatch() {
  let cases = [
    (
      BudgetResource::Attempts,
      BudgetUsage {
        attempts: 5,
        ..BudgetUsage::default()
      },
    ),
    (
      BudgetResource::ElapsedTime,
      BudgetUsage {
        elapsed_millis: 100,
        ..BudgetUsage::default()
      },
    ),
    (
      BudgetResource::Tokens,
      BudgetUsage {
        tokens: 100,
        ..BudgetUsage::default()
      },
    ),
    (
      BudgetResource::Cost,
      BudgetUsage {
        cost_micro_units: 100,
        ..BudgetUsage::default()
      },
    ),
    (
      BudgetResource::OutputBytes,
      BudgetUsage {
        output_bytes: 100,
        ..BudgetUsage::default()
      },
    ),
  ];

  for (resource, usage) in cases {
    let subject = FactoryLifecycleSnapshot::new(
      FactoryRunState::Admitted,
      FactoryLifecycleProgress::Admitted,
      DecisionSignalProgress::Disabled,
      guard_with(fence(1), timestamp(150), usage, FactoryWipUsage::new(1, 0)),
      false,
    );
    assert_eq!(
      decide_next_action(&subject),
      Ok(FactoryNextAction::Escalate(FactoryEscalationReason::BudgetExhausted(
        resource
      ),))
    );
  }
}

#[test]
fn stage_wip_exhaustion_waits_without_dispatching() {
  let subject = FactoryLifecycleSnapshot::new(
    FactoryRunState::Admitted,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::Disabled,
    guard_with(
      fence(1),
      timestamp(150),
      BudgetUsage::default(),
      FactoryWipUsage::new(1, 3),
    ),
    false,
  );

  assert_eq!(
    decide_next_action(&subject),
    Ok(FactoryNextAction::Wait(FactoryWaitReason::StageCapacity))
  );
}

#[test]
fn invalid_wip_usage_is_rejected_and_cannot_dispatch() {
  let subject = FactoryLifecycleSnapshot::new(
    FactoryRunState::Admitted,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::Disabled,
    guard_with(
      fence(1),
      timestamp(150),
      BudgetUsage::default(),
      FactoryWipUsage::new(4, 0),
    ),
    false,
  );

  assert_eq!(decide_next_action(&subject), Err(FactoryError::WipExceeded));
}

#[test]
fn terminal_budget_exhaustion_is_not_hidden_by_temporary_wip_pressure() {
  let subject = FactoryLifecycleSnapshot::new(
    FactoryRunState::Admitted,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::Disabled,
    guard_with(
      fence(1),
      timestamp(150),
      BudgetUsage {
        attempts: 5,
        ..BudgetUsage::default()
      },
      FactoryWipUsage::new(1, 3),
    ),
    false,
  );

  assert_eq!(
    decide_next_action(&subject),
    Ok(FactoryNextAction::Escalate(FactoryEscalationReason::BudgetExhausted(
      BudgetResource::Attempts,
    )))
  );
}

#[test]
fn decision_signal_is_a_gate_and_never_selects_a_lifecycle_transition() {
  let request = snapshot(
    FactoryRunState::Admitted,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::ReadyToRequest(DecisionSignalPurpose::Routing),
  );
  let consume = snapshot(
    FactoryRunState::Admitted,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::ReadyToConsume,
  );
  let consumed = snapshot(
    FactoryRunState::Admitted,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::Consumed,
  );

  assert_eq!(
    decide_next_action(&request),
    Ok(FactoryNextAction::RequestDecisionSignal(DecisionSignalPurpose::Routing,))
  );
  assert_eq!(
    decide_next_action(&consume),
    Ok(FactoryNextAction::ConsumeDecisionSignal)
  );
  assert_eq!(
    decide_next_action(&consumed),
    Ok(FactoryNextAction::CreateStageAttempt(
      FactoryStageTarget::Implementation,
    ))
  );
}

#[test]
fn cancellation_preempts_signal_and_stage_dispatch() {
  let subject = FactoryLifecycleSnapshot::new(
    FactoryRunState::Implementing,
    FactoryLifecycleProgress::Stage {
      target: FactoryStageTarget::Implementation,
      progress: FactoryStageProgress::Ready,
    },
    DecisionSignalProgress::ReadyToRequest(DecisionSignalPurpose::ToolRisk),
    guard(),
    true,
  );

  assert_eq!(decide_next_action(&subject), Ok(FactoryNextAction::Cancel));
}

#[test]
fn evaluation_fans_out_in_canonical_order_and_waits_for_the_join() {
  let branches = EvaluationProgress::try_new(
    vec![
      EvaluationBranch::new(key("security"), true, EvaluationBranchState::Pending),
      EvaluationBranch::new(key("architecture"), true, EvaluationBranchState::Pending),
    ],
    2,
  )
  .expect("fixture evaluation is valid");
  let first = snapshot(
    FactoryRunState::Evaluating,
    FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(branches)),
    DecisionSignalProgress::Disabled,
  );
  assert_eq!(
    decide_next_action(&first),
    Ok(FactoryNextAction::CreateStageAttempt(FactoryStageTarget::Evaluation(
      key("architecture")
    ),))
  );

  let active = EvaluationProgress::try_new(
    vec![
      EvaluationBranch::new(key("architecture"), true, EvaluationBranchState::BuildActive),
      EvaluationBranch::new(key("security"), true, EvaluationBranchState::Completed),
    ],
    2,
  )
  .expect("fixture evaluation is valid");
  let join = snapshot(
    FactoryRunState::Evaluating,
    FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(active)),
    DecisionSignalProgress::Disabled,
  );
  assert_eq!(
    decide_next_action(&join),
    Ok(FactoryNextAction::Wait(FactoryWaitReason::EvaluationJoin))
  );
}

#[test]
fn evaluation_collects_pending_typed_results_before_waiting_on_active_branches() {
  let progress = EvaluationProgress::try_new(
    vec![
      EvaluationBranch::new(key("architecture"), true, EvaluationBranchState::ResultPending),
      EvaluationBranch::new(key("security"), true, EvaluationBranchState::BuildActive),
    ],
    2,
  )
  .expect("fixture evaluation is valid");
  let restored: EvaluationProgress =
    serde_json::from_slice(&serde_json::to_vec(&progress).expect("serialize progress")).expect("restore progress");

  assert_eq!(
    decide_next_action(&snapshot(
      FactoryRunState::Evaluating,
      FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(restored)),
      DecisionSignalProgress::Disabled,
    )),
    Ok(FactoryNextAction::CollectEvaluationResult(key("architecture")))
  );
}

#[test]
fn legacy_succeeded_branch_state_restores_as_completed() {
  let restored: EvaluationBranchState = serde_json::from_str("\"Succeeded\"").expect("legacy state");
  assert_eq!(restored, EvaluationBranchState::Completed);
}

#[test]
fn required_exhaustion_fails_closed_while_substituted_results_count_toward_quorum() {
  let exhausted = EvaluationProgress::try_new(
    vec![
      EvaluationBranch::new(key("architecture"), false, EvaluationBranchState::Substituted),
      EvaluationBranch::new(key("security"), true, EvaluationBranchState::Exhausted),
    ],
    1,
  )
  .expect("fixture evaluation is valid");
  assert_eq!(
    decide_next_action(&snapshot(
      FactoryRunState::Evaluating,
      FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(exhausted)),
      DecisionSignalProgress::Disabled,
    )),
    Ok(FactoryNextAction::Escalate(
      FactoryEscalationReason::RequiredEvaluationFailed(key("security")),
    ))
  );

  let joined = EvaluationProgress::try_new(
    vec![
      EvaluationBranch::new(key("architecture"), false, EvaluationBranchState::Substituted),
      EvaluationBranch::new(key("security"), false, EvaluationBranchState::Completed),
    ],
    2,
  )
  .expect("fixture evaluation is valid");
  assert_eq!(
    decide_next_action(&snapshot(
      FactoryRunState::Evaluating,
      FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(joined)),
      DecisionSignalProgress::Disabled,
    )),
    Ok(FactoryNextAction::Decide)
  );
}

#[test]
fn evaluation_branch_progression_is_single_step_and_monotonic() {
  let pending = EvaluationProgress::try_new(
    vec![
      EvaluationBranch::new(key("architecture"), true, EvaluationBranchState::Pending),
      EvaluationBranch::new(key("security"), true, EvaluationBranchState::Pending),
    ],
    2,
  )
  .expect("fixture evaluation is valid");
  let started = pending
    .advance_branch(&key("architecture"), EvaluationBranchState::AttemptCreated)
    .expect("pending branch may create an attempt");

  assert!(
    validate_lifecycle_transition(
      FactoryRunState::Evaluating,
      &FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(pending.clone())),
      FactoryRunState::Evaluating,
      &FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(started)),
    )
    .is_ok()
  );
  assert!(
    pending
      .advance_branch(&key("architecture"), EvaluationBranchState::Completed)
      .is_err(),
    "a pending branch cannot skip attempt creation and Build execution"
  );
}

#[test]
fn a_missing_required_evaluation_branch_prevents_other_dispatch() {
  let progress = EvaluationProgress::try_new(
    vec![
      EvaluationBranch::new(key("architecture"), false, EvaluationBranchState::Pending),
      EvaluationBranch::new(key("security"), true, EvaluationBranchState::Missing),
    ],
    1,
  )
  .expect("fixture evaluation is valid");
  let subject = snapshot(
    FactoryRunState::Evaluating,
    FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(progress)),
    DecisionSignalProgress::Disabled,
  );

  assert_eq!(
    decide_next_action(&subject),
    Ok(FactoryNextAction::Escalate(
      FactoryEscalationReason::RequiredEvaluationFailed(key("security")),
    ))
  );
}

#[test]
fn evaluation_join_requires_quorum_before_decision() {
  let cases = [
    (
      vec![
        EvaluationBranch::new(key("architecture"), true, EvaluationBranchState::Completed),
        EvaluationBranch::new(key("security"), false, EvaluationBranchState::Completed),
      ],
      FactoryNextAction::Decide,
    ),
    (
      vec![
        EvaluationBranch::new(key("architecture"), false, EvaluationBranchState::Completed),
        EvaluationBranch::new(key("security"), false, EvaluationBranchState::Failed),
      ],
      FactoryNextAction::Escalate(FactoryEscalationReason::EvaluationQuorum),
    ),
  ];

  for (branches, expected) in cases {
    let progress = EvaluationProgress::try_new(branches, 2).expect("fixture evaluation is valid");
    assert_eq!(
      decide_next_action(&snapshot(
        FactoryRunState::Evaluating,
        FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(progress)),
        DecisionSignalProgress::Disabled,
      )),
      Ok(expected)
    );
  }
}

#[test]
fn only_a_completed_evaluation_join_may_record_a_decision() {
  let completed = EvaluationProgress::try_new(
    vec![EvaluationBranch::new(
      key("architecture"),
      true,
      EvaluationBranchState::Completed,
    )],
    1,
  )
  .expect("fixture evaluation is valid");
  let pending = EvaluationProgress::try_new(
    vec![EvaluationBranch::new(
      key("architecture"),
      true,
      EvaluationBranchState::Pending,
    )],
    1,
  )
  .expect("fixture evaluation is valid");
  let decision = FactoryLifecycleProgress::Evaluating(EvaluationState::DecisionRecorded {
    decision_id: DecisionId::generate(),
    outcome: DecisionOutcome::Accept,
  });

  assert!(
    validate_lifecycle_transition(
      FactoryRunState::Evaluating,
      &FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(completed)),
      FactoryRunState::Evaluating,
      &decision,
    )
    .is_ok()
  );
  assert!(
    validate_lifecycle_transition(
      FactoryRunState::Evaluating,
      &FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(pending)),
      FactoryRunState::Evaluating,
      &decision,
    )
    .is_err(),
    "a Decision cannot bypass unfinished evaluator work"
  );
}

#[test]
fn recorded_decisions_cover_delivery_rework_and_terminal_dispositions() {
  let cases = [
    (
      DecisionOutcome::Accept,
      0,
      1,
      DecisionOutcome::Escalate,
      FactoryNextAction::PrepareDelivery,
    ),
    (
      DecisionOutcome::Rework,
      0,
      1,
      DecisionOutcome::Escalate,
      FactoryNextAction::RequestRework,
    ),
    (
      DecisionOutcome::Rework,
      1,
      1,
      DecisionOutcome::Escalate,
      FactoryNextAction::Escalate(FactoryEscalationReason::ReworkExhausted),
    ),
    (
      DecisionOutcome::Rework,
      1,
      1,
      DecisionOutcome::Reject,
      FactoryNextAction::Reject,
    ),
    (
      DecisionOutcome::Reject,
      0,
      1,
      DecisionOutcome::Escalate,
      FactoryNextAction::Reject,
    ),
    (
      DecisionOutcome::Escalate,
      0,
      1,
      DecisionOutcome::Escalate,
      FactoryNextAction::Escalate(FactoryEscalationReason::Decision),
    ),
    (
      DecisionOutcome::Cancel,
      0,
      1,
      DecisionOutcome::Escalate,
      FactoryNextAction::Cancel,
    ),
  ];

  for (outcome, completed_rework_cycles, max_rework_cycles, exhausted_outcome, expected) in cases {
    let snapshot = snapshot_with_rework(
      FactoryRunState::Evaluating,
      FactoryLifecycleProgress::Evaluating(EvaluationState::DecisionRecorded {
        decision_id: DecisionId::generate(),
        outcome,
      }),
      completed_rework_cycles,
      max_rework_cycles,
      exhausted_outcome,
    );
    let actual = decide_next_action(&snapshot);
    assert_eq!(actual, Ok(expected));
    if outcome == DecisionOutcome::Rework && completed_rework_cycles >= max_rework_cycles {
      assert!(
        actual.expect("exhausted disposition").dispatch_key().is_none(),
        "rework exhaustion must not dispatch another model call"
      );
    }
  }
}

#[test]
fn rework_authority_rejects_impossible_history_and_non_terminal_exhaustion_policy() {
  assert!(FactoryReworkStatus::new(2, 1, DecisionOutcome::Escalate).is_err());
  for outcome in [
    DecisionOutcome::Accept,
    DecisionOutcome::Rework,
    DecisionOutcome::Cancel,
  ] {
    assert!(FactoryReworkStatus::new(0, 1, outcome).is_err());
  }
}

#[test]
fn delivery_reporting_and_completion_remain_separate_actions() {
  let cases = [
    (
      FactoryRunState::ReadyForDelivery,
      FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::AwaitingApproval),
      FactoryNextAction::Wait(FactoryWaitReason::DeliveryApproval),
    ),
    (
      FactoryRunState::ReadyForDelivery,
      FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::Requested),
      FactoryNextAction::RequestDelivery,
    ),
    (
      FactoryRunState::Delivering,
      FactoryLifecycleProgress::Delivering(DeliveryProgress::Succeeded(ReportingProgress::Ready)),
      FactoryNextAction::Report,
    ),
    (
      FactoryRunState::Rejected,
      FactoryLifecycleProgress::Rejected(ReportingProgress::Succeeded),
      FactoryNextAction::Complete,
    ),
    (
      FactoryRunState::Cancelled,
      FactoryLifecycleProgress::Cancelled(ReportingProgress::Exhausted),
      FactoryNextAction::Complete,
    ),
    (
      FactoryRunState::Escalated,
      FactoryLifecycleProgress::Escalated(ReportingProgress::Succeeded),
      FactoryNextAction::Wait(FactoryWaitReason::EscalationDisposition),
    ),
    (
      FactoryRunState::Completed,
      FactoryLifecycleProgress::Completed,
      FactoryNextAction::Wait(FactoryWaitReason::Completed),
    ),
  ];

  for (state, progress, expected) in cases {
    assert_eq!(
      decide_next_action(&snapshot(state, progress, DecisionSignalProgress::Disabled)),
      Ok(expected)
    );
  }
}

#[test]
fn operator_control_transitions_are_closed_and_single_step() {
  assert!(
    validate_lifecycle_transition(
      FactoryRunState::ReadyForDelivery,
      &FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::AwaitingApproval),
      FactoryRunState::ReadyForDelivery,
      &FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::Requested),
    )
    .is_ok()
  );
  assert!(
    validate_lifecycle_transition(
      FactoryRunState::Escalated,
      &FactoryLifecycleProgress::Escalated(ReportingProgress::Succeeded),
      FactoryRunState::Cancelled,
      &FactoryLifecycleProgress::Cancelled(ReportingProgress::Ready),
    )
    .is_ok()
  );
  assert!(
    validate_lifecycle_transition(
      FactoryRunState::Escalated,
      &FactoryLifecycleProgress::Escalated(ReportingProgress::Succeeded),
      FactoryRunState::ReadyForDelivery,
      &FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::Requested),
    )
    .is_err(),
    "an escalation disposition cannot manufacture delivery approval"
  );
}
