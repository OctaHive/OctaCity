use super::{
  DefectResearchOutcome, MAX_RESEARCH_PROPOSAL_BYTES, RESEARCH_CONTRACT_VERSION, ResearchDetails, ResearchInput,
  ResearchOutcome, ResearchSourceReference, canonicalize, digest, invalid,
};
use crate::{
  BoundedSummary, EvidenceProducer, FactoryArtifactReference, FactoryDigest, FactoryError, FactoryKey, FactorySafeText,
  FlowDefinitionRef, FlowRunId, ImmutableReference, NodeAttemptId, NodeAttemptNumber, WorkKind, WorkflowCycleId,
};
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

/// Exact ordinary-Build provenance of one non-authoritative research result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchProvenance {
  /// Digest of the complete immutable Research Input.
  pub input_digest: FactoryDigest,
  /// Digest of the complete frozen Context Manifest.
  pub context_digest: FactoryDigest,
  /// Exact executed definition from the pinned research closure.
  pub definition: FlowDefinitionRef,
  /// Exact nested Flow execution.
  pub flow_run_id: FlowRunId,
  /// Current append-only workflow cycle.
  pub cycle_id: WorkflowCycleId,
  /// Node that produced the observations.
  pub node: FactoryKey,
  /// Immutable producing Node Attempt.
  pub node_attempt_id: NodeAttemptId,
  /// Bounded producing attempt number.
  pub attempt: NodeAttemptNumber,
  /// Exact ordinary Build, Attempt, Job, tool, and plugin.
  pub producer: EvidenceProducer,
  /// Exact selected model or deterministic command profile.
  pub model_or_tool: ImmutableReference,
  /// Exact task/prompt contract digest.
  pub task_digest: FactoryDigest,
  /// Retained raw typed result Artifact.
  pub result: FactoryArtifactReference,
  /// Authoritative observation time.
  pub observed_at: Timestamp,
}

/// Typed defect observations; the reproduction report still needs trusted validation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DefectResearchResult {
  /// One distinct bounded reproduction outcome.
  pub outcome: DefectResearchOutcome,
  /// Exact deterministic reproduction or diagnostic report.
  pub reproduction_report: FactoryArtifactReference,
  /// Bounded subject-bound summary, never an execution instruction.
  pub summary: BoundedSummary,
  /// Missing inputs or remaining bounded questions.
  pub unresolved_items: Vec<FactorySafeText>,
}

/// Bounded feature proposal and its frozen source provenance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureResearchResult {
  /// Exact retained proposal bytes.
  pub proposal: FactoryArtifactReference,
  /// Complete immutable source references selected for the proposal.
  pub sources: Vec<ResearchSourceReference>,
  /// Bounded explicit assumptions.
  pub assumptions: Vec<FactorySafeText>,
  /// Non-empty bounded alternatives considered by the proposal.
  pub alternatives: Vec<FactorySafeText>,
  /// Unresolved questions to pass to requirements or human review.
  pub unresolved_questions: Vec<FactorySafeText>,
  /// Bounded subject-bound summary.
  pub summary: BoundedSummary,
}

/// Closed provider observation vocabulary with no route, permission, or readiness field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "result", deny_unknown_fields)]
pub enum ResearchObservations {
  /// Defect reproduction observations.
  Defect(Box<DefectResearchResult>),
  /// Feature proposal observations.
  Feature(Box<FeatureResearchResult>),
}

/// Immutable schema-validated observations; acceptance remains a separate code-owned gate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ResearchResultWire")]
pub struct ResearchResult {
  schema_version: u16,
  provenance: ResearchProvenance,
  observations: ResearchObservations,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchResultWire {
  schema_version: u16,
  provenance: ResearchProvenance,
  observations: ResearchObservations,
}

impl TryFrom<ResearchResultWire> for ResearchResult {
  type Error = FactoryError;
  fn try_from(mut wire: ResearchResultWire) -> Result<Self, Self::Error> {
    if wire.schema_version != RESEARCH_CONTRACT_VERSION {
      return Err(invalid("research result version"));
    }
    match &mut wire.observations {
      ResearchObservations::Defect(row) => {
        canonicalize(&mut row.unresolved_items, 0)?;
        if row.outcome == DefectResearchOutcome::NeedsHumanInput && row.unresolved_items.is_empty() {
          return Err(invalid("research missing human question"));
        }
      }
      ResearchObservations::Feature(row) => {
        canonicalize(&mut row.sources, 1)?;
        canonicalize(&mut row.assumptions, 0)?;
        canonicalize(&mut row.alternatives, 1)?;
        canonicalize(&mut row.unresolved_questions, 0)?;
        if row.proposal.encoded_size() > MAX_RESEARCH_PROPOSAL_BYTES {
          return Err(invalid("research proposal bytes"));
        }
      }
    }
    let result = Self {
      schema_version: wire.schema_version,
      provenance: wire.provenance,
      observations: wire.observations,
    };
    if result.summary().subject().candidate().is_some() {
      return Err(invalid("research cannot publish a candidate"));
    }
    if serde_json::to_vec(&result)
      .map_err(|_| invalid("research result serialization"))?
      .len()
      > super::MAX_RESEARCH_CONTRACT_BYTES
    {
      return Err(invalid("research result bytes"));
    }
    Ok(result)
  }
}

impl ResearchResult {
  /// Constructs bounded observations bound to the exact dispatched input and context.
  pub fn new(
    input: &ResearchInput,
    provenance: ResearchProvenance,
    observations: ResearchObservations,
  ) -> Result<Self, FactoryError> {
    let result = Self::try_from(ResearchResultWire {
      schema_version: RESEARCH_CONTRACT_VERSION,
      provenance,
      observations,
    })?;
    result.validate_input(input)?;
    Ok(result)
  }

  /// Revalidates untrusted parsed observations against frozen dispatch inputs.
  pub fn validate_input(&self, input: &ResearchInput) -> Result<(), FactoryError> {
    if self.provenance.input_digest != input.digest()?
      || self.provenance.context_digest != input.context().digest()?
      || self.summary().subject().exact() != input.subject()
    {
      return Err(invalid("research result input"));
    }
    match (&self.observations, input.details()) {
      (ResearchObservations::Defect(_), ResearchDetails::Defect { .. }) => Ok(()),
      (ResearchObservations::Feature(row), ResearchDetails::Feature { sources, .. }) if &row.sources == sources => {
        Ok(())
      }
      _ => Err(invalid("research result kind or sources")),
    }
  }
  /// Returns the exact producing execution and input provenance.
  #[must_use]
  pub const fn provenance(&self) -> &ResearchProvenance {
    &self.provenance
  }
  /// Returns the complete non-authoritative typed observations.
  #[must_use]
  pub const fn observations(&self) -> &ResearchObservations {
    &self.observations
  }
  /// Returns the finite observed outcome.
  #[must_use]
  pub fn outcome(&self) -> ResearchOutcome {
    match &self.observations {
      ResearchObservations::Defect(row) => ResearchOutcome::Defect(row.outcome),
      ResearchObservations::Feature(_) => ResearchOutcome::FeatureProposal,
    }
  }
  /// Returns the observed Work kind.
  #[must_use]
  pub const fn kind(&self) -> WorkKind {
    match &self.observations {
      ResearchObservations::Defect(_) => WorkKind::Defect,
      ResearchObservations::Feature(_) => WorkKind::FeatureRequest,
    }
  }
  /// Returns the exact report or proposal that trusted validation must accept.
  #[must_use]
  pub fn evidence_artifact(&self) -> &FactoryArtifactReference {
    match &self.observations {
      ResearchObservations::Defect(row) => &row.reproduction_report,
      ResearchObservations::Feature(row) => &row.proposal,
    }
  }
  /// Returns the bounded summary passed to declared successors.
  #[must_use]
  pub fn summary(&self) -> &BoundedSummary {
    match &self.observations {
      ResearchObservations::Defect(row) => &row.summary,
      ResearchObservations::Feature(row) => &row.summary,
    }
  }
  /// Returns the canonical observation record digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.research-result.v1", self)
  }
}
