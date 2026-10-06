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
    budget(),
    usage,
    FactoryWipLimits::new(3, 3).expect("fixture WIP limits are valid"),
    wip_usage,
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
      EvaluationBranch::new(key("security"), true, EvaluationBranchState::Succeeded),
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
        EvaluationBranch::new(key("architecture"), true, EvaluationBranchState::Succeeded),
        EvaluationBranch::new(key("security"), false, EvaluationBranchState::Succeeded),
      ],
      FactoryNextAction::Decide,
    ),
    (
      vec![
        EvaluationBranch::new(key("architecture"), false, EvaluationBranchState::Succeeded),
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
fn recorded_decisions_cover_delivery_rework_and_terminal_dispositions() {
  let cases = [
    (DecisionOutcome::Accept, 0, 1, FactoryNextAction::PrepareDelivery),
    (DecisionOutcome::Rework, 0, 1, FactoryNextAction::RequestRework),
    (
      DecisionOutcome::Rework,
      1,
      1,
      FactoryNextAction::Escalate(FactoryEscalationReason::ReworkExhausted),
    ),
    (DecisionOutcome::Reject, 0, 1, FactoryNextAction::Reject),
    (
      DecisionOutcome::Escalate,
      0,
      1,
      FactoryNextAction::Escalate(FactoryEscalationReason::Decision),
    ),
    (DecisionOutcome::Cancel, 0, 1, FactoryNextAction::Cancel),
  ];

  for (outcome, completed_rework_cycles, max_rework_cycles, expected) in cases {
    assert_eq!(
      decide_next_action(&snapshot(
        FactoryRunState::Evaluating,
        FactoryLifecycleProgress::Evaluating(EvaluationState::DecisionRecorded {
          outcome,
          completed_rework_cycles,
          max_rework_cycles,
        }),
        DecisionSignalProgress::Disabled,
      )),
      Ok(expected)
    );
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
