use super::{
  AcceptedResearchEvidence, RESEARCH_CONTRACT_VERSION, ResearchDecision, ResearchInput, ResearchPolicy, ResearchResult,
  digest,
};
use crate::{
  BudgetUsage, ContextManifestId, ExactSubject, FactoryArtifactReference, FactoryDigest, FactoryError, StageHandoffId,
  WorkEnvelopeId,
};
use octacity_server_domain::Timestamp;
use serde::Serialize;

/// Immutable research Stage Handoff produced by a generic Node Attempt.
///
/// The typed result retains its bounded summary, sources, assumptions,
/// alternatives, unresolved items, and producer provenance. No candidate,
/// permission grant, successor route, or implementation-ready state is carried.
/// Research uses the same Stage Handoff identity without creating a fixed Stage
/// Attempt projection for an ordinary nested Flow node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResearchStageHandoff {
  schema_version: u16,
  id: StageHandoffId,
  work_id: WorkEnvelopeId,
  subject: ExactSubject,
  context_id: ContextManifestId,
  context_digest: FactoryDigest,
  input_digest: FactoryDigest,
  policy_digest: FactoryDigest,
  evidence_digest: FactoryDigest,
  result: ResearchResult,
  artifacts: Vec<FactoryArtifactReference>,
}

/// Separately retained non-authoritative handoff and code-owned successor decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResearchAcceptance {
  /// Immutable bounded facts passed only to explicitly declared successors.
  pub handoff: ResearchStageHandoff,
  /// Deterministic declared outcome; execution still requires the current fence and gates.
  pub decision: ResearchDecision,
}

impl ResearchPolicy {
  /// Freezes a bounded non-authoritative handoff alongside its deterministic disposition.
  pub fn accept(
    &self,
    id: StageHandoffId,
    input: &ResearchInput,
    result: &ResearchResult,
    evidence: &[AcceptedResearchEvidence],
    usage: BudgetUsage,
    at: Timestamp,
  ) -> Result<ResearchAcceptance, FactoryError> {
    let decision = self.evaluate(input, result, evidence, usage, at)?;
    let mut artifacts = vec![result.provenance().result.clone(), result.evidence_artifact().clone()];
    artifacts.extend(evidence.iter().map(|row| row.record().artifact.clone()));
    artifacts.sort();
    artifacts.dedup();
    let handoff = ResearchStageHandoff {
      schema_version: RESEARCH_CONTRACT_VERSION,
      id,
      work_id: input.work_id(),
      subject: input.subject().clone(),
      context_id: input.context().id(),
      context_digest: input.context().digest()?,
      input_digest: decision.input_digest,
      policy_digest: decision.policy_digest,
      evidence_digest: decision.evidence_digest,
      result: result.clone(),
      artifacts,
    };
    if serde_json::to_vec(&handoff)
      .map_err(|_| super::invalid("research handoff serialization"))?
      .len()
      > super::MAX_RESEARCH_CONTRACT_BYTES
    {
      return Err(super::invalid("research handoff bytes"));
    }
    Ok(ResearchAcceptance { handoff, decision })
  }
}

impl ResearchStageHandoff {
  /// Returns the stable append-only Stage Handoff identity.
  #[must_use]
  pub const fn id(&self) -> StageHandoffId {
    self.id
  }
  /// Returns the exact admitted Work.
  #[must_use]
  pub const fn work_id(&self) -> WorkEnvelopeId {
    self.work_id
  }
  /// Returns the exact Project, Repository, and original base revision.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }
  /// Returns the frozen context identity.
  #[must_use]
  pub const fn context_id(&self) -> ContextManifestId {
    self.context_id
  }
  /// Returns the frozen complete context digest.
  #[must_use]
  pub const fn context_digest(&self) -> FactoryDigest {
    self.context_digest
  }
  /// Returns the complete input digest, including accepted eligibility and classification.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }
  /// Returns the exact bound research policy.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }
  /// Returns canonical trusted evidence provenance.
  #[must_use]
  pub const fn evidence_digest(&self) -> FactoryDigest {
    self.evidence_digest
  }
  /// Returns the complete bounded observations and producing Node Attempt provenance.
  #[must_use]
  pub const fn result(&self) -> &ResearchResult {
    &self.result
  }
  /// Returns retained raw result, reproduction/proposal, and accepted evidence Artifacts.
  #[must_use]
  pub fn artifacts(&self) -> &[FactoryArtifactReference] {
    &self.artifacts
  }
  /// Returns the canonical content-bound handoff identity.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.research-stage-handoff.v1", self)
  }
}
