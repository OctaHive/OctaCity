use std::collections::HashSet;

use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

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
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FactoryClaimFence(FactoryDigest);

impl FactoryClaimFence {
  /// Wraps an opaque digest without making it displayable in diagnostics.
  #[must_use]
  pub const fn new(value: FactoryDigest) -> Self {
    Self(value)
  }

  /// Returns the opaque digest for durable equality and identity binding.
  #[must_use]
  pub const fn digest(self) -> FactoryDigest {
    self.0
  }
}

/// Current exclusive ownership window for one Factory Run reconciliation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

  /// Returns the opaque fence bound to this ownership window.
  #[must_use]
  pub const fn fence(self) -> FactoryClaimFence {
    self.fence
  }

  /// Returns the authoritative instant at which ownership began.
  #[must_use]
  pub const fn claimed_at(self) -> Timestamp {
    self.claimed_at
  }

  /// Returns the exclusive ownership deadline.
  #[must_use]
  pub const fn expires_at(self) -> Timestamp {
    self.expires_at
  }

  /// Verifies that a presented fence owns the claim at an authoritative time.
  pub fn verify_fence(self, presented_fence: FactoryClaimFence, observed_at: Timestamp) -> Result<(), FactoryError> {
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
    self.claim.verify_fence(self.presented_fence, self.observed_at)?;
    self.budget_usage.validate(self.budget_limit)?;
    self.wip_usage.validate(self.wip_limits)
  }
}

/// Progress of an optional non-authoritative Decision Signal gate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

  /// Returns a canonical stable key for durable action identities.
  #[must_use]
  pub fn canonical_key(&self) -> String {
    match self {
      Self::Implementation => "implementation".to_owned(),
      Self::Validation => "validation".to_owned(),
      Self::Rework => "rework".to_owned(),
      Self::Evaluation(key) => format!("evaluation.{}", key.as_str()),
    }
  }
}

/// Persisted progress of one implementation, validation, or rework stage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
  /// An operator authorized exactly one bounded retry of the failed attempt.
  RetryRequested,
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
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
  /// An operator authorized exactly one bounded retry of this branch.
  RetryRequested,
  /// The branch exhausted its failure policy.
  Failed,
  /// A required planned branch is absent from persisted execution facts.
  Missing,
  /// The branch Build was cancelled.
  Cancelled,
}

/// One expected evaluator branch and its authoritative progress.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

  /// Returns the stable evaluator branch key.
  #[must_use]
  pub const fn key(&self) -> &FactoryKey {
    &self.key
  }

  /// Returns the current authoritative branch progress.
  #[must_use]
  pub const fn state(&self) -> EvaluationBranchState {
    self.state
  }

  fn with_state(&self, state: EvaluationBranchState) -> Self {
    Self {
      key: self.key.clone(),
      required: self.required,
      state,
    }
  }
}

/// Canonically ordered evaluator branches and their deterministic quorum.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

  /// Advances exactly one evaluator branch through an allowed immediate transition.
  pub fn advance_branch(&self, key: &FactoryKey, next_state: EvaluationBranchState) -> Result<Self, FactoryError> {
    let Some(current) = self.branches.iter().find(|branch| &branch.key == key) else {
      return Err(invalid_lifecycle(FactoryRunState::Evaluating));
    };
    if !valid_evaluation_branch_transition(current.state, next_state) {
      return Err(invalid_lifecycle(FactoryRunState::Evaluating));
    }
    let branches = self
      .branches
      .iter()
      .map(|branch| {
        if &branch.key == key {
          branch.with_state(next_state)
        } else {
          branch.clone()
        }
      })
      .collect();
    Ok(Self {
      branches,
      required_quorum: self.required_quorum,
    })
  }

  /// Returns evaluator branches in canonical key order.
  #[must_use]
  pub fn branches(&self) -> &[EvaluationBranch] {
    &self.branches
  }

  fn is_immediate_successor(&self, next: &Self) -> bool {
    if self.required_quorum != next.required_quorum || self.branches.len() != next.branches.len() {
      return false;
    }
    let mut changed = 0_u8;
    self.branches.iter().zip(&next.branches).all(|(previous, next)| {
      if previous.key != next.key || previous.required != next.required {
        return false;
      }
      if previous.state == next.state {
        return true;
      }
      changed = changed.saturating_add(1);
      changed == 1 && valid_evaluation_branch_transition(previous.state, next.state)
    }) && changed == 1
  }
}

const fn valid_evaluation_branch_transition(previous: EvaluationBranchState, next: EvaluationBranchState) -> bool {
  matches!(
    (previous, next),
    (
      EvaluationBranchState::Pending | EvaluationBranchState::RetryRequested,
      EvaluationBranchState::AttemptCreated
    ) | (
      EvaluationBranchState::RetryableFailure,
      EvaluationBranchState::RetryRequested
    ) | (
      EvaluationBranchState::AttemptCreated,
      EvaluationBranchState::BuildActive
    ) | (
      EvaluationBranchState::BuildActive,
      EvaluationBranchState::Succeeded
        | EvaluationBranchState::RetryableFailure
        | EvaluationBranchState::Failed
        | EvaluationBranchState::Missing
        | EvaluationBranchState::Cancelled
    )
  )
}

/// Persisted evaluation planning, fan-out, join, and Decision progress.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DeliveryIntent {
  /// Human approval has not been recorded.
  AwaitingApproval,
  /// A preconditioned delivery-for-review intent was accepted.
  Requested,
}

/// Closed operator dispositions for a Run that is waiting in escalation.
///
/// The value is deliberately smaller than [`FactoryRunState`]: management
/// clients may acknowledge reviewed escalation or request cancellation, but
/// cannot manufacture acceptance, rejection, delivery, or completion facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryEscalationDisposition {
  /// Record that the escalated outcome was reviewed and close the Run.
  Acknowledge,
  /// Request the normal cancellation path and its active-Build propagation.
  Cancel,
}

impl FactoryEscalationDisposition {
  /// Returns the stable value used in audit and idempotency material.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Acknowledge => "acknowledge",
      Self::Cancel => "cancel",
    }
  }
}

/// Non-authoritative external reporting progress.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

impl FactoryLifecycleProgress {
  /// Returns the number of Stage Attempts that currently consume stage WIP.
  ///
  /// The value is derived only from persisted lifecycle facts and can be
  /// reconstructed after a process restart.
  #[must_use]
  pub fn active_stage_count(&self) -> u32 {
    match self {
      Self::Stage {
        progress: FactoryStageProgress::AttemptCreated | FactoryStageProgress::BuildActive,
        ..
      } => 1,
      Self::Evaluating(EvaluationState::Branches(progress)) => progress
        .branches
        .iter()
        .filter(|branch| {
          matches!(
            branch.state,
            EvaluationBranchState::AttemptCreated | EvaluationBranchState::BuildActive
          )
        })
        .count()
        .try_into()
        .unwrap_or(u32::MAX),
      Self::Admitted
      | Self::Stage { .. }
      | Self::Evaluating(_)
      | Self::ReadyForDelivery(_)
      | Self::Delivering(_)
      | Self::Escalated(_)
      | Self::Rejected(_)
      | Self::Cancelled(_)
      | Self::Completed => 0,
    }
  }
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
  /// Waiting for explicit authorization to retry an infrastructure failure.
  RetryApproval(FactoryStageTarget),
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

impl FactoryNextAction {
  /// Returns the canonical stable representation used for durable operation identities.
  #[must_use]
  pub fn canonical_key(&self) -> String {
    match self {
      Self::Wait(reason) => format!("wait:{}", wait_reason_key(reason)),
      Self::RequestDecisionSignal(purpose) => format!("signal.request:{}", purpose.as_str()),
      Self::ConsumeDecisionSignal => "signal.consume".to_owned(),
      Self::CreateStageAttempt(target) => format!("stage.create:{}", target.canonical_key()),
      Self::CreateBuild(target) => format!("build.create:{}", target.canonical_key()),
      Self::RetryStage(target) => format!("stage.retry:{}", target.canonical_key()),
      Self::CaptureCandidate => "candidate.capture".to_owned(),
      Self::ConstructEvidence => "evidence.construct".to_owned(),
      Self::PlanEvaluations => "evaluation.plan".to_owned(),
      Self::Decide => "decision.compute".to_owned(),
      Self::RequestRework => "rework.request".to_owned(),
      Self::Escalate(reason) => format!("escalate:{}", escalation_reason_key(reason)),
      Self::Reject => "reject".to_owned(),
      Self::Cancel => "cancel".to_owned(),
      Self::PrepareDelivery => "delivery.prepare".to_owned(),
      Self::RequestDelivery => "delivery.request".to_owned(),
      Self::Report => "report".to_owned(),
      Self::Complete => "complete".to_owned(),
    }
  }

  /// Returns the stable worker kind when this action requires a durable
  /// external dispatch rather than only an aggregate transition.
  #[must_use]
  pub const fn dispatch_key(&self) -> Option<&'static str> {
    match self {
      Self::RequestDecisionSignal(_) => Some("signal.request"),
      Self::ConsumeDecisionSignal => Some("signal.consume"),
      Self::CreateBuild(_) => Some("build.create"),
      Self::CaptureCandidate => Some("candidate.capture"),
      Self::ConstructEvidence => Some("evidence.construct"),
      Self::PlanEvaluations => Some("evaluation.plan"),
      Self::Decide => Some("decision.compute"),
      Self::Cancel => Some("run.cancel"),
      Self::RequestDelivery => Some("delivery.request"),
      Self::Report => Some("report.dispatch"),
      Self::Wait(_)
      | Self::CreateStageAttempt(_)
      | Self::RetryStage(_)
      | Self::RequestRework
      | Self::Escalate(_)
      | Self::Reject
      | Self::PrepareDelivery
      | Self::Complete => None,
    }
  }
}

fn escalation_reason_key(reason: &FactoryEscalationReason) -> String {
  match reason {
    FactoryEscalationReason::BudgetExhausted(resource) => format!("budget.{}", resource.as_str()),
    FactoryEscalationReason::StageFailed(target) => format!("stage.{}", target.canonical_key()),
    FactoryEscalationReason::RequiredEvaluationFailed(key) => format!("evaluation.{}", key.as_str()),
    FactoryEscalationReason::EvaluationQuorum => "evaluation.quorum".to_owned(),
    FactoryEscalationReason::ReworkExhausted => "rework.exhausted".to_owned(),
    FactoryEscalationReason::Decision => "decision".to_owned(),
    FactoryEscalationReason::DeliveryFailed => "delivery.failed".to_owned(),
  }
}

const fn wait_reason_key(reason: &FactoryWaitReason) -> &'static str {
  match reason {
    FactoryWaitReason::DecisionSignal => "decision_signal",
    FactoryWaitReason::Build(_) => "build",
    FactoryWaitReason::RetryApproval(_) => "retry_approval",
    FactoryWaitReason::StageCapacity => "stage_capacity",
    FactoryWaitReason::EvaluationJoin => "evaluation_join",
    FactoryWaitReason::DeliveryApproval => "delivery_approval",
    FactoryWaitReason::Delivery => "delivery",
    FactoryWaitReason::EscalationDisposition => "escalation_disposition",
    FactoryWaitReason::Reporting => "reporting",
    FactoryWaitReason::Completed => "completed",
  }
}

/// Derives exactly one permitted next action from a complete immutable snapshot.
pub fn decide_next_action(snapshot: &FactoryLifecycleSnapshot) -> Result<FactoryNextAction, FactoryError> {
  snapshot.guard.validate()?;
  validate_lifecycle_progress(snapshot.state, &snapshot.progress)?;

  if snapshot.cancellation_requested {
    if snapshot.state.is_terminal() {
      return Err(invalid_lifecycle(snapshot.state));
    }
    return Ok(FactoryNextAction::Cancel);
  }

  if snapshot.state.is_active() {
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
      EvaluationBranchState::RetryableFailure => {
        Some(FactoryNextAction::Wait(FactoryWaitReason::RetryApproval(target)))
      }
      EvaluationBranchState::RetryRequested => Some(FactoryNextAction::RetryStage(target)),
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

/// Verifies that a persisted lifecycle checkpoint matches its Run state.
pub fn validate_lifecycle_progress(
  state: FactoryRunState,
  progress: &FactoryLifecycleProgress,
) -> Result<(), FactoryError> {
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

/// Validates that one persisted lifecycle checkpoint is the immediate successor
/// of another rather than an arbitrary valid state jump.
pub fn validate_lifecycle_transition(
  previous_state: FactoryRunState,
  previous: &FactoryLifecycleProgress,
  next_state: FactoryRunState,
  next: &FactoryLifecycleProgress,
) -> Result<(), FactoryError> {
  validate_lifecycle_progress(previous_state, previous)?;
  validate_lifecycle_progress(next_state, next)?;
  let valid = if previous_state == next_state && previous == next {
    true
  } else {
    match (previous, next) {
      (
        FactoryLifecycleProgress::Admitted,
        FactoryLifecycleProgress::Stage {
          target: FactoryStageTarget::Implementation,
          progress: FactoryStageProgress::AttemptCreated,
        },
      ) => previous_state == FactoryRunState::Admitted && next_state == FactoryRunState::Implementing,
      (
        FactoryLifecycleProgress::Stage {
          target: previous_target,
          progress: previous_progress,
        },
        FactoryLifecycleProgress::Stage {
          target: next_target,
          progress: next_progress,
        },
      ) if previous_target == next_target && previous_state == next_state => matches!(
        (previous_progress, next_progress),
        (
          FactoryStageProgress::Ready | FactoryStageProgress::RetryRequested,
          FactoryStageProgress::AttemptCreated
        ) | (
          FactoryStageProgress::RetryableFailure,
          FactoryStageProgress::RetryRequested
        ) | (FactoryStageProgress::AttemptCreated, FactoryStageProgress::BuildActive)
          | (
            FactoryStageProgress::BuildActive,
            FactoryStageProgress::BuildSucceeded
              | FactoryStageProgress::RetryableFailure
              | FactoryStageProgress::Failed
              | FactoryStageProgress::Cancelled
          )
          | (
            FactoryStageProgress::BuildSucceeded,
            FactoryStageProgress::CandidateCaptured
          )
          | (
            FactoryStageProgress::BuildSucceeded,
            FactoryStageProgress::EvidenceConstructed
          )
      ),
      (
        FactoryLifecycleProgress::Stage {
          target: FactoryStageTarget::Implementation | FactoryStageTarget::Rework,
          progress: FactoryStageProgress::CandidateCaptured,
        },
        FactoryLifecycleProgress::Stage {
          target: FactoryStageTarget::Validation,
          progress: FactoryStageProgress::AttemptCreated,
        },
      ) => next_state == FactoryRunState::Validating,
      (
        FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(previous)),
        FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(next)),
      ) => {
        previous_state == FactoryRunState::Evaluating
          && next_state == FactoryRunState::Evaluating
          && previous.is_immediate_successor(next)
      }
      (
        FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(previous)),
        FactoryLifecycleProgress::Evaluating(EvaluationState::DecisionRecorded {
          completed_rework_cycles,
          max_rework_cycles,
          ..
        }),
      ) => {
        previous_state == FactoryRunState::Evaluating
          && next_state == FactoryRunState::Evaluating
          && completed_rework_cycles <= max_rework_cycles
          && matches!(decide_evaluation_branches(previous), Ok(FactoryNextAction::Decide))
      }
      (
        FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::AwaitingApproval),
        FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::Requested),
      ) => previous_state == FactoryRunState::ReadyForDelivery && next_state == previous_state,
      _ => allowed_state_successor(previous_state, next_state),
    }
  };
  if valid {
    Ok(())
  } else {
    Err(invalid_lifecycle(next_state))
  }
}

const fn allowed_state_successor(previous: FactoryRunState, next: FactoryRunState) -> bool {
  matches!(
    (previous, next),
    (
      FactoryRunState::Admitted,
      FactoryRunState::Escalated | FactoryRunState::Cancelled
    ) | (
      FactoryRunState::Implementing | FactoryRunState::Reworking,
      FactoryRunState::Validating | FactoryRunState::Escalated | FactoryRunState::Cancelled
    ) | (
      FactoryRunState::Validating,
      FactoryRunState::Evaluating | FactoryRunState::Escalated | FactoryRunState::Cancelled
    ) | (
      FactoryRunState::Evaluating,
      FactoryRunState::Reworking
        | FactoryRunState::ReadyForDelivery
        | FactoryRunState::Escalated
        | FactoryRunState::Rejected
        | FactoryRunState::Cancelled
    ) | (
      FactoryRunState::ReadyForDelivery,
      FactoryRunState::Delivering | FactoryRunState::Escalated | FactoryRunState::Cancelled
    ) | (
      FactoryRunState::Delivering,
      FactoryRunState::Completed | FactoryRunState::Escalated | FactoryRunState::Cancelled
    ) | (
      FactoryRunState::Escalated,
      FactoryRunState::Cancelled | FactoryRunState::Completed
    ) | (
      FactoryRunState::Rejected | FactoryRunState::Cancelled,
      FactoryRunState::Completed
    )
  )
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
    FactoryStageProgress::RetryableFailure => FactoryNextAction::Wait(FactoryWaitReason::RetryApproval(target.clone())),
    FactoryStageProgress::RetryRequested => FactoryNextAction::RetryStage(target.clone()),
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
