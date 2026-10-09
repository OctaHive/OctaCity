//! Immutable schema-valid evaluator results and their exact bindings.

mod finding;
mod provenance;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

pub use finding::{AssessmentEvidenceReference, AssessmentFinding, AssessmentFindingKind};
pub use provenance::AssessmentProvenance;

use crate::{
  AssessmentId, AssessmentOutcome, BoundedSummary, CandidateSubject, EvaluationPlan, EvaluationPlanId,
  EvidenceManifest, FactoryArtifactReference, FactoryDigest, FactoryError, FactoryKey, FactoryTaskSubject,
  ImmutableReference,
};

/// Maximum findings returned by one Assessment.
pub const MAX_ASSESSMENT_FINDINGS: usize = 128;
/// Maximum exact evidence references attached to one Assessment finding.
pub const MAX_ASSESSMENT_EVIDENCE_REFERENCES: usize = 32;

/// Named trusted inputs used to construct one immutable Assessment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssessmentInput {
  /// Exact candidate reviewed by the evaluator.
  pub subject: CandidateSubject,
  /// Exact evaluator selected by the plan.
  pub evaluator: ImmutableReference,
  /// Schema-valid assessment outcome.
  pub outcome: AssessmentOutcome,
  /// Bounded result summary bound to the exact candidate.
  pub summary: BoundedSummary,
  /// Typed findings with resolved evidence and server-derived fingerprints.
  pub findings: Vec<AssessmentFinding>,
  /// Exact selected model.
  pub model: ImmutableReference,
  /// Digest of the exact rendered prompt.
  pub prompt_digest: FactoryDigest,
  /// Exact typed result Artifact.
  pub result: FactoryArtifactReference,
  /// Exact retained provenance record Artifact.
  pub provenance: FactoryArtifactReference,
}

/// Immutable schema-valid evaluator result for one exact Evaluation Plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Assessment {
  id: AssessmentId,
  plan_id: EvaluationPlanId,
  subject: CandidateSubject,
  outcome: AssessmentOutcome,
  summary: BoundedSummary,
  findings: Vec<AssessmentFinding>,
  provenance: AssessmentProvenance,
}

impl Assessment {
  /// Constructs an Assessment from an evaluator declared by the plan.
  pub fn new(id: AssessmentId, plan: &EvaluationPlan, input: AssessmentInput) -> Result<Self, FactoryError> {
    if &input.subject != plan.subject()
      || input.summary.subject() != &FactoryTaskSubject::Candidate(input.subject.clone())
    {
      return Err(FactoryError::InconsistentSubject);
    }
    validate_findings(input.outcome, &input.findings)?;
    let provenance = AssessmentProvenance::new(
      plan,
      input.evaluator,
      input.model,
      input.prompt_digest,
      input.result,
      input.provenance,
    )?;
    Ok(Self {
      id,
      plan_id: plan.id(),
      subject: input.subject,
      outcome: input.outcome,
      summary: input.summary,
      findings: input.findings,
      provenance,
    })
  }

  fn from_wire(wire: AssessmentWire) -> Result<Self, FactoryError> {
    if wire.summary.subject() != &FactoryTaskSubject::Candidate(wire.subject.clone()) {
      return Err(FactoryError::InconsistentSubject);
    }
    validate_findings(wire.outcome, &wire.findings)?;
    Ok(Self {
      id: wire.id,
      plan_id: wire.plan_id,
      subject: wire.subject,
      outcome: wire.outcome,
      summary: wire.summary,
      findings: wire.findings,
      provenance: wire.provenance,
    })
  }

  /// Verifies restored cross-record bindings against the authoritative plan and evidence.
  pub fn validate_bindings(&self, plan: &EvaluationPlan, evidence: &EvidenceManifest) -> Result<(), FactoryError> {
    if self.plan_id != plan.id()
      || &self.subject != plan.subject()
      || self.provenance.plan_digest() != plan.digest()?
      || self.provenance.evidence_id() != evidence.id()
      || plan.evidence_id() != evidence.id()
      || evidence.subject() != &self.subject
      || !plan.has_evaluator(self.provenance.evaluator())
    {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment bindings",
      });
    }
    let expected_packs = plan
      .criterion_packs()
      .iter()
      .map(|pack| pack.reference())
      .collect::<Vec<_>>();
    if expected_packs != self.provenance.criterion_packs().iter().collect::<Vec<_>>()
      || self
        .findings
        .iter()
        .flat_map(AssessmentFinding::evidence)
        .any(|reference| {
          evidence
            .item(reference.kind())
            .is_none_or(|item| item.artifact() != reference.artifact())
        })
    {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment bindings",
      });
    }
    Ok(())
  }

  /// Returns the Assessment identity.
  #[must_use]
  pub const fn id(&self) -> AssessmentId {
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

  /// Returns the selected evaluator logical identity.
  #[must_use]
  pub const fn evaluator(&self) -> &FactoryKey {
    self.provenance.evaluator().identity()
  }

  /// Returns the exact selected evaluator identity, version, and digest.
  #[must_use]
  pub const fn evaluator_reference(&self) -> &ImmutableReference {
    self.provenance.evaluator()
  }

  /// Returns the schema-valid outcome.
  #[must_use]
  pub const fn outcome(&self) -> AssessmentOutcome {
    self.outcome
  }

  /// Returns the bounded candidate-bound assessment summary.
  #[must_use]
  pub const fn summary(&self) -> &BoundedSummary {
    &self.summary
  }

  /// Returns bounded typed findings.
  #[must_use]
  pub fn findings(&self) -> &[AssessmentFinding] {
    &self.findings
  }

  /// Returns exact evaluator, model, prompt, criteria, evidence, result, and record provenance.
  #[must_use]
  pub const fn provenance(&self) -> &AssessmentProvenance {
    &self.provenance
  }

  /// Computes the stable content digest of the complete immutable Assessment.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let encoded = serde_json::to_vec(self).map_err(|_| FactoryError::InvalidReference {
      relationship: "assessment serialization",
    })?;
    Ok(FactoryDigest::sha256("octacity.factory.assessment.v2", &[&encoded]))
  }
}

fn validate_findings(outcome: AssessmentOutcome, findings: &[AssessmentFinding]) -> Result<(), FactoryError> {
  if findings.len() > MAX_ASSESSMENT_FINDINGS {
    return Err(FactoryError::CollectionLimitExceeded {
      collection: "assessment findings",
    });
  }
  let matches_outcome = match outcome {
    AssessmentOutcome::Satisfied => findings.is_empty(),
    AssessmentOutcome::Violated => !findings.is_empty() && findings.iter().all(|finding| finding.severity().is_some()),
    AssessmentOutcome::Indeterminate => {
      !findings.is_empty() && findings.iter().all(|finding| finding.severity().is_none())
    }
  };
  if matches_outcome {
    Ok(())
  } else {
    Err(FactoryError::InvalidReference {
      relationship: "assessment outcome findings",
    })
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentWire {
  id: AssessmentId,
  plan_id: EvaluationPlanId,
  subject: CandidateSubject,
  outcome: AssessmentOutcome,
  summary: BoundedSummary,
  findings: Vec<AssessmentFinding>,
  provenance: AssessmentProvenance,
}

impl<'de> Deserialize<'de> for Assessment {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    Self::from_wire(AssessmentWire::deserialize(deserializer)?).map_err(D::Error::custom)
  }
}
