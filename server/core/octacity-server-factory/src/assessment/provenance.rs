//! Exact evaluator and input provenance for immutable Assessments.

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
  EvaluationPlan, EvidenceManifestId, FactoryArtifactReference, FactoryDigest, FactoryError, ImmutableReference,
  MAX_CRITERION_PACKS,
};

/// Exact provenance required to reproduce and audit one Assessment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AssessmentProvenance {
  plan_digest: FactoryDigest,
  evidence_id: EvidenceManifestId,
  criterion_packs: Vec<ImmutableReference>,
  evaluator: ImmutableReference,
  model: ImmutableReference,
  prompt_digest: FactoryDigest,
  result: FactoryArtifactReference,
  record: FactoryArtifactReference,
}

impl AssessmentProvenance {
  pub(super) fn new(
    plan: &EvaluationPlan,
    evaluator: ImmutableReference,
    model: ImmutableReference,
    prompt_digest: FactoryDigest,
    result: FactoryArtifactReference,
    record: FactoryArtifactReference,
  ) -> Result<Self, FactoryError> {
    if !plan.has_evaluator(&evaluator) || result.artifact_id() == record.artifact_id() {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment provenance",
      });
    }
    let criterion_packs = plan
      .criterion_packs()
      .iter()
      .map(|pack| pack.reference().clone())
      .collect();
    Self::from_parts(AssessmentProvenanceParts {
      plan_digest: plan.digest()?,
      evidence_id: plan.evidence_id(),
      criterion_packs,
      evaluator,
      model,
      prompt_digest,
      result,
      record,
    })
  }

  fn from_parts(mut parts: AssessmentProvenanceParts) -> Result<Self, FactoryError> {
    if parts.criterion_packs.is_empty()
      || parts.criterion_packs.len() > MAX_CRITERION_PACKS
      || parts.result.artifact_id() == parts.record.artifact_id()
    {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment provenance",
      });
    }
    parts.criterion_packs.sort();
    if parts.criterion_packs.windows(2).any(|pair| pair[0] >= pair[1]) {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment criterion packs",
      });
    }
    Ok(Self {
      plan_digest: parts.plan_digest,
      evidence_id: parts.evidence_id,
      criterion_packs: parts.criterion_packs,
      evaluator: parts.evaluator,
      model: parts.model,
      prompt_digest: parts.prompt_digest,
      result: parts.result,
      record: parts.record,
    })
  }

  /// Returns the digest of the exact frozen Evaluation Plan.
  #[must_use]
  pub const fn plan_digest(&self) -> FactoryDigest {
    self.plan_digest
  }

  /// Returns the exact criterion-pack identities selected by the plan.
  #[must_use]
  pub fn criterion_packs(&self) -> &[ImmutableReference] {
    &self.criterion_packs
  }

  /// Returns the exact selected evaluator identity and digest.
  #[must_use]
  pub const fn evaluator(&self) -> &ImmutableReference {
    &self.evaluator
  }

  /// Returns the exact selected model identity and digest.
  #[must_use]
  pub const fn model(&self) -> &ImmutableReference {
    &self.model
  }

  /// Returns the exact Evidence Manifest identity.
  #[must_use]
  pub const fn evidence_id(&self) -> EvidenceManifestId {
    self.evidence_id
  }

  /// Returns the digest of the exact rendered evaluator prompt.
  #[must_use]
  pub const fn prompt_digest(&self) -> FactoryDigest {
    self.prompt_digest
  }

  /// Returns the exact typed evaluator result Artifact.
  #[must_use]
  pub const fn result(&self) -> &FactoryArtifactReference {
    &self.result
  }

  /// Returns the exact retained evaluator provenance record.
  #[must_use]
  pub const fn record(&self) -> &FactoryArtifactReference {
    &self.record
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentProvenanceWire {
  plan_digest: FactoryDigest,
  evidence_id: EvidenceManifestId,
  criterion_packs: Vec<ImmutableReference>,
  evaluator: ImmutableReference,
  model: ImmutableReference,
  prompt_digest: FactoryDigest,
  result: FactoryArtifactReference,
  record: FactoryArtifactReference,
}

struct AssessmentProvenanceParts {
  plan_digest: FactoryDigest,
  evidence_id: EvidenceManifestId,
  criterion_packs: Vec<ImmutableReference>,
  evaluator: ImmutableReference,
  model: ImmutableReference,
  prompt_digest: FactoryDigest,
  result: FactoryArtifactReference,
  record: FactoryArtifactReference,
}

impl<'de> Deserialize<'de> for AssessmentProvenance {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = AssessmentProvenanceWire::deserialize(deserializer)?;
    Self::from_parts(AssessmentProvenanceParts {
      plan_digest: wire.plan_digest,
      evidence_id: wire.evidence_id,
      criterion_packs: wire.criterion_packs,
      evaluator: wire.evaluator,
      model: wire.model,
      prompt_digest: wire.prompt_digest,
      result: wire.result,
      record: wire.record,
    })
    .map_err(D::Error::custom)
  }
}
