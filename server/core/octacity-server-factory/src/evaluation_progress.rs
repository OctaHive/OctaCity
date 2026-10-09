//! Durable deterministic progress for bounded evaluator fan-out and joins.

use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
  Assessment, AssessmentId, ContextManifestId, DecisionOutcome, EvaluationPlan, FactoryDigest, FactoryError,
  FactoryKey, FactoryRunState, FactoryStageTarget, ImmutableReference, MAX_EVALUATORS, MacroCall, MacroCallId,
  MacroCallKind, ReviewBranch, StageAttempt, StageAttemptId,
};

/// Persisted progress for one evaluator branch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EvaluationBranchState {
  /// The expected branch has no Stage Attempt yet.
  Pending,
  /// The branch Stage Attempt exists but has no linked Build.
  AttemptCreated,
  /// The linked evaluation Build is non-terminal.
  BuildActive,
  /// The Build completed and its typed result is still being validated.
  ResultPending,
  /// A schema-valid Assessment was accepted from the primary evaluator.
  #[serde(alias = "Succeeded")]
  Completed,
  /// A provider-independent failure may create another bounded attempt.
  RetryableFailure,
  /// An operator authorized exactly one bounded retry of this branch.
  RetryRequested,
  /// The branch failed without an eligible retry.
  Failed,
  /// A schema-valid Assessment was accepted from an explicitly declared substitute.
  Substituted,
  /// The branch consumed its bounded retry policy without a terminal Assessment.
  Exhausted,
  /// A required planned branch is absent from persisted execution facts.
  Missing,
  /// The branch Build was cancelled.
  Cancelled,
}

/// Immutable accepted result of one evaluator branch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationBranchResult {
  branch: FactoryKey,
  evaluator: ImmutableReference,
  assessment_id: AssessmentId,
  stage_attempt_id: StageAttemptId,
  call_id: MacroCallId,
  context_manifest_id: ContextManifestId,
  context_digest: FactoryDigest,
  substituted: bool,
}

impl EvaluationBranchResult {
  /// Binds a validated Assessment to its exact plan branch and scoped call context.
  pub fn new(
    plan: &EvaluationPlan,
    stage: &StageAttempt,
    call: &MacroCall,
    assessment: &Assessment,
  ) -> Result<Self, FactoryError> {
    let branch = plan
      .branches()
      .iter()
      .find(|branch| branch.allows_evaluator(assessment.evaluator_reference()))
      .ok_or(FactoryError::InvalidReference {
        relationship: "evaluation branch result evaluator",
      })?;
    if assessment.plan_id() != plan.id()
      || assessment.subject() != plan.subject()
      || call.kind() != MacroCallKind::Evaluate
      || call.subject().candidate() != Some(plan.subject())
      || call.stage_attempt_id() != stage.id()
      || stage.subject() != plan.subject().exact()
      || stage.target() != &FactoryStageTarget::Evaluation(branch.key().clone())
    {
      return Err(FactoryError::InvalidReference {
        relationship: "evaluation branch result bindings",
      });
    }
    Ok(Self {
      branch: branch.key().clone(),
      evaluator: assessment.evaluator_reference().clone(),
      assessment_id: assessment.id(),
      stage_attempt_id: call.stage_attempt_id(),
      call_id: call.id(),
      context_manifest_id: call.context_manifest_id(),
      context_digest: call.context_digest(),
      substituted: assessment.evaluator_reference() != branch.evaluator(),
    })
  }

  /// Returns the logical plan branch satisfied by this result.
  #[must_use]
  pub const fn branch(&self) -> &FactoryKey {
    &self.branch
  }

  /// Returns the exact evaluator that produced the accepted Assessment.
  #[must_use]
  pub const fn evaluator(&self) -> &ImmutableReference {
    &self.evaluator
  }

  /// Returns the accepted immutable Assessment identity.
  #[must_use]
  pub const fn assessment_id(&self) -> AssessmentId {
    self.assessment_id
  }

  /// Returns the Stage Attempt that owned the evaluator call.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the durable evaluator call identity.
  #[must_use]
  pub const fn call_id(&self) -> MacroCallId {
    self.call_id
  }

  /// Returns the exact scoped Context Manifest identity.
  #[must_use]
  pub const fn context_manifest_id(&self) -> ContextManifestId {
    self.context_manifest_id
  }

  /// Returns the digest of the exact scoped Context Manifest.
  #[must_use]
  pub const fn context_digest(&self) -> FactoryDigest {
    self.context_digest
  }

  /// Reports whether an explicitly declared substitute produced the result.
  #[must_use]
  pub const fn is_substituted(&self) -> bool {
    self.substituted
  }
}

/// One expected evaluator branch and its authoritative progress.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvaluationBranch {
  key: FactoryKey,
  required: bool,
  state: EvaluationBranchState,
  #[serde(skip_serializing_if = "Option::is_none")]
  evaluator: Option<ImmutableReference>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  substitutes: Vec<ImmutableReference>,
  #[serde(skip_serializing_if = "Option::is_none")]
  result: Option<EvaluationBranchResult>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationBranchWire {
  key: FactoryKey,
  required: bool,
  state: EvaluationBranchState,
  #[serde(default)]
  evaluator: Option<ImmutableReference>,
  #[serde(default)]
  substitutes: Vec<ImmutableReference>,
  #[serde(default)]
  result: Option<EvaluationBranchResult>,
}

impl EvaluationBranch {
  /// Constructs one legacy-compatible branch projection without exact evaluator bindings.
  ///
  /// New plans should use [`EvaluationProgress::from_plan`] so accepted results remain bound to
  /// the evaluator and substitute set frozen by the immutable Evaluation Plan.
  #[must_use]
  pub const fn new(key: FactoryKey, required: bool, state: EvaluationBranchState) -> Self {
    Self {
      key,
      required,
      state,
      evaluator: None,
      substitutes: Vec::new(),
      result: None,
    }
  }

  fn from_review_branch(branch: &ReviewBranch) -> Self {
    Self {
      key: branch.key().clone(),
      required: branch.is_required(),
      state: EvaluationBranchState::Pending,
      evaluator: Some(branch.evaluator().clone()),
      substitutes: branch.substitutes().to_vec(),
      result: None,
    }
  }

  fn from_wire(mut wire: EvaluationBranchWire) -> Result<Self, FactoryError> {
    wire.substitutes.sort();
    if wire.substitutes.windows(2).any(|pair| pair[0] == pair[1])
      || wire
        .evaluator
        .as_ref()
        .is_some_and(|evaluator| wire.substitutes.contains(evaluator))
    {
      return Err(invalid_lifecycle());
    }
    let result_state = match wire.state {
      EvaluationBranchState::Completed => Some(false),
      EvaluationBranchState::Substituted => Some(true),
      _ => None,
    };
    match (result_state, &wire.result) {
      (Some(substituted), Some(result))
        if result.branch == wire.key
          && result.substituted == substituted
          && wire.evaluator.as_ref().is_some_and(|primary| {
            if substituted {
              &result.evaluator != primary && wire.substitutes.contains(&result.evaluator)
            } else {
              &result.evaluator == primary
            }
          }) => {}
      (None, None) => {}
      // Old snapshots did not retain evaluator/result bindings. They remain
      // readable, but only plan-bound branches can accept new typed results.
      (Some(_), None) if wire.evaluator.is_none() => {}
      _ => return Err(invalid_lifecycle()),
    }
    Ok(Self {
      key: wire.key,
      required: wire.required,
      state: wire.state,
      evaluator: wire.evaluator,
      substitutes: wire.substitutes,
      result: wire.result,
    })
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

  /// Reports whether deterministic policy requires this branch to complete.
  #[must_use]
  pub const fn is_required(&self) -> bool {
    self.required
  }

  /// Returns the immutable accepted result, when the branch completed.
  #[must_use]
  pub const fn result(&self) -> Option<&EvaluationBranchResult> {
    self.result.as_ref()
  }

  /// Reports whether the branch carries exact evaluator policy from a plan.
  #[must_use]
  pub const fn is_plan_bound(&self) -> bool {
    self.evaluator.is_some()
  }

  fn with_state(&self, state: EvaluationBranchState) -> Self {
    Self {
      key: self.key.clone(),
      required: self.required,
      state,
      evaluator: self.evaluator.clone(),
      substitutes: self.substitutes.clone(),
      result: self.result.clone(),
    }
  }

  fn with_result(&self, result: EvaluationBranchResult) -> Result<Self, FactoryError> {
    let Some(primary) = &self.evaluator else {
      return Err(invalid_lifecycle());
    };
    if result.branch != self.key || self.result.is_some() {
      return Err(invalid_lifecycle());
    }
    let state = if result.evaluator == *primary && !result.substituted {
      EvaluationBranchState::Completed
    } else if result.evaluator != *primary && result.substituted && self.substitutes.contains(&result.evaluator) {
      EvaluationBranchState::Substituted
    } else {
      return Err(invalid_lifecycle());
    };
    Ok(Self {
      key: self.key.clone(),
      required: self.required,
      state,
      evaluator: self.evaluator.clone(),
      substitutes: self.substitutes.clone(),
      result: Some(result),
    })
  }
}

impl<'de> Deserialize<'de> for EvaluationBranch {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    Self::from_wire(EvaluationBranchWire::deserialize(deserializer)?).map_err(D::Error::custom)
  }
}

/// Canonically ordered evaluator branches and their deterministic quorum.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvaluationProgress {
  branches: Vec<EvaluationBranch>,
  required_quorum: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationProgressWire {
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
      return Err(invalid_lifecycle());
    }
    branches.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(Self {
      branches,
      required_quorum,
    })
  }

  /// Creates the durable bounded fan-out from an immutable Evaluation Plan.
  pub fn from_plan(plan: &EvaluationPlan) -> Result<Self, FactoryError> {
    Self::try_new(
      plan
        .branches()
        .iter()
        .map(EvaluationBranch::from_review_branch)
        .collect(),
      plan.required_quorum(),
    )
  }

  /// Advances exactly one evaluator branch through an allowed immediate transition.
  pub fn advance_branch(&self, key: &FactoryKey, next_state: EvaluationBranchState) -> Result<Self, FactoryError> {
    let Some(current) = self.branches.iter().find(|branch| &branch.key == key) else {
      return Err(invalid_lifecycle());
    };
    if (current.is_plan_bound()
      && matches!(
        next_state,
        EvaluationBranchState::Completed | EvaluationBranchState::Substituted
      ))
      || !valid_evaluation_branch_transition(current.state, next_state)
    {
      return Err(invalid_lifecycle());
    }
    self.replace_branch(key, current.with_state(next_state))
  }

  /// Records one immutable schema-valid result after the Build has completed.
  pub fn record_result(&self, result: EvaluationBranchResult) -> Result<Self, FactoryError> {
    let Some(current) = self.branches.iter().find(|branch| branch.key == result.branch) else {
      return Err(invalid_lifecycle());
    };
    if current.state != EvaluationBranchState::ResultPending {
      return Err(invalid_lifecycle());
    }
    self.replace_branch(&current.key, current.with_result(result)?)
  }

  /// Marks a retryable branch terminal after its declared retry policy is exhausted.
  pub fn exhaust_branch(&self, key: &FactoryKey) -> Result<Self, FactoryError> {
    self.advance_branch(key, EvaluationBranchState::Exhausted)
  }

  fn replace_branch(&self, key: &FactoryKey, replacement: EvaluationBranch) -> Result<Self, FactoryError> {
    let branches = self
      .branches
      .iter()
      .map(|branch| {
        if &branch.key == key {
          replacement.clone()
        } else {
          branch.clone()
        }
      })
      .collect();
    Self::try_new(branches, self.required_quorum)
  }

  /// Returns evaluator branches in canonical key order.
  #[must_use]
  pub fn branches(&self) -> &[EvaluationBranch] {
    &self.branches
  }

  /// Returns the deterministic number of accepted branch results required by the join.
  #[must_use]
  pub const fn required_quorum(&self) -> u16 {
    self.required_quorum
  }

  /// Reports whether every branch was created from an exact immutable plan.
  #[must_use]
  pub fn is_plan_bound(&self) -> bool {
    self.branches.iter().all(EvaluationBranch::is_plan_bound)
  }

  /// Returns the exact Assessment identities accepted by a completed deterministic join.
  pub fn decision_assessment_ids(&self, plan: &EvaluationPlan) -> Result<Vec<AssessmentId>, FactoryError> {
    self.decision_bindings(plan).map(|(assessments, _)| assessments)
  }

  pub(crate) fn decision_bindings(
    &self,
    plan: &EvaluationPlan,
  ) -> Result<(Vec<AssessmentId>, HashSet<StageAttemptId>), FactoryError> {
    self.validate_plan_binding(plan, false)?;
    if self.branches.iter().any(|branch| {
      matches!(
        branch.state,
        EvaluationBranchState::Pending
          | EvaluationBranchState::AttemptCreated
          | EvaluationBranchState::BuildActive
          | EvaluationBranchState::ResultPending
          | EvaluationBranchState::RetryableFailure
          | EvaluationBranchState::RetryRequested
      )
    }) || self.branches.iter().any(|branch| {
      branch.required
        && !matches!(
          branch.state,
          EvaluationBranchState::Completed | EvaluationBranchState::Substituted
        )
    }) {
      return Err(invalid_lifecycle());
    }
    let assessment_ids = self
      .branches
      .iter()
      .filter_map(|branch| branch.result.as_ref().map(EvaluationBranchResult::assessment_id))
      .collect::<Vec<_>>();
    if assessment_ids.len() < usize::from(self.required_quorum)
      || assessment_ids.iter().copied().collect::<HashSet<_>>().len() != assessment_ids.len()
    {
      return Err(invalid_lifecycle());
    }
    let stage_attempt_ids = self
      .branches
      .iter()
      .filter_map(|branch| branch.result.as_ref().map(|result| result.stage_attempt_id))
      .collect::<HashSet<_>>();
    Ok((assessment_ids, stage_attempt_ids))
  }

  /// Verifies restored plan, call-context, and Assessment bindings.
  pub fn validate_bindings<'a, S, C, A>(
    &self,
    plan: &EvaluationPlan,
    stages: S,
    calls: C,
    assessments: A,
  ) -> Result<(), FactoryError>
  where
    S: Clone + IntoIterator<Item = &'a StageAttempt>,
    C: Clone + IntoIterator<Item = &'a MacroCall>,
    A: Clone + IntoIterator<Item = &'a Assessment>,
  {
    self.validate_plan_binding(plan, true)?;
    for result in self.branches.iter().filter_map(EvaluationBranch::result) {
      let call = calls
        .clone()
        .into_iter()
        .find(|call| call.id() == result.call_id)
        .ok_or_else(invalid_lifecycle)?;
      let stage = stages
        .clone()
        .into_iter()
        .find(|stage| stage.id() == result.stage_attempt_id)
        .ok_or_else(invalid_lifecycle)?;
      let assessment = assessments
        .clone()
        .into_iter()
        .find(|assessment| assessment.id() == result.assessment_id)
        .ok_or_else(invalid_lifecycle)?;
      if &EvaluationBranchResult::new(plan, stage, call, assessment)? != result {
        return Err(invalid_lifecycle());
      }
    }
    Ok(())
  }

  fn validate_plan_binding(&self, plan: &EvaluationPlan, require_terminal_results: bool) -> Result<(), FactoryError> {
    let planned = Self::from_plan(plan)?;
    let invalid = self.branches.len() != planned.branches.len()
      || self.required_quorum != planned.required_quorum
      || self.branches.iter().zip(&planned.branches).any(|(actual, expected)| {
        actual.key != expected.key
          || actual.required != expected.required
          || actual.evaluator != expected.evaluator
          || actual.substitutes != expected.substitutes
          || (require_terminal_results
            && matches!(
              actual.state,
              EvaluationBranchState::Completed | EvaluationBranchState::Substituted
            )
            && actual.result.is_none())
      });
    if invalid { Err(invalid_lifecycle()) } else { Ok(()) }
  }

  pub(crate) fn is_immediate_successor(&self, next: &Self) -> bool {
    if self.required_quorum != next.required_quorum || self.branches.len() != next.branches.len() {
      return false;
    }
    let mut changed = 0_u8;
    self.branches.iter().zip(&next.branches).all(|(previous, next)| {
      if previous.key != next.key
        || previous.required != next.required
        || previous.evaluator != next.evaluator
        || previous.substitutes != next.substitutes
      {
        return false;
      }
      if previous == next {
        return true;
      }
      changed = changed.saturating_add(1);
      changed == 1
        && valid_evaluation_branch_transition(previous.state, next.state)
        && match next.state {
          EvaluationBranchState::Completed | EvaluationBranchState::Substituted if next.is_plan_bound() => {
            previous.result.is_none() && next.result.is_some()
          }
          _ => previous.result == next.result,
        }
    }) && changed == 1
  }
}

impl<'de> Deserialize<'de> for EvaluationProgress {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = EvaluationProgressWire::deserialize(deserializer)?;
    Self::try_new(wire.branches, wire.required_quorum).map_err(D::Error::custom)
  }
}

pub(crate) const fn valid_evaluation_branch_transition(
  previous: EvaluationBranchState,
  next: EvaluationBranchState,
) -> bool {
  matches!(
    (previous, next),
    (
      EvaluationBranchState::Pending | EvaluationBranchState::RetryRequested,
      EvaluationBranchState::AttemptCreated
    ) | (
      EvaluationBranchState::RetryableFailure,
      EvaluationBranchState::RetryRequested | EvaluationBranchState::Exhausted
    ) | (
      EvaluationBranchState::AttemptCreated,
      EvaluationBranchState::BuildActive
    ) | (
      EvaluationBranchState::BuildActive,
      EvaluationBranchState::ResultPending
        | EvaluationBranchState::Completed
        | EvaluationBranchState::RetryableFailure
        | EvaluationBranchState::Failed
        | EvaluationBranchState::Missing
        | EvaluationBranchState::Cancelled
    ) | (
      EvaluationBranchState::ResultPending,
      EvaluationBranchState::Completed | EvaluationBranchState::Substituted
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
    /// Exact immutable Decision that selected the outcome.
    decision_id: crate::DecisionId,
    /// Typed deterministic outcome.
    outcome: DecisionOutcome,
  },
}

fn invalid_lifecycle() -> FactoryError {
  FactoryError::InvalidLifecycle {
    state: FactoryRunState::Evaluating,
  }
}
