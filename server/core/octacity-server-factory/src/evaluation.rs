//! Immutable deterministic candidate Decisions.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
  Assessment, AssessmentId, CandidateSubject, DecisionId, DecisionOutcome, EvaluationPlan, EvaluationPlanId,
  FactoryDigest, FactoryError, FactoryKey, FindingSeverity, MAX_CRITERION_PACKS, MAX_EVALUATORS,
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
    reasons: Vec<DecisionReason>,
    bindings: DecisionBindings,
  ) -> Result<Self, FactoryError> {
    if assessments.len() > MAX_DECISION_ASSESSMENTS {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "decision assessments",
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
    Ok(Self {
      id,
      plan_id: plan.id(),
      subject: plan.subject().clone(),
      outcome,
      assessment_ids,
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
