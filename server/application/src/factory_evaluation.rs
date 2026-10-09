//! Pure application projection from a completed evaluation join to durable state.

use octacity_server_factory::{
  Assessment, Decision, DecisionEngineInput, DecisionId, DecisionPolicy, DecisionSignalReceipt, EvaluationPlan,
  EvaluationProgress, EvaluationState, EvidenceManifest, FactoryError, FactoryLifecycleProgress, evaluate_decision,
};
use octacity_server_store::FactoryRunHistoryAppend;

/// Decision and lifecycle records produced from one completed immutable join.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryEvaluationDecision {
  decision: Decision,
  progress: FactoryLifecycleProgress,
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
    let progress = FactoryLifecycleProgress::Evaluating(EvaluationState::DecisionRecorded {
      decision_id: decision.id(),
      outcome: decision.outcome(),
    });
    Ok(Self { decision, progress })
  }

  /// Returns the authoritative deterministic Decision.
  #[must_use]
  pub const fn decision(&self) -> &Decision {
    &self.decision
  }

  /// Returns the corresponding code-owned lifecycle checkpoint.
  #[must_use]
  pub const fn progress(&self) -> &FactoryLifecycleProgress {
    &self.progress
  }

  /// Produces the append-only records and checkpoint for one fenced transition.
  #[must_use]
  pub fn into_parts(self) -> (FactoryLifecycleProgress, FactoryRunHistoryAppend) {
    (
      self.progress,
      FactoryRunHistoryAppend {
        decisions: vec![self.decision],
        ..FactoryRunHistoryAppend::default()
      },
    )
  }
}
