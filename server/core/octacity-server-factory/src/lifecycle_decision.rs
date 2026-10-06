use std::collections::HashSet;

use octacity_server_domain::Timestamp;

use crate::{
  BudgetLimit, BudgetResource, BudgetUsage, DecisionOutcome, DecisionSignalPurpose, FactoryDigest, FactoryError,
  FactoryKey, FactoryRunState, FactoryStageKind, FactoryWipLimits, MAX_EVALUATORS,
};

const ALL_BUDGET_RESOURCES: [BudgetResource; 5] = [
  BudgetResource::Attempts,
  BudgetResource::ElapsedTime,
  BudgetResource::Tokens,
  BudgetResource::Cost,
  BudgetResource::OutputBytes,
];
const BUILD_BUDGET_RESOURCES: [BudgetResource; 4] = [
  BudgetResource::ElapsedTime,
  BudgetResource::Tokens,
  BudgetResource::Cost,
  BudgetResource::OutputBytes,
];
const OUTPUT_BUDGET_RESOURCES: [BudgetResource; 2] = [BudgetResource::ElapsedTime, BudgetResource::OutputBytes];
const EXTERNAL_BUDGET_RESOURCES: [BudgetResource; 4] = [
  BudgetResource::Attempts,
  BudgetResource::ElapsedTime,
  BudgetResource::Cost,
  BudgetResource::OutputBytes,
];

/// Opaque content-derived fencing value for one Factory reconciler claim.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FactoryClaimFence(FactoryDigest);

impl FactoryClaimFence {
  /// Wraps an opaque digest without making it displayable in diagnostics.
  #[must_use]
  pub const fn new(value: FactoryDigest) -> Self {
    Self(value)
  }
}

/// Current exclusive ownership window for one Factory Run reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryClaim {
  fence: FactoryClaimFence,
  claimed_at: Timestamp,
  expires_at: Timestamp,
}

impl FactoryClaim {
  /// Constructs a positive claim window.
  pub fn new(fence: FactoryClaimFence, claimed_at: Timestamp, expires_at: Timestamp) -> Result<Self, FactoryError> {
    if expires_at <= claimed_at {
      return Err(FactoryError::InvalidClaim);
    }
    Ok(Self {
      fence,
      claimed_at,
      expires_at,
    })
  }

  fn validate(self, presented_fence: FactoryClaimFence, observed_at: Timestamp) -> Result<(), FactoryError> {
    if self.fence != presented_fence {
      return Err(FactoryError::StaleClaim);
    }
    if observed_at < self.claimed_at {
      return Err(FactoryError::InvalidClaim);
    }
    if observed_at >= self.expires_at {
      return Err(FactoryError::ExpiredClaim);
    }
    Ok(())
  }
}

/// Authoritative WIP counters observed with one Factory snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryWipUsage {
  active_runs: u32,
  active_stages: u32,
}

impl FactoryWipUsage {
  /// Constructs WIP usage including the current non-terminal Run.
  #[must_use]
  pub const fn new(active_runs: u32, active_stages: u32) -> Self {
    Self {
      active_runs,
      active_stages,
    }
  }

  fn validate(self, limits: FactoryWipLimits) -> Result<(), FactoryError> {
    if self.active_runs == 0
      || self.active_runs > limits.max_active_runs()
      || self.active_stages > limits.max_active_stages()
    {
      return Err(FactoryError::WipExceeded);
    }
    Ok(())
  }

  const fn stage_capacity_available(self, limits: FactoryWipLimits) -> bool {
    self.active_stages < limits.max_active_stages()
  }
}

/// Claim, budget, and WIP facts required before a pure lifecycle decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryDecisionGuard {
  claim: FactoryClaim,
  presented_fence: FactoryClaimFence,
  observed_at: Timestamp,
  budget_limit: BudgetLimit,
  budget_usage: BudgetUsage,
  wip_limits: FactoryWipLimits,
  wip_usage: FactoryWipUsage,
}

impl FactoryDecisionGuard {
  /// Captures the authoritative decision guard loaded under one claim.
  #[must_use]
  pub const fn new(
    claim: FactoryClaim,
    presented_fence: FactoryClaimFence,
    observed_at: Timestamp,
    budget_limit: BudgetLimit,
    budget_usage: BudgetUsage,
    wip_limits: FactoryWipLimits,
    wip_usage: FactoryWipUsage,
  ) -> Self {
    Self {
      claim,
      presented_fence,
      observed_at,
      budget_limit,
      budget_usage,
      wip_limits,
      wip_usage,
    }
  }

  fn validate(self) -> Result<(), FactoryError> {
    self.claim.validate(self.presented_fence, self.observed_at)?;
    self.budget_usage.validate(self.budget_limit)?;
    self.wip_usage.validate(self.wip_limits)
  }
}

/// Progress of an optional non-authoritative Decision Signal gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionSignalProgress {
  /// No signal is configured for the current deterministic choice.
  Disabled,
  /// A stable signal request may be recorded and dispatched.
  ReadyToRequest(DecisionSignalPurpose),
  /// The recorded request is awaiting a terminal receipt.
  Waiting,
  /// A terminal receipt is ready for deterministic policy consumption.
  ReadyToConsume,
  /// Deterministic policy consumed the receipt or fallback without transitioning directly from provider output.
  Consumed,
}

/// One program-owned stage, including an evaluation branch when applicable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryStageTarget {
  /// Candidate implementation.
  Implementation,
  /// Deterministic candidate validation.
  Validation,
  /// Bounded candidate rework.
  Rework,
  /// One named independent evaluator branch.
  Evaluation(FactoryKey),
}

impl FactoryStageTarget {
  /// Returns the stable stage kind.
  #[must_use]
  pub const fn kind(&self) -> FactoryStageKind {
    match self {
      Self::Implementation => FactoryStageKind::Implementation,
      Self::Validation => FactoryStageKind::Validation,
      Self::Rework => FactoryStageKind::Rework,
      Self::Evaluation(_) => FactoryStageKind::Evaluation,
    }
  }
}

/// Persisted progress of one implementation, validation, or rework stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryStageProgress {
  /// No attempt has been created.
  Ready,
  /// An append-only attempt exists but has no linked Build.
  AttemptCreated,
  /// The linked ordinary Build is non-terminal.
  BuildActive,
  /// The linked ordinary Build succeeded.
  BuildSucceeded,
  /// A provider-independent failure may start another bounded attempt.
  RetryableFailure,
  /// The stage failed without an eligible retry.
  Failed,
  /// The linked Build was cancelled.
  Cancelled,
  /// Trusted capture accepted an exact candidate.
  CandidateCaptured,
  /// Trusted projection accepted exact deterministic evidence.
  EvidenceConstructed,
}

/// Persisted progress for one evaluator branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationBranchState {
  /// The expected branch has no Stage Attempt yet.
  Pending,
  /// The branch Stage Attempt exists but has no linked Build.
  AttemptCreated,
  /// The linked evaluation Build is non-terminal.
  BuildActive,
  /// A schema-valid Assessment was accepted.
  Succeeded,
  /// A provider-independent failure may create another bounded attempt.
  RetryableFailure,
  /// The branch exhausted its failure policy.
  Failed,
  /// A required planned branch is absent from persisted execution facts.
  Missing,
  /// The branch Build was cancelled.
  Cancelled,
}

/// One expected evaluator branch and its authoritative progress.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationBranch {
  key: FactoryKey,
  required: bool,
  state: EvaluationBranchState,
}

impl EvaluationBranch {
  /// Constructs one planned evaluator branch.
  #[must_use]
  pub const fn new(key: FactoryKey, required: bool, state: EvaluationBranchState) -> Self {
    Self { key, required, state }
  }
}

/// Canonically ordered evaluator branches and their deterministic quorum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationProgress {
  branches: Vec<EvaluationBranch>,
  required_quorum: u16,
}

impl EvaluationProgress {
  /// Constructs a non-empty, bounded branch set with unique identities.
  pub fn try_new(mut branches: Vec<EvaluationBranch>, required_quorum: u16) -> Result<Self, FactoryError> {
    if branches.is_empty()
      || branches.len() > MAX_EVALUATORS
      || required_quorum == 0
      || usize::from(required_quorum) > branches.len()
      || branches.iter().map(|branch| &branch.key).collect::<HashSet<_>>().len() != branches.len()
    {
      return Err(FactoryError::InvalidLifecycle {
        state: FactoryRunState::Evaluating,
      });
    }
    branches.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(Self {
      branches,
      required_quorum,
    })
  }
}

/// Persisted evaluation planning, fan-out, join, and Decision progress.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvaluationState {
  /// Exact evidence exists but no immutable Evaluation Plan has been recorded.
  PlanRequired,
  /// The immutable plan's expected branches are being joined.
  Branches(EvaluationProgress),
  /// The pure Decision Engine recorded one authoritative typed outcome.
  DecisionRecorded {
    /// Typed deterministic outcome.
    outcome: DecisionOutcome,
    /// Rework cycles already consumed before this Decision.
    completed_rework_cycles: u16,
    /// Immutable maximum rework cycles.
    max_rework_cycles: u16,
  },
}

/// Human delivery intent recorded before any write-capable adapter runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryIntent {
  /// Human approval has not been recorded.
  AwaitingApproval,
  /// A preconditioned delivery-for-review intent was accepted.
  Requested,
}

/// Non-authoritative external reporting progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportingProgress {
  /// No reporter is configured.
  Disabled,
  /// Reporting work is ready for bounded dispatch.
  Ready,
  /// A reporter attempt is active.
  Active,
  /// A failed attempt is eligible for bounded retry.
  RetryableFailure,
  /// Reporting succeeded.
  Succeeded,
  /// Retry budget is exhausted; authoritative lifecycle state remains unchanged.
  Exhausted,
}

/// Delivery-for-human-review progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryProgress {
  /// An accepted intent is ready for adapter dispatch.
  Ready,
  /// A delivery attempt is active.
  Active,
  /// A failed attempt is eligible for bounded observe-before-retry.
  RetryableFailure,
  /// The adapter observed an ambiguous external outcome requiring safe observation.
  Unknown,
  /// Delivery cannot continue without operator disposition.
  Failed,
  /// The exact accepted candidate was delivered for review.
  Succeeded(ReportingProgress),
}

/// Persisted facts sufficient for one pure Factory next-action decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryLifecycleProgress {
  /// Admitted Work has not started implementation.
  Admitted,
  /// One implementation, validation, or rework stage.
  Stage {
    /// Program-owned stage identity.
    target: FactoryStageTarget,
    /// Authoritative stage facts.
    progress: FactoryStageProgress,
  },
  /// Evaluation planning, fan-out, join, and Decision progress.
  Evaluating(EvaluationState),
  /// Accepted candidate waiting for a human delivery intent.
  ReadyForDelivery(DeliveryIntent),
  /// Delivery-for-review progress.
  Delivering(DeliveryProgress),
  /// Escalated Run reporting progress; escalation itself remains authoritative.
  Escalated(ReportingProgress),
  /// Rejected Run reporting progress.
  Rejected(ReportingProgress),
  /// Cancelled Run reporting progress.
  Cancelled(ReportingProgress),
  /// Fully completed lifecycle.
  Completed,
}

/// Complete immutable input to one pure lifecycle decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryLifecycleSnapshot {
  state: FactoryRunState,
  progress: FactoryLifecycleProgress,
  signal: DecisionSignalProgress,
  guard: FactoryDecisionGuard,
  cancellation_requested: bool,
}

impl FactoryLifecycleSnapshot {
  /// Captures one authoritative snapshot loaded under a fenced claim.
  #[must_use]
  pub const fn new(
    state: FactoryRunState,
    progress: FactoryLifecycleProgress,
    signal: DecisionSignalProgress,
    guard: FactoryDecisionGuard,
    cancellation_requested: bool,
  ) -> Self {
    Self {
      state,
      progress,
      signal,
      guard,
      cancellation_requested,
    }
  }
}

/// Stable reason no mutation or external dispatch is currently permitted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryWaitReason {
  /// Waiting for a Decision Signal receipt.
  DecisionSignal,
  /// Waiting for an ordinary linked Build.
  Build(FactoryStageTarget),
  /// Stage WIP is currently full.
  StageCapacity,
  /// Waiting for at least one evaluator branch.
  EvaluationJoin,
  /// Waiting for explicit human delivery approval.
  DeliveryApproval,
  /// Waiting for a delivery adapter observation.
  Delivery,
  /// Waiting for human escalation disposition.
  EscalationDisposition,
  /// Waiting for a reporter attempt.
  Reporting,
  /// The lifecycle is already complete.
  Completed,
}

/// Deterministic reason a Run must stop dispatching and escalate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryEscalationReason {
  /// A hard budget has no capacity for the proposed bounded action.
  BudgetExhausted(BudgetResource),
  /// A stage exhausted its retry policy.
  StageFailed(FactoryStageTarget),
  /// A required evaluator branch is missing or terminally failed.
  RequiredEvaluationFailed(FactoryKey),
  /// Terminal evaluator branches cannot satisfy quorum.
  EvaluationQuorum,
  /// The immutable rework-cycle ceiling is exhausted.
  ReworkExhausted,
  /// The Decision Engine selected escalation.
  Decision,
  /// Delivery cannot be observed or retried safely.
  DeliveryFailed,
}

/// One code-owned action; callers must commit it under the same fence before side effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryNextAction {
  /// No mutation is currently allowed.
  Wait(FactoryWaitReason),
  /// Record and dispatch one optional non-authoritative signal request.
  RequestDecisionSignal(DecisionSignalPurpose),
  /// Apply deterministic policy to a recorded receipt without committing a provider-selected transition.
  ConsumeDecisionSignal,
  /// Append a new Stage Attempt.
  CreateStageAttempt(FactoryStageTarget),
  /// Create an ordinary immutable Build for an existing Stage Attempt.
  CreateBuild(FactoryStageTarget),
  /// Append a bounded retry Stage Attempt.
  RetryStage(FactoryStageTarget),
  /// Invoke trusted exact candidate capture.
  CaptureCandidate,
  /// Construct exact deterministic evidence.
  ConstructEvidence,
  /// Persist an immutable Evaluation Plan.
  PlanEvaluations,
  /// Run the pure Decision Engine after the deterministic join.
  Decide,
  /// Start one configured bounded rework cycle.
  RequestRework,
  /// Move to human or policy escalation without dispatching more work.
  Escalate(FactoryEscalationReason),
  /// Record deterministic rejection.
  Reject,
  /// Record cancellation and propagate it to active Factory-owned Builds.
  Cancel,
  /// Move an accepted candidate to explicit human delivery readiness.
  PrepareDelivery,
  /// Dispatch or safely observe delivery-for-review.
  RequestDelivery,
  /// Dispatch non-authoritative source reporting.
  Report,
  /// Complete a terminal authoritative lifecycle.
  Complete,
}

/// Derives exactly one permitted next action from a complete immutable snapshot.
pub fn decide_next_action(snapshot: &FactoryLifecycleSnapshot) -> Result<FactoryNextAction, FactoryError> {
  snapshot.guard.validate()?;
  validate_progress(snapshot.state, &snapshot.progress)?;

  if snapshot.cancellation_requested {
    if matches!(
      snapshot.state,
      FactoryRunState::Rejected | FactoryRunState::Cancelled | FactoryRunState::Completed
    ) {
      return Err(invalid_lifecycle(snapshot.state));
    }
    return Ok(FactoryNextAction::Cancel);
  }

  if !matches!(
    snapshot.state,
    FactoryRunState::Rejected | FactoryRunState::Cancelled | FactoryRunState::Completed
  ) {
    match snapshot.signal {
      DecisionSignalProgress::ReadyToRequest(purpose) => {
        return guard_action(snapshot, FactoryNextAction::RequestDecisionSignal(purpose));
      }
      DecisionSignalProgress::Waiting => {
        return Ok(FactoryNextAction::Wait(FactoryWaitReason::DecisionSignal));
      }
      DecisionSignalProgress::ReadyToConsume => return Ok(FactoryNextAction::ConsumeDecisionSignal),
      DecisionSignalProgress::Disabled | DecisionSignalProgress::Consumed => {}
    }
  } else if snapshot.signal != DecisionSignalProgress::Disabled {
    return Err(invalid_lifecycle(snapshot.state));
  }

  let action = match &snapshot.progress {
    FactoryLifecycleProgress::Admitted => FactoryNextAction::CreateStageAttempt(FactoryStageTarget::Implementation),
    FactoryLifecycleProgress::Stage { target, progress } => decide_stage(target, *progress, snapshot.state)?,
    FactoryLifecycleProgress::Evaluating(progress) => decide_evaluation(progress)?,
    FactoryLifecycleProgress::ReadyForDelivery(intent) => match intent {
      DeliveryIntent::AwaitingApproval => FactoryNextAction::Wait(FactoryWaitReason::DeliveryApproval),
      DeliveryIntent::Requested => FactoryNextAction::RequestDelivery,
    },
    FactoryLifecycleProgress::Delivering(progress) => decide_delivery(*progress),
    FactoryLifecycleProgress::Escalated(progress) => decide_reporting(*progress, true),
    FactoryLifecycleProgress::Rejected(progress) | FactoryLifecycleProgress::Cancelled(progress) => {
      decide_reporting(*progress, false)
    }
    FactoryLifecycleProgress::Completed => FactoryNextAction::Wait(FactoryWaitReason::Completed),
  };
  guard_action(snapshot, action)
}

fn decide_evaluation(progress: &EvaluationState) -> Result<FactoryNextAction, FactoryError> {
  match progress {
    EvaluationState::PlanRequired => Ok(FactoryNextAction::PlanEvaluations),
    EvaluationState::Branches(progress) => decide_evaluation_branches(progress),
    EvaluationState::DecisionRecorded {
      outcome,
      completed_rework_cycles,
      max_rework_cycles,
    } => {
      if completed_rework_cycles > max_rework_cycles {
        return Err(invalid_lifecycle(FactoryRunState::Evaluating));
      }
      Ok(match outcome {
        DecisionOutcome::Accept => FactoryNextAction::PrepareDelivery,
        DecisionOutcome::Rework if completed_rework_cycles < max_rework_cycles => FactoryNextAction::RequestRework,
        DecisionOutcome::Rework => FactoryNextAction::Escalate(FactoryEscalationReason::ReworkExhausted),
        DecisionOutcome::Reject => FactoryNextAction::Reject,
        DecisionOutcome::Escalate => FactoryNextAction::Escalate(FactoryEscalationReason::Decision),
        DecisionOutcome::Cancel => FactoryNextAction::Cancel,
      })
    }
  }
}

fn decide_evaluation_branches(progress: &EvaluationProgress) -> Result<FactoryNextAction, FactoryError> {
  if let Some(branch) = progress.branches.iter().find(|branch| {
    branch.required
      && matches!(
        branch.state,
        EvaluationBranchState::Missing | EvaluationBranchState::Failed | EvaluationBranchState::Cancelled
      )
  }) {
    return Ok(FactoryNextAction::Escalate(
      FactoryEscalationReason::RequiredEvaluationFailed(branch.key.clone()),
    ));
  }

  for branch in &progress.branches {
    let target = FactoryStageTarget::Evaluation(branch.key.clone());
    let action = match branch.state {
      EvaluationBranchState::Pending => Some(FactoryNextAction::CreateStageAttempt(target)),
      EvaluationBranchState::AttemptCreated => Some(FactoryNextAction::CreateBuild(target)),
      EvaluationBranchState::RetryableFailure => Some(FactoryNextAction::RetryStage(target)),
      EvaluationBranchState::BuildActive
      | EvaluationBranchState::Succeeded
      | EvaluationBranchState::Failed
      | EvaluationBranchState::Missing
      | EvaluationBranchState::Cancelled => None,
    };
    if let Some(action) = action {
      return Ok(action);
    }
  }

  if progress
    .branches
    .iter()
    .any(|branch| branch.state == EvaluationBranchState::BuildActive)
  {
    return Ok(FactoryNextAction::Wait(FactoryWaitReason::EvaluationJoin));
  }

  let succeeded = progress
    .branches
    .iter()
    .filter(|branch| branch.state == EvaluationBranchState::Succeeded)
    .count();
  if succeeded >= usize::from(progress.required_quorum) {
    Ok(FactoryNextAction::Decide)
  } else {
    Ok(FactoryNextAction::Escalate(FactoryEscalationReason::EvaluationQuorum))
  }
}

const fn decide_delivery(progress: DeliveryProgress) -> FactoryNextAction {
  match progress {
    DeliveryProgress::Ready | DeliveryProgress::RetryableFailure | DeliveryProgress::Unknown => {
      FactoryNextAction::RequestDelivery
    }
    DeliveryProgress::Active => FactoryNextAction::Wait(FactoryWaitReason::Delivery),
    DeliveryProgress::Failed => FactoryNextAction::Escalate(FactoryEscalationReason::DeliveryFailed),
    DeliveryProgress::Succeeded(reporting) => decide_reporting(reporting, false),
  }
}

const fn decide_reporting(progress: ReportingProgress, requires_disposition: bool) -> FactoryNextAction {
  match progress {
    ReportingProgress::Ready | ReportingProgress::RetryableFailure => FactoryNextAction::Report,
    ReportingProgress::Active => FactoryNextAction::Wait(FactoryWaitReason::Reporting),
    ReportingProgress::Disabled | ReportingProgress::Succeeded | ReportingProgress::Exhausted
      if requires_disposition =>
    {
      FactoryNextAction::Wait(FactoryWaitReason::EscalationDisposition)
    }
    ReportingProgress::Disabled | ReportingProgress::Succeeded | ReportingProgress::Exhausted => {
      FactoryNextAction::Complete
    }
  }
}

fn validate_progress(state: FactoryRunState, progress: &FactoryLifecycleProgress) -> Result<(), FactoryError> {
  let valid = matches!(
    (state, progress),
    (FactoryRunState::Admitted, FactoryLifecycleProgress::Admitted)
      | (
        FactoryRunState::Implementing,
        FactoryLifecycleProgress::Stage {
          target: FactoryStageTarget::Implementation,
          ..
        }
      )
      | (
        FactoryRunState::Validating,
        FactoryLifecycleProgress::Stage {
          target: FactoryStageTarget::Validation,
          ..
        }
      )
      | (
        FactoryRunState::Reworking,
        FactoryLifecycleProgress::Stage {
          target: FactoryStageTarget::Rework,
          ..
        }
      )
      | (FactoryRunState::Evaluating, FactoryLifecycleProgress::Evaluating(_))
      | (
        FactoryRunState::ReadyForDelivery,
        FactoryLifecycleProgress::ReadyForDelivery(_)
      )
      | (FactoryRunState::Delivering, FactoryLifecycleProgress::Delivering(_))
      | (FactoryRunState::Escalated, FactoryLifecycleProgress::Escalated(_))
      | (FactoryRunState::Rejected, FactoryLifecycleProgress::Rejected(_))
      | (FactoryRunState::Cancelled, FactoryLifecycleProgress::Cancelled(_))
      | (FactoryRunState::Completed, FactoryLifecycleProgress::Completed)
  );
  if valid { Ok(()) } else { Err(invalid_lifecycle(state)) }
}

fn decide_stage(
  target: &FactoryStageTarget,
  progress: FactoryStageProgress,
  state: FactoryRunState,
) -> Result<FactoryNextAction, FactoryError> {
  let action = match progress {
    FactoryStageProgress::Ready => FactoryNextAction::CreateStageAttempt(target.clone()),
    FactoryStageProgress::AttemptCreated => FactoryNextAction::CreateBuild(target.clone()),
    FactoryStageProgress::BuildActive => FactoryNextAction::Wait(FactoryWaitReason::Build(target.clone())),
    FactoryStageProgress::RetryableFailure => FactoryNextAction::RetryStage(target.clone()),
    FactoryStageProgress::Failed => FactoryNextAction::Escalate(FactoryEscalationReason::StageFailed(target.clone())),
    FactoryStageProgress::Cancelled => FactoryNextAction::Cancel,
    FactoryStageProgress::BuildSucceeded
      if matches!(target, FactoryStageTarget::Implementation | FactoryStageTarget::Rework) =>
    {
      FactoryNextAction::CaptureCandidate
    }
    FactoryStageProgress::CandidateCaptured
      if matches!(target, FactoryStageTarget::Implementation | FactoryStageTarget::Rework) =>
    {
      FactoryNextAction::CreateStageAttempt(FactoryStageTarget::Validation)
    }
    FactoryStageProgress::BuildSucceeded if *target == FactoryStageTarget::Validation => {
      FactoryNextAction::ConstructEvidence
    }
    FactoryStageProgress::EvidenceConstructed if *target == FactoryStageTarget::Validation => {
      FactoryNextAction::PlanEvaluations
    }
    _ => return Err(invalid_lifecycle(state)),
  };
  Ok(action)
}

fn guard_action(
  snapshot: &FactoryLifecycleSnapshot,
  action: FactoryNextAction,
) -> Result<FactoryNextAction, FactoryError> {
  for resource in required_budget_resources(&action) {
    if snapshot
      .guard
      .budget_usage
      .is_exhausted(snapshot.guard.budget_limit, *resource)
    {
      return Ok(FactoryNextAction::Escalate(FactoryEscalationReason::BudgetExhausted(
        *resource,
      )));
    }
  }
  if matches!(
    action,
    FactoryNextAction::CreateStageAttempt(_) | FactoryNextAction::RetryStage(_) | FactoryNextAction::RequestRework
  ) && !snapshot
    .guard
    .wip_usage
    .stage_capacity_available(snapshot.guard.wip_limits)
  {
    return Ok(FactoryNextAction::Wait(FactoryWaitReason::StageCapacity));
  }
  Ok(action)
}

fn required_budget_resources(action: &FactoryNextAction) -> &'static [BudgetResource] {
  match action {
    FactoryNextAction::RequestDecisionSignal(_)
    | FactoryNextAction::CreateStageAttempt(_)
    | FactoryNextAction::RetryStage(_)
    | FactoryNextAction::RequestRework => &ALL_BUDGET_RESOURCES,
    FactoryNextAction::CreateBuild(_) => &BUILD_BUDGET_RESOURCES,
    FactoryNextAction::CaptureCandidate | FactoryNextAction::ConstructEvidence => &OUTPUT_BUDGET_RESOURCES,
    FactoryNextAction::RequestDelivery | FactoryNextAction::Report => &EXTERNAL_BUDGET_RESOURCES,
    FactoryNextAction::Wait(_)
    | FactoryNextAction::ConsumeDecisionSignal
    | FactoryNextAction::PlanEvaluations
    | FactoryNextAction::Decide
    | FactoryNextAction::Escalate(_)
    | FactoryNextAction::Reject
    | FactoryNextAction::Cancel
    | FactoryNextAction::PrepareDelivery
    | FactoryNextAction::Complete => &[],
  }
}

const fn invalid_lifecycle(state: FactoryRunState) -> FactoryError {
  FactoryError::InvalidLifecycle { state }
}
