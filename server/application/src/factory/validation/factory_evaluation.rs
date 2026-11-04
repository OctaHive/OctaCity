//! Pure evaluation result; graph progress remains owned by the configured Flow.

use octacity_server_factory::{
  Assessment, Decision, DecisionEngineInput, DecisionId, DecisionPolicy, DecisionSignalReceipt, EvaluationPlan,
  EvaluationProgress, EvaluationState, EvidenceManifest, FactoryError, evaluate_decision,
};

/// Decision and operation-local progress produced from one completed immutable join.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryEvaluationDecision {
  decision: Decision,
  progress: EvaluationState,
}

impl FactoryEvaluationDecision {
  /// Evaluates only the Assessments selected by a completed branch join.
  pub fn from_completed_join(
    id: DecisionId,
    evidence: &EvidenceManifest,
    plan: &EvaluationPlan,
    policy: &DecisionPolicy,
    progress: &EvaluationProgress,
    assessments: &[Assessment],
    signal_receipts: &[DecisionSignalReceipt],
  ) -> Result<Self, FactoryError> {
    let decision = evaluate_decision(
      id,
      DecisionEngineInput::from_completed_evaluation(evidence, plan, policy, progress, assessments, signal_receipts)?,
    )?;
    let progress = EvaluationState::DecisionRecorded {
      decision_id: decision.id(),
      outcome: decision.outcome(),
    };
    Ok(Self { decision, progress })
  }

  /// Returns the authoritative deterministic Decision.
  #[must_use]
  pub const fn decision(&self) -> &Decision {
    &self.decision
  }

  /// Returns local evaluation progress without prescribing a graph transition.
  #[must_use]
  pub const fn progress(&self) -> &EvaluationState {
    &self.progress
  }

  /// Returns operation-local progress and its verified Decision as capability data.
  #[must_use]
  pub fn into_parts(self) -> (EvaluationState, Decision) {
    (self.progress, self.decision)
  }
}
