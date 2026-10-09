//! Immutable deterministic candidate Decisions.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
  Assessment, AssessmentId, CandidateSubject, DecisionId, DecisionOutcome, DecisionSignalReceipt,
  DecisionSignalReceiptId, EvaluationPlan, EvaluationPlanId, FactoryDigest, FactoryError, FactoryKey,
  FactoryStageTarget, FindingSeverity, MAX_CRITERION_PACKS, MAX_DECISION_SIGNAL_RECEIPTS, MAX_EVALUATORS, StageAttempt,
};

/// Maximum Assessments consumed by one Decision.
pub const MAX_DECISION_ASSESSMENTS: usize = 64;
/// Maximum bounded reasons recorded by one Decision.
pub const MAX_DECISION_REASONS: usize = MAX_CRITERION_PACKS + MAX_EVALUATORS + 1;

/// Typed deterministic reason recorded by the Decision Engine.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DecisionReason {
  /// Every mandatory gate and evaluator policy was satisfied.
  PolicySatisfied,
  /// Required deterministic evidence was not supplied.
  RequiredEvidenceMissing(FactoryKey),
  /// Required deterministic evidence recorded failure.
  RequiredEvidenceFailed(FactoryKey),
  /// Required deterministic evidence could not produce a result.
  RequiredEvidenceIndeterminate(FactoryKey),
  /// A required evaluator Assessment is absent.
  RequiredAssessmentMissing(FactoryKey),
  /// An evaluator Assessment is indeterminate under the immutable policy.
  AssessmentIndeterminate(FactoryKey),
  /// A typed violation met or exceeded the immutable severity threshold.
  SeverityThresholdExceeded {
    /// Evaluator whose immutable Assessment contained the violation.
    evaluator: FactoryKey,
    /// Highest policy-relevant severity reported by that evaluator.
    severity: FindingSeverity,
  },
  /// Too few distinct declared evaluator Assessments reached the join.
  QuorumNotMet {
    /// Immutable required number of Assessments.
    required: u16,
    /// Number of distinct valid Assessments supplied.
    observed: u16,
  },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DecisionBindings {
  pub(crate) input_digest: FactoryDigest,
  pub(crate) policy_version: crate::DecisionPolicyVersion,
  pub(crate) policy_digest: FactoryDigest,
}

/// Immutable deterministic candidate disposition input to later lifecycle policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Decision {
  id: DecisionId,
  plan_id: EvaluationPlanId,
  subject: CandidateSubject,
  outcome: DecisionOutcome,
  assessment_ids: Vec<AssessmentId>,
  #[serde(default)]
  signal_receipt_ids: Vec<DecisionSignalReceiptId>,
  reasons: Vec<DecisionReason>,
  input_digest: FactoryDigest,
  policy_version: crate::DecisionPolicyVersion,
  policy_digest: FactoryDigest,
}

impl Decision {
  pub(crate) fn from_engine(
    id: DecisionId,
    plan: &EvaluationPlan,
    outcome: DecisionOutcome,
    assessments: &[Assessment],
    signal_receipts: &[DecisionSignalReceipt],
    reasons: Vec<DecisionReason>,
    bindings: DecisionBindings,
  ) -> Result<Self, FactoryError> {
    if assessments.len() > MAX_DECISION_ASSESSMENTS {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "decision assessments",
      });
    }
    if signal_receipts.len() > MAX_DECISION_SIGNAL_RECEIPTS {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "decision signal receipts",
      });
    }
    validate_count(&reasons, 1, MAX_DECISION_REASONS, "decision reasons")?;
    if assessments
      .iter()
      .any(|assessment| assessment.plan_id() != plan.id() || assessment.subject() != plan.subject())
    {
      return Err(FactoryError::InconsistentSubject);
    }
    let assessment_ids = assessments.iter().map(Assessment::id).collect::<Vec<_>>();
    if assessment_ids.iter().copied().collect::<HashSet<_>>().len() != assessment_ids.len() {
      return Err(FactoryError::InvalidReference {
        relationship: "decision assessment",
      });
    }
    let signal_receipt_ids = signal_receipts
      .iter()
      .map(DecisionSignalReceipt::id)
      .collect::<Vec<_>>();
    if signal_receipt_ids.iter().copied().collect::<HashSet<_>>().len() != signal_receipt_ids.len() {
      return Err(FactoryError::InvalidReference {
        relationship: "decision signal receipt",
      });
    }
    Ok(Self {
      id,
      plan_id: plan.id(),
      subject: plan.subject().clone(),
      outcome,
      assessment_ids,
      signal_receipt_ids,
      reasons,
      input_digest: bindings.input_digest,
      policy_version: bindings.policy_version,
      policy_digest: bindings.policy_digest,
    })
  }

  /// Returns the Decision identity.
  #[must_use]
  pub const fn id(&self) -> DecisionId {
    self.id
  }

  /// Returns the Evaluation Plan identity.
  #[must_use]
  pub const fn plan_id(&self) -> EvaluationPlanId {
    self.plan_id
  }

  /// Returns the exact candidate subject.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the deterministic outcome.
  #[must_use]
  pub const fn outcome(&self) -> DecisionOutcome {
    self.outcome
  }

  /// Returns the consumed Assessment identities.
  #[must_use]
  pub fn assessment_ids(&self) -> &[AssessmentId] {
    &self.assessment_ids
  }

  /// Returns non-authoritative Decision Signal receipts included in the canonical input digest.
  #[must_use]
  pub fn signal_receipt_ids(&self) -> &[DecisionSignalReceiptId] {
    &self.signal_receipt_ids
  }

  /// Returns the bounded decision reasons.
  #[must_use]
  pub fn reasons(&self) -> &[DecisionReason] {
    &self.reasons
  }

  /// Returns the canonical deterministic input digest.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }

  /// Returns the immutable Decision policy version.
  #[must_use]
  pub const fn policy_version(&self) -> crate::DecisionPolicyVersion {
    self.policy_version
  }

  /// Returns the exact immutable Decision policy digest.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }

  /// Revalidates every immutable record consumed by this Decision.
  ///
  /// Persistence adapters deliberately delegate these domain bindings to the
  /// Factory core so restored state and newly appended state follow identical
  /// rules.
  pub fn validate_bindings<'a, S, A, R>(
    &self,
    plan: &EvaluationPlan,
    stages: S,
    assessments: A,
    signal_receipts: R,
  ) -> Result<(), FactoryError>
  where
    S: IntoIterator<Item = &'a StageAttempt>,
    A: IntoIterator<Item = &'a Assessment>,
    R: IntoIterator<Item = &'a DecisionSignalReceipt>,
  {
    if self.plan_id != plan.id() || &self.subject != plan.subject() {
      return Err(FactoryError::InconsistentSubject);
    }
    let assessment_ids_are_unique =
      self.assessment_ids.iter().copied().collect::<HashSet<_>>().len() == self.assessment_ids.len();
    let receipt_ids_are_unique =
      self.signal_receipt_ids.iter().copied().collect::<HashSet<_>>().len() == self.signal_receipt_ids.len();
    let assessments = assessments
      .into_iter()
      .map(|assessment| (assessment.id(), assessment))
      .collect::<std::collections::HashMap<_, _>>();
    let receipts = signal_receipts
      .into_iter()
      .map(|receipt| (receipt.id(), receipt))
      .collect::<std::collections::HashMap<_, _>>();
    let stages = stages
      .into_iter()
      .map(|stage| (stage.id(), stage))
      .collect::<std::collections::HashMap<_, _>>();
    let assessments_valid = self.assessment_ids.iter().all(|id| {
      assessments.get(id).is_some_and(|assessment| {
        assessment.plan_id() == self.plan_id
          && assessment.subject() == &self.subject
          && plan
            .branches()
            .iter()
            .any(|branch| branch.allows_evaluator(assessment.evaluator_reference()))
      })
    });
    let receipts_valid = self.signal_receipt_ids.iter().all(|id| {
      receipts.get(id).is_some_and(|receipt| {
        let request = receipt.request().request();
        let stage_matches = stages.get(&request.stage_attempt_id()).is_some_and(|stage| {
          stage.run_id() == receipt.run_id()
            && stage.subject() == receipt.subject()
            && matches!(stage.target(), FactoryStageTarget::Evaluation(key) if plan.branches().iter().any(|branch| branch.key() == key))
        });
        receipt.subject() == self.subject.exact()
          && receipt.request().request().purpose() == crate::DecisionSignalPurpose::Routing
          && receipt.validate_integrity().is_ok()
          && stage_matches
      })
    });
    if assessment_ids_are_unique && receipt_ids_are_unique && assessments_valid && receipts_valid {
      Ok(())
    } else {
      Err(FactoryError::InvalidReference {
        relationship: "decision inputs",
      })
    }
  }
}

fn validate_count<T>(
  values: &[T],
  minimum: usize,
  maximum: usize,
  collection: &'static str,
) -> Result<(), FactoryError> {
  if values.len() < minimum {
    return Err(FactoryError::InvalidReference {
      relationship: collection,
    });
  }
  if values.len() > maximum {
    return Err(FactoryError::CollectionLimitExceeded { collection });
  }
  Ok(())
}
