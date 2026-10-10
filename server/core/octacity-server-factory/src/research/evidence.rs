use super::{DefectResearchOutcome, ResearchDetails, ResearchInput, ResearchOutcome, ResearchResult, digest, invalid};
use crate::{
  DeterministicGateOutcome, EvidenceOutputKind, EvidenceProducer, ExactSubject, FactoryArtifactReference,
  FactoryDigest, FactoryError, ImmutableReference,
};
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

/// Required trusted evidence category, independent of model recommendations.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchEvidenceKind {
  /// Deterministically parsed reproduction report for the frozen environment.
  Reproduction,
  /// Deterministically validated bounded proposal and complete source references.
  Proposal,
  /// Evidence permitting verification without implementation.
  Verification,
  /// Evidence permitting an existing-behavior terminal resolution.
  TerminalResolution,
}

/// One deterministic fact accepted independently of research prose.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "outcome", deny_unknown_fields)]
pub enum ResearchEvidenceFact {
  /// Exact reproduction classification parsed from the verified report.
  Reproduction(DefectResearchOutcome),
  /// Proposal schema, sources, size, and retained bytes passed validation.
  Proposal,
  /// Existing behavior may enter the verification-only path.
  Verification,
  /// Existing behavior satisfies the exact accepted Work.
  TerminalResolution,
}

impl ResearchEvidenceFact {
  /// Returns the evidence requirement consumed by this fact.
  #[must_use]
  pub const fn kind(self) -> ResearchEvidenceKind {
    match self {
      Self::Reproduction(_) => ResearchEvidenceKind::Reproduction,
      Self::Proposal => ResearchEvidenceKind::Proposal,
      Self::Verification => ResearchEvidenceKind::Verification,
      Self::TerminalResolution => ResearchEvidenceKind::TerminalResolution,
    }
  }
}

/// Metadata supplied by trusted validation of retained report or Artifact bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResearchEvidenceRecord {
  /// Exact Project, Repository, and base revision verified.
  pub subject: ExactSubject,
  /// Complete frozen research input verified.
  pub input_digest: FactoryDigest,
  /// Frozen defect environment digest, absent for feature research.
  pub environment_digest: Option<FactoryDigest>,
  /// Deterministically established fact.
  pub fact: ResearchEvidenceFact,
  /// Verified retained bytes supporting the fact.
  pub artifact: FactoryArtifactReference,
  /// Exact parsed report or Artifact schema.
  pub schema: ImmutableReference,
  /// Verified logical output category.
  pub output_kind: EvidenceOutputKind,
  /// Exact deterministic validation Build, Job, tool, and plugin.
  pub producer: EvidenceProducer,
  /// Time the trusted gate accepted the fact.
  pub verified_at: Timestamp,
  /// Exclusive immutable freshness deadline.
  pub fresh_until: Timestamp,
}

/// Trusted evidence proof; model/provider observations cannot deserialize this type.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AcceptedResearchEvidence {
  pub(super) record: ResearchEvidenceRecord,
  pub(super) result_digest: FactoryDigest,
}

impl AcceptedResearchEvidence {
  /// Constructs a proof only after a trusted gate validates exact retained bytes.
  pub fn new(
    input: &ResearchInput,
    result: &ResearchResult,
    record: ResearchEvidenceRecord,
    gate: DeterministicGateOutcome,
  ) -> Result<Self, FactoryError> {
    result.validate_input(input)?;
    let environment = match input.details() {
      ResearchDetails::Defect { environment, .. } => Some(environment.content_digest()),
      ResearchDetails::Feature { .. } => None,
    };
    if gate != DeterministicGateOutcome::Passed
      || record.subject != *input.subject()
      || record.input_digest != input.digest()?
      || record.environment_digest != environment
      || record.verified_at < result.provenance().observed_at
      || record.verified_at >= record.fresh_until
    {
      return Err(invalid("research evidence binding or gate"));
    }
    match record.fact {
      ResearchEvidenceFact::Reproduction(outcome) => {
        if result.outcome() != ResearchOutcome::Defect(outcome) || &record.artifact != result.evidence_artifact() {
          return Err(invalid("research reproduction fact"));
        }
      }
      ResearchEvidenceFact::Proposal => {
        if result.outcome() != ResearchOutcome::FeatureProposal || &record.artifact != result.evidence_artifact() {
          return Err(invalid("research proposal fact"));
        }
      }
      ResearchEvidenceFact::Verification | ResearchEvidenceFact::TerminalResolution => {}
    }
    Ok(Self {
      record,
      result_digest: result.digest()?,
    })
  }
  /// Returns the exact independently validated fact and producer metadata.
  #[must_use]
  pub const fn record(&self) -> &ResearchEvidenceRecord {
    &self.record
  }
  /// Returns the proof's canonical identity.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.research-evidence.v1", self)
  }
}
