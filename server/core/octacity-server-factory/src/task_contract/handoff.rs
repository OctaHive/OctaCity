use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
  ChangeSetId, EvidenceManifestId, FactoryDigest, FactoryError, FactorySafeText, StageAttemptId, StageHandoffId,
};

use super::common::*;

/// Immutable bounded state passed from one completed stage to declared successors.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StageHandoff {
  schema_version: u16,
  id: StageHandoffId,
  stage_attempt_id: StageAttemptId,
  subject: FactoryTaskSubject,
  outcome: StageHandoffOutcome,
  summary: BoundedSummary,
  decisions: Vec<FactorySafeText>,
  assumptions: Vec<FactorySafeText>,
  unresolved_items: Vec<FactorySafeText>,
  changed_components: Vec<FactoryRepositoryPath>,
  validation_observations: Vec<FactorySafeText>,
  prior_findings: Vec<FactoryDigest>,
  artifacts: Vec<FactoryArtifactReference>,
  changeset_id: Option<ChangeSetId>,
  evidence_manifest_id: Option<EvidenceManifestId>,
  result_digest: FactoryDigest,
  policy_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

/// Stable identity and outcome of one immutable Stage Handoff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageHandoffDeclaration {
  /// Stable handoff identity.
  pub id: StageHandoffId,
  /// Stage Attempt that published the handoff.
  pub stage_attempt_id: StageAttemptId,
  /// Exact immutable subject.
  pub subject: FactoryTaskSubject,
  /// Typed stage outcome.
  pub outcome: StageHandoffOutcome,
}

/// Bounded successor-facing narrative retained by one handoff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageHandoffContent {
  /// Bounded stage summary.
  pub summary: BoundedSummary,
  /// Candidate-affecting decisions.
  pub decisions: Vec<FactorySafeText>,
  /// Candidate-affecting assumptions.
  pub assumptions: Vec<FactorySafeText>,
  /// Unresolved successor work.
  pub unresolved_items: Vec<FactorySafeText>,
  /// Changed repository components.
  pub changed_components: Vec<FactoryRepositoryPath>,
  /// Bounded validation observations.
  pub validation_observations: Vec<FactorySafeText>,
  /// Inherited typed finding digests.
  pub prior_findings: Vec<FactoryDigest>,
}

/// Exact retained references and controlling digests for one handoff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageHandoffReferences {
  /// Exact retained Artifacts.
  pub artifacts: Vec<FactoryArtifactReference>,
  /// Exact ChangeSet when a candidate was produced.
  pub changeset_id: Option<ChangeSetId>,
  /// Exact Evidence Manifest when evidence was produced.
  pub evidence_manifest_id: Option<EvidenceManifestId>,
  /// Exact typed result digest.
  pub result_digest: FactoryDigest,
  /// Exact immutable policy digest.
  pub policy_digest: FactoryDigest,
  /// Exact producer provenance digest.
  pub provenance_digest: FactoryDigest,
}

impl StageHandoff {
  /// Constructs a subject-consistent handoff with canonical bounded collections.
  pub fn new(
    declaration: StageHandoffDeclaration,
    mut content: StageHandoffContent,
    mut references: StageHandoffReferences,
  ) -> Result<Self, FactoryError> {
    if content.summary.subject() != &declaration.subject {
      return Err(FactoryError::InconsistentSubject);
    }
    canonicalize_handoff_items(&mut content.decisions, "handoff decisions")?;
    canonicalize_handoff_items(&mut content.assumptions, "handoff assumptions")?;
    canonicalize_handoff_items(&mut content.unresolved_items, "handoff unresolved items")?;
    canonicalize_handoff_items(&mut content.changed_components, "handoff changed components")?;
    canonicalize_handoff_items(&mut content.validation_observations, "handoff validation observations")?;
    canonicalize_handoff_items(&mut content.prior_findings, "handoff prior findings")?;
    validate_count(&references.artifacts, 0, MAX_TASK_ARTIFACTS, "handoff artifacts")?;
    references.artifacts.sort();
    reject_duplicates(&references.artifacts, "handoff artifacts")?;
    if declaration.outcome == StageHandoffOutcome::Succeeded
      && declaration.subject.candidate().is_some()
      && references.changeset_id.is_none()
      && references.evidence_manifest_id.is_none()
    {
      return Err(invalid("successful candidate handoff references"));
    }
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      id: declaration.id,
      stage_attempt_id: declaration.stage_attempt_id,
      subject: declaration.subject,
      outcome: declaration.outcome,
      summary: content.summary,
      decisions: content.decisions,
      assumptions: content.assumptions,
      unresolved_items: content.unresolved_items,
      changed_components: content.changed_components,
      validation_observations: content.validation_observations,
      prior_findings: content.prior_findings,
      artifacts: references.artifacts,
      changeset_id: references.changeset_id,
      evidence_manifest_id: references.evidence_manifest_id,
      result_digest: references.result_digest,
      policy_digest: references.policy_digest,
      provenance_digest: references.provenance_digest,
    })
  }

  /// Returns the immutable handoff identity.
  #[must_use]
  pub const fn id(&self) -> StageHandoffId {
    self.id
  }

  /// Returns the Stage Attempt that published this handoff.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the exact handoff subject.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the typed stage outcome.
  #[must_use]
  pub const fn outcome(&self) -> StageHandoffOutcome {
    self.outcome
  }

  /// Returns the bounded stage summary.
  #[must_use]
  pub const fn summary(&self) -> &BoundedSummary {
    &self.summary
  }

  /// Returns candidate-affecting decisions in canonical order.
  #[must_use]
  pub fn decisions(&self) -> &[FactorySafeText] {
    &self.decisions
  }

  /// Returns candidate-affecting assumptions in canonical order.
  #[must_use]
  pub fn assumptions(&self) -> &[FactorySafeText] {
    &self.assumptions
  }

  /// Returns unresolved successor work in canonical order.
  #[must_use]
  pub fn unresolved_items(&self) -> &[FactorySafeText] {
    &self.unresolved_items
  }

  /// Returns changed repository components in canonical order.
  #[must_use]
  pub fn changed_components(&self) -> &[FactoryRepositoryPath] {
    &self.changed_components
  }

  /// Returns bounded validation observations in canonical order.
  #[must_use]
  pub fn validation_observations(&self) -> &[FactorySafeText] {
    &self.validation_observations
  }

  /// Returns inherited typed finding digests in canonical order.
  #[must_use]
  pub fn prior_findings(&self) -> &[FactoryDigest] {
    &self.prior_findings
  }

  /// Returns exact referenced Artifacts in canonical order.
  #[must_use]
  pub fn artifacts(&self) -> &[FactoryArtifactReference] {
    &self.artifacts
  }

  /// Returns the exact ChangeSet reference when the handoff carries a candidate.
  #[must_use]
  pub const fn changeset_id(&self) -> Option<ChangeSetId> {
    self.changeset_id
  }

  /// Returns the exact Evidence Manifest reference when evidence was produced.
  #[must_use]
  pub const fn evidence_manifest_id(&self) -> Option<EvidenceManifestId> {
    self.evidence_manifest_id
  }

  /// Returns the exact typed result digest.
  #[must_use]
  pub const fn result_digest(&self) -> FactoryDigest {
    self.result_digest
  }

  /// Returns the exact immutable policy digest.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }

  /// Returns the exact producer provenance digest.
  #[must_use]
  pub const fn provenance_digest(&self) -> FactoryDigest {
    self.provenance_digest
  }

  /// Returns deterministic canonical JSON bytes.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    canonical_json(self, "stage handoff serialization")
  }

  /// Returns the content-addressed canonical handoff digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    record_digest("octacity.factory.stage-handoff.v1", self)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StageHandoffWire {
  schema_version: u16,
  id: StageHandoffId,
  stage_attempt_id: StageAttemptId,
  subject: FactoryTaskSubject,
  outcome: StageHandoffOutcome,
  summary: BoundedSummary,
  decisions: Vec<FactorySafeText>,
  assumptions: Vec<FactorySafeText>,
  unresolved_items: Vec<FactorySafeText>,
  changed_components: Vec<FactoryRepositoryPath>,
  validation_observations: Vec<FactorySafeText>,
  prior_findings: Vec<FactoryDigest>,
  artifacts: Vec<FactoryArtifactReference>,
  changeset_id: Option<ChangeSetId>,
  evidence_manifest_id: Option<EvidenceManifestId>,
  result_digest: FactoryDigest,
  policy_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl<'de> Deserialize<'de> for StageHandoff {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = StageHandoffWire::deserialize(deserializer)?;
    require_version(wire.schema_version).map_err(D::Error::custom)?;
    Self::new(
      StageHandoffDeclaration {
        id: wire.id,
        stage_attempt_id: wire.stage_attempt_id,
        subject: wire.subject,
        outcome: wire.outcome,
      },
      StageHandoffContent {
        summary: wire.summary,
        decisions: wire.decisions,
        assumptions: wire.assumptions,
        unresolved_items: wire.unresolved_items,
        changed_components: wire.changed_components,
        validation_observations: wire.validation_observations,
        prior_findings: wire.prior_findings,
      },
      StageHandoffReferences {
        artifacts: wire.artifacts,
        changeset_id: wire.changeset_id,
        evidence_manifest_id: wire.evidence_manifest_id,
        result_digest: wire.result_digest,
        policy_digest: wire.policy_digest,
        provenance_digest: wire.provenance_digest,
      },
    )
    .map_err(D::Error::custom)
  }
}

fn canonicalize_handoff_items<T: Ord>(values: &mut [T], collection: &'static str) -> Result<(), FactoryError> {
  validate_count(values, 0, MAX_STAGE_HANDOFF_ITEMS, collection)?;
  values.sort();
  reject_duplicates(values, collection)
}
