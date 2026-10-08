use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{AssessmentOutcome, FactoryDigest, FactoryError, FactorySafeText, FindingSeverity, TaskEnvelopeId};

use super::{
  common::*,
  envelope::{FactoryTaskDeliverable, FactoryTaskEnvelope},
};

/// One declared task deliverable bound to its exact retained Artifact.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct FactoryProducedDeliverable {
  declaration: FactoryTaskDeliverable,
  artifact: FactoryArtifactReference,
}

impl FactoryProducedDeliverable {
  /// Binds one declared output to the exact bytes published for it.
  #[must_use]
  pub const fn new(declaration: FactoryTaskDeliverable, artifact: FactoryArtifactReference) -> Self {
    Self { declaration, artifact }
  }

  /// Returns the declared output kind, path, and requirement.
  #[must_use]
  pub const fn declaration(&self) -> &FactoryTaskDeliverable {
    &self.declaration
  }

  /// Returns the exact retained output Artifact.
  #[must_use]
  pub const fn artifact(&self) -> &FactoryArtifactReference {
    &self.artifact
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryProducedDeliverableWire {
  declaration: FactoryTaskDeliverable,
  artifact: FactoryArtifactReference,
}

impl<'de> Deserialize<'de> for FactoryProducedDeliverable {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = FactoryProducedDeliverableWire::deserialize(deserializer)?;
    Ok(Self::new(wire.declaration, wire.artifact))
  }
}

/// Typed versioned result from an implementation or rework task.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ImplementationResult {
  schema_version: u16,
  task_envelope_id: TaskEnvelopeId,
  task_envelope_digest: FactoryDigest,
  subject: FactoryTaskSubject,
  outcome: ImplementationOutcome,
  summary: BoundedSummary,
  deliverables: Vec<FactoryProducedDeliverable>,
  deliverable_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl ImplementationResult {
  /// Constructs a result bound to one implementation-compatible Task Envelope.
  pub fn new(
    envelope: &FactoryTaskEnvelope,
    outcome: ImplementationOutcome,
    candidate: Option<crate::CandidateSubject>,
    summary: BoundedSummary,
    mut deliverables: Vec<FactoryProducedDeliverable>,
    provenance_digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if envelope.mode() == FactoryTaskMode::Evaluate {
      return Err(invalid("implementation task mode"));
    }
    let subject = match (outcome, candidate) {
      (ImplementationOutcome::Succeeded, Some(candidate)) if candidate.exact() == envelope.subject().exact() => {
        FactoryTaskSubject::Candidate(candidate)
      }
      (ImplementationOutcome::Succeeded, _) => return Err(FactoryError::InconsistentSubject),
      (_, None) => envelope.subject().clone(),
      (_, Some(_)) => return Err(invalid("unsuccessful implementation candidate")),
    };
    if summary.subject() != &subject {
      return Err(FactoryError::InconsistentSubject);
    }
    validate_count(&deliverables, 0, MAX_TASK_DELIVERABLES, "implementation deliverables")?;
    deliverables.sort();
    reject_duplicates(&deliverables, "implementation deliverables")?;
    validate_produced_deliverables(outcome, &deliverables, envelope.deliverables())?;
    let deliverable_digest = record_digest("octacity.factory.implementation-deliverables.v1", &deliverables)?;
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      task_envelope_id: envelope.id(),
      task_envelope_digest: envelope.digest()?,
      subject,
      outcome,
      summary,
      deliverables,
      deliverable_digest,
      provenance_digest,
    })
  }

  /// Returns the Task Envelope identity consumed by this result.
  #[must_use]
  pub const fn task_envelope_id(&self) -> TaskEnvelopeId {
    self.task_envelope_id
  }

  /// Returns the exact consumed Task Envelope digest.
  #[must_use]
  pub const fn task_envelope_digest(&self) -> FactoryDigest {
    self.task_envelope_digest
  }

  /// Returns the exact resulting subject.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the typed terminal outcome.
  #[must_use]
  pub const fn outcome(&self) -> ImplementationOutcome {
    self.outcome
  }

  /// Returns the bounded result summary.
  #[must_use]
  pub const fn summary(&self) -> &BoundedSummary {
    &self.summary
  }

  /// Returns retained result deliverables in canonical order.
  #[must_use]
  pub fn deliverables(&self) -> &[FactoryProducedDeliverable] {
    &self.deliverables
  }

  /// Returns the canonical deliverable-set digest.
  #[must_use]
  pub const fn deliverable_digest(&self) -> FactoryDigest {
    self.deliverable_digest
  }

  /// Returns the exact producer provenance digest.
  #[must_use]
  pub const fn provenance_digest(&self) -> FactoryDigest {
    self.provenance_digest
  }

  /// Returns deterministic canonical JSON bytes.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    canonical_json(self, "implementation result serialization")
  }

  /// Returns the content-addressed canonical result digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    record_digest("octacity.factory.implementation-result.v1", self)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImplementationResultWire {
  schema_version: u16,
  task_envelope_id: TaskEnvelopeId,
  task_envelope_digest: FactoryDigest,
  subject: FactoryTaskSubject,
  outcome: ImplementationOutcome,
  summary: BoundedSummary,
  deliverables: Vec<FactoryProducedDeliverable>,
  deliverable_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl<'de> Deserialize<'de> for ImplementationResult {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let mut wire = ImplementationResultWire::deserialize(deserializer)?;
    require_version(wire.schema_version).map_err(D::Error::custom)?;
    if wire.summary.subject() != &wire.subject {
      return Err(D::Error::custom(FactoryError::InconsistentSubject));
    }
    validate_count(
      &wire.deliverables,
      0,
      MAX_TASK_DELIVERABLES,
      "implementation deliverables",
    )
    .map_err(D::Error::custom)?;
    wire.deliverables.sort();
    reject_duplicates(&wire.deliverables, "implementation deliverables").map_err(D::Error::custom)?;
    if (wire.outcome == ImplementationOutcome::Succeeded && wire.deliverables.is_empty())
      || (wire.outcome != ImplementationOutcome::Succeeded && !wire.deliverables.is_empty())
    {
      return Err(D::Error::custom(invalid("implementation outcome deliverables")));
    }
    let digest =
      record_digest("octacity.factory.implementation-deliverables.v1", &wire.deliverables).map_err(D::Error::custom)?;
    if digest != wire.deliverable_digest {
      return Err(D::Error::custom(invalid("implementation deliverable digest")));
    }
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      task_envelope_id: wire.task_envelope_id,
      task_envelope_digest: wire.task_envelope_digest,
      subject: wire.subject,
      outcome: wire.outcome,
      summary: wire.summary,
      deliverables: wire.deliverables,
      deliverable_digest: wire.deliverable_digest,
      provenance_digest: wire.provenance_digest,
    })
  }
}

/// One strict provider-neutral finding returned by an evaluation task.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum EvaluationResultFinding {
  /// Policy-relevant candidate violation.
  Violation {
    /// Ordered severity interpreted later by deterministic policy.
    severity: FindingSeverity,
    /// Bounded secret-checked observation.
    summary: FactorySafeText,
  },
  /// Missing evidence prevented a determination.
  EvidenceGap {
    /// Bounded secret-checked evidence requirement.
    summary: FactorySafeText,
  },
}

/// Typed versioned result from one independent evaluation task.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvaluationResult {
  schema_version: u16,
  task_envelope_id: TaskEnvelopeId,
  task_envelope_digest: FactoryDigest,
  subject: crate::CandidateSubject,
  outcome: AssessmentOutcome,
  summary: BoundedSummary,
  findings: Vec<EvaluationResultFinding>,
  content_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl EvaluationResult {
  /// Constructs a strict result bound to one candidate evaluation envelope.
  pub fn new(
    envelope: &FactoryTaskEnvelope,
    outcome: AssessmentOutcome,
    summary: BoundedSummary,
    mut findings: Vec<EvaluationResultFinding>,
    provenance_digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if envelope.mode() != FactoryTaskMode::Evaluate {
      return Err(invalid("evaluation task mode"));
    }
    let subject = envelope
      .subject()
      .candidate()
      .cloned()
      .ok_or_else(|| invalid("evaluation candidate"))?;
    if summary.subject() != envelope.subject() {
      return Err(FactoryError::InconsistentSubject);
    }
    validate_evaluation_findings(outcome, &findings)?;
    findings.sort();
    reject_duplicates(&findings, "evaluation findings")?;
    let content_digest = evaluation_content_digest(outcome, &summary, &findings)?;
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      task_envelope_id: envelope.id(),
      task_envelope_digest: envelope.digest()?,
      subject,
      outcome,
      summary,
      findings,
      content_digest,
      provenance_digest,
    })
  }

  /// Returns the Task Envelope identity consumed by this result.
  #[must_use]
  pub const fn task_envelope_id(&self) -> TaskEnvelopeId {
    self.task_envelope_id
  }

  /// Returns the exact consumed Task Envelope digest.
  #[must_use]
  pub const fn task_envelope_digest(&self) -> FactoryDigest {
    self.task_envelope_digest
  }

  /// Returns the exact evaluated candidate.
  #[must_use]
  pub const fn subject(&self) -> &crate::CandidateSubject {
    &self.subject
  }

  /// Returns the schema-valid assessment outcome.
  #[must_use]
  pub const fn outcome(&self) -> AssessmentOutcome {
    self.outcome
  }

  /// Returns the bounded result summary.
  #[must_use]
  pub const fn summary(&self) -> &BoundedSummary {
    &self.summary
  }

  /// Returns bounded findings in canonical order.
  #[must_use]
  pub fn findings(&self) -> &[EvaluationResultFinding] {
    &self.findings
  }

  /// Returns the digest of the typed outcome, summary, and findings.
  #[must_use]
  pub const fn content_digest(&self) -> FactoryDigest {
    self.content_digest
  }

  /// Returns the exact evaluator provenance digest.
  #[must_use]
  pub const fn provenance_digest(&self) -> FactoryDigest {
    self.provenance_digest
  }

  /// Returns deterministic canonical JSON bytes.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    canonical_json(self, "evaluation result serialization")
  }

  /// Returns the content-addressed canonical result digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    record_digest("octacity.factory.evaluation-result.v1", self)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationResultWire {
  schema_version: u16,
  task_envelope_id: TaskEnvelopeId,
  task_envelope_digest: FactoryDigest,
  subject: crate::CandidateSubject,
  outcome: AssessmentOutcome,
  summary: BoundedSummary,
  findings: Vec<EvaluationResultFinding>,
  content_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl<'de> Deserialize<'de> for EvaluationResult {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let mut wire = EvaluationResultWire::deserialize(deserializer)?;
    require_version(wire.schema_version).map_err(D::Error::custom)?;
    if wire.summary.subject() != &FactoryTaskSubject::Candidate(wire.subject.clone()) {
      return Err(D::Error::custom(FactoryError::InconsistentSubject));
    }
    validate_evaluation_findings(wire.outcome, &wire.findings).map_err(D::Error::custom)?;
    wire.findings.sort();
    reject_duplicates(&wire.findings, "evaluation findings").map_err(D::Error::custom)?;
    let content_digest =
      evaluation_content_digest(wire.outcome, &wire.summary, &wire.findings).map_err(D::Error::custom)?;
    if content_digest != wire.content_digest {
      return Err(D::Error::custom(invalid("evaluation content digest")));
    }
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      task_envelope_id: wire.task_envelope_id,
      task_envelope_digest: wire.task_envelope_digest,
      subject: wire.subject,
      outcome: wire.outcome,
      summary: wire.summary,
      findings: wire.findings,
      content_digest: wire.content_digest,
      provenance_digest: wire.provenance_digest,
    })
  }
}

fn validate_evaluation_findings(
  outcome: AssessmentOutcome,
  findings: &[EvaluationResultFinding],
) -> Result<(), FactoryError> {
  validate_count(
    findings,
    usize::from(outcome != AssessmentOutcome::Satisfied),
    MAX_TASK_EVALUATION_FINDINGS,
    "evaluation findings",
  )?;
  let valid = match outcome {
    AssessmentOutcome::Satisfied => findings.is_empty(),
    AssessmentOutcome::Violated => findings
      .iter()
      .all(|finding| matches!(finding, EvaluationResultFinding::Violation { .. })),
    AssessmentOutcome::Indeterminate => findings
      .iter()
      .all(|finding| matches!(finding, EvaluationResultFinding::EvidenceGap { .. })),
  };
  if valid {
    Ok(())
  } else {
    Err(invalid("evaluation outcome findings"))
  }
}

fn validate_produced_deliverables(
  outcome: ImplementationOutcome,
  produced: &[FactoryProducedDeliverable],
  declared: &[FactoryTaskDeliverable],
) -> Result<(), FactoryError> {
  if produced.iter().any(|item| !declared.contains(&item.declaration)) {
    return Err(invalid("undeclared implementation deliverable"));
  }
  let distinct_declarations = produced.iter().map(|item| &item.declaration).collect::<HashSet<_>>();
  if distinct_declarations.len() != produced.len() {
    return Err(invalid("implementation deliverable declaration"));
  }
  let all_required_present = declared
    .iter()
    .filter(|declaration| declaration.required())
    .all(|declaration| distinct_declarations.contains(declaration));
  if outcome == ImplementationOutcome::Succeeded && !all_required_present {
    return Err(invalid("required implementation deliverable"));
  }
  if outcome != ImplementationOutcome::Succeeded && !produced.is_empty() {
    return Err(invalid("unsuccessful implementation deliverable"));
  }
  Ok(())
}

fn evaluation_content_digest(
  outcome: AssessmentOutcome,
  summary: &BoundedSummary,
  findings: &[EvaluationResultFinding],
) -> Result<FactoryDigest, FactoryError> {
  #[derive(Serialize)]
  struct Content<'a> {
    outcome: AssessmentOutcome,
    summary: &'a BoundedSummary,
    findings: &'a [EvaluationResultFinding],
  }
  record_digest(
    "octacity.factory.evaluation-content.v1",
    &Content {
      outcome,
      summary,
      findings,
    },
  )
}
