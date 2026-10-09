//! Exact retained deterministic evidence for one immutable candidate.

use std::collections::HashSet;

use octacity_server_domain::{ArtifactId, AttemptId, BuildId, JobId, Timestamp};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
  CandidateSubject, ChangeSetId, DeterministicGateOutcome, EvidenceManifestId, FactoryArtifactReference, FactoryDigest,
  FactoryError, FactoryKey, ImmutableReference, StageAttempt, StageAttemptId,
};

/// Maximum typed evidence items in one manifest.
pub const MAX_EVIDENCE_ITEMS: usize = 256;

/// Trusted immutable source candidate captured after a writable Stage Attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChangeSet {
  id: ChangeSetId,
  stage_attempt_id: StageAttemptId,
  subject: CandidateSubject,
  bundle_artifact: ArtifactId,
  manifest_artifact: ArtifactId,
}

impl ChangeSet {
  /// Constructs a ChangeSet bound to the Stage Attempt's exact base subject.
  pub fn new(
    id: ChangeSetId,
    stage: &StageAttempt,
    subject: CandidateSubject,
    bundle_artifact: ArtifactId,
    manifest_artifact: ArtifactId,
  ) -> Result<Self, FactoryError> {
    if subject.exact() != stage.subject() {
      return Err(FactoryError::InconsistentSubject);
    }
    if !matches!(
      stage.kind(),
      crate::FactoryStageKind::Implementation | crate::FactoryStageKind::Rework
    ) {
      return Err(FactoryError::InvalidReference {
        relationship: "changeset producing stage",
      });
    }
    if bundle_artifact == manifest_artifact {
      return Err(FactoryError::InvalidReference {
        relationship: "changeset artifacts",
      });
    }
    Ok(Self {
      id,
      stage_attempt_id: stage.id(),
      subject,
      bundle_artifact,
      manifest_artifact,
    })
  }

  /// Returns the ChangeSet identity.
  #[must_use]
  pub const fn id(&self) -> ChangeSetId {
    self.id
  }

  /// Returns the producing Stage Attempt.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the exact candidate subject.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the immutable candidate bundle Artifact.
  #[must_use]
  pub const fn bundle_artifact(&self) -> ArtifactId {
    self.bundle_artifact
  }

  /// Returns the immutable capture-manifest Artifact.
  #[must_use]
  pub const fn manifest_artifact(&self) -> ArtifactId {
    self.manifest_artifact
  }
}

/// Logical shape of one retained validation output.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceOutputKind {
  /// A machine-readable deterministic report.
  Report,
  /// A retained immutable Artifact consumed as evidence.
  Artifact,
}

impl EvidenceOutputKind {
  /// Returns the canonical persistence and digest representation.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Report => "report",
      Self::Artifact => "artifact",
    }
  }
}

/// Exact ordinary-Build producer of one evidence output.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceProducer {
  build_id: BuildId,
  attempt_id: AttemptId,
  job_id: JobId,
  tool: ImmutableReference,
  plugin: ImmutableReference,
}

impl EvidenceProducer {
  /// Binds an output to its Build, Attempt, Job, tool, and plugin identities.
  #[must_use]
  pub const fn new(
    build_id: BuildId,
    attempt_id: AttemptId,
    job_id: JobId,
    tool: ImmutableReference,
    plugin: ImmutableReference,
  ) -> Self {
    Self {
      build_id,
      attempt_id,
      job_id,
      tool,
      plugin,
    }
  }

  /// Returns the producing Build.
  #[must_use]
  pub const fn build_id(&self) -> BuildId {
    self.build_id
  }

  /// Returns the producing Attempt.
  #[must_use]
  pub const fn attempt_id(&self) -> AttemptId {
    self.attempt_id
  }

  /// Returns the producing Job.
  #[must_use]
  pub const fn job_id(&self) -> JobId {
    self.job_id
  }

  /// Returns the exact validation tool identity.
  #[must_use]
  pub const fn tool(&self) -> &ImmutableReference {
    &self.tool
  }

  /// Returns the exact producing plugin identity.
  #[must_use]
  pub const fn plugin(&self) -> &ImmutableReference {
    &self.plugin
  }
}

/// One required output in a complete Evidence Manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceRequirement {
  kind: FactoryKey,
  output_kind: EvidenceOutputKind,
  schema: ImmutableReference,
  tool: ImmutableReference,
  plugin: ImmutableReference,
}

impl EvidenceRequirement {
  /// Declares the exact schema and producer identities required for one evidence kind.
  #[must_use]
  pub const fn new(
    kind: FactoryKey,
    output_kind: EvidenceOutputKind,
    schema: ImmutableReference,
    tool: ImmutableReference,
    plugin: ImmutableReference,
  ) -> Self {
    Self {
      kind,
      output_kind,
      schema,
      tool,
      plugin,
    }
  }

  /// Returns the required evidence kind.
  #[must_use]
  pub const fn kind(&self) -> &FactoryKey {
    &self.kind
  }

  /// Returns the required logical output shape.
  #[must_use]
  pub const fn output_kind(&self) -> EvidenceOutputKind {
    self.output_kind
  }

  /// Returns the exact schema required to validate the output.
  #[must_use]
  pub const fn schema(&self) -> &ImmutableReference {
    &self.schema
  }

  /// Returns the exact validation tool required for the output.
  #[must_use]
  pub const fn tool(&self) -> &ImmutableReference {
    &self.tool
  }

  /// Returns the exact producing plugin required for the output.
  #[must_use]
  pub const fn plugin(&self) -> &ImmutableReference {
    &self.plugin
  }
}

/// Named inputs for one typed, content-addressed evidence item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceItemInput {
  /// Exact candidate validated by the producer.
  pub subject: CandidateSubject,
  /// Stable deterministic evidence kind.
  pub kind: FactoryKey,
  /// Whether the output is a report or another immutable Artifact.
  pub output_kind: EvidenceOutputKind,
  /// Published immutable output reference.
  pub artifact: FactoryArtifactReference,
  /// Exact output schema identity.
  pub schema: ImmutableReference,
  /// Exact ordinary-Build and implementation producer identities.
  pub producer: EvidenceProducer,
  /// Authoritative deterministic outcome parsed from the verified output.
  pub outcome: DeterministicGateOutcome,
  /// Time at which the verified output became visible.
  pub published_at: Timestamp,
  /// Exclusive freshness deadline selected by policy.
  pub fresh_until: Timestamp,
}

/// One typed, content-addressed report or Artifact in an Evidence Manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceItem {
  subject: CandidateSubject,
  kind: FactoryKey,
  output_kind: EvidenceOutputKind,
  artifact: FactoryArtifactReference,
  schema: ImmutableReference,
  producer: EvidenceProducer,
  outcome: DeterministicGateOutcome,
  published_at: Timestamp,
  fresh_until: Timestamp,
}

impl EvidenceItem {
  /// Constructs one typed evidence reference from trusted verified metadata.
  #[must_use]
  pub fn new(input: EvidenceItemInput) -> Self {
    Self {
      subject: input.subject,
      kind: input.kind,
      output_kind: input.output_kind,
      artifact: input.artifact,
      schema: input.schema,
      producer: input.producer,
      outcome: input.outcome,
      published_at: input.published_at,
      fresh_until: input.fresh_until,
    }
  }

  /// Returns the stable evidence kind.
  #[must_use]
  pub const fn kind(&self) -> &FactoryKey {
    &self.kind
  }

  /// Returns the immutable evidence Artifact.
  #[must_use]
  pub const fn artifact_id(&self) -> ArtifactId {
    self.artifact.artifact_id()
  }

  /// Returns the exact evidence content digest.
  #[must_use]
  pub const fn digest(&self) -> FactoryDigest {
    self.artifact.content_digest()
  }

  /// Returns the exact immutable Artifact reference.
  #[must_use]
  pub const fn artifact(&self) -> &FactoryArtifactReference {
    &self.artifact
  }

  /// Returns the exact candidate validated by this output.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns whether this item is a report or another Artifact.
  #[must_use]
  pub const fn output_kind(&self) -> EvidenceOutputKind {
    self.output_kind
  }

  /// Returns the exact schema identity used to validate the output.
  #[must_use]
  pub const fn schema(&self) -> &ImmutableReference {
    &self.schema
  }

  /// Returns the exact output producer.
  #[must_use]
  pub const fn producer(&self) -> &EvidenceProducer {
    &self.producer
  }

  /// Returns the authoritative deterministic outcome.
  #[must_use]
  pub const fn outcome(&self) -> DeterministicGateOutcome {
    self.outcome
  }

  /// Returns the verified publication time.
  #[must_use]
  pub const fn published_at(&self) -> Timestamp {
    self.published_at
  }

  /// Returns the exclusive policy freshness deadline.
  #[must_use]
  pub const fn fresh_until(&self) -> Timestamp {
    self.fresh_until
  }
}

/// Immutable bounded deterministic evidence for one exact candidate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvidenceManifest {
  id: EvidenceManifestId,
  changeset_id: ChangeSetId,
  subject: CandidateSubject,
  constructed_at: Timestamp,
  requirements: Vec<EvidenceRequirement>,
  items: Vec<EvidenceItem>,
}

impl EvidenceManifest {
  /// Constructs a complete manifest for the exact ChangeSet candidate.
  pub fn new(
    id: EvidenceManifestId,
    changeset: &ChangeSet,
    subject: CandidateSubject,
    constructed_at: Timestamp,
    requirements: Vec<EvidenceRequirement>,
    items: Vec<EvidenceItem>,
  ) -> Result<Self, FactoryError> {
    if subject != changeset.subject {
      return Err(FactoryError::InconsistentSubject);
    }
    Self::from_parts(id, changeset.id, subject, constructed_at, requirements, items)
  }

  fn from_parts(
    id: EvidenceManifestId,
    changeset_id: ChangeSetId,
    subject: CandidateSubject,
    constructed_at: Timestamp,
    mut requirements: Vec<EvidenceRequirement>,
    mut items: Vec<EvidenceItem>,
  ) -> Result<Self, FactoryError> {
    validate_count(&requirements, 1, MAX_EVIDENCE_ITEMS, "evidence requirements")?;
    if items.len() > MAX_EVIDENCE_ITEMS {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "evidence items",
      });
    }
    requirements.sort_unstable_by(|left, right| left.kind.cmp(&right.kind));
    items.sort_unstable_by(|left, right| left.kind.cmp(&right.kind));
    if requirements.windows(2).any(|pair| pair[0].kind == pair[1].kind)
      || items.windows(2).any(|pair| pair[0].kind == pair[1].kind)
      || items
        .iter()
        .map(EvidenceItem::artifact_id)
        .collect::<HashSet<_>>()
        .len()
        != items.len()
    {
      return Err(FactoryError::InvalidReference {
        relationship: "evidence artifact",
      });
    }
    if requirements.len() != items.len()
      || requirements.iter().zip(&items).any(|(requirement, item)| {
        requirement.kind != item.kind
          || requirement.output_kind != item.output_kind
          || requirement.schema != item.schema
          || &requirement.tool != item.producer.tool()
          || &requirement.plugin != item.producer.plugin()
      })
    {
      return Err(FactoryError::InvalidReference {
        relationship: "evidence completeness",
      });
    }
    if items.iter().any(|item| item.subject != subject) {
      return Err(FactoryError::InconsistentSubject);
    }
    if items
      .iter()
      .any(|item| item.published_at > constructed_at || constructed_at >= item.fresh_until)
    {
      return Err(FactoryError::InvalidReference {
        relationship: "evidence freshness",
      });
    }
    Ok(Self {
      id,
      changeset_id,
      subject,
      constructed_at,
      requirements,
      items,
    })
  }

  /// Returns the Evidence Manifest identity.
  #[must_use]
  pub const fn id(&self) -> EvidenceManifestId {
    self.id
  }

  /// Returns the exact ChangeSet identity.
  #[must_use]
  pub const fn changeset_id(&self) -> ChangeSetId {
    self.changeset_id
  }

  /// Returns the exact candidate subject.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the time at which all requirements were proven complete and fresh.
  #[must_use]
  pub const fn constructed_at(&self) -> Timestamp {
    self.constructed_at
  }

  /// Returns the exact requirements proven by this manifest.
  #[must_use]
  pub fn requirements(&self) -> &[EvidenceRequirement] {
    &self.requirements
  }

  /// Returns the bounded evidence items.
  #[must_use]
  pub fn items(&self) -> &[EvidenceItem] {
    &self.items
  }

  /// Resolves one canonical evidence kind to its exact retained item.
  #[must_use]
  pub fn item(&self, kind: &FactoryKey) -> Option<&EvidenceItem> {
    self
      .items
      .binary_search_by(|item| item.kind().cmp(kind))
      .ok()
      .map(|index| &self.items[index])
  }

  /// Computes the stable content digest of this complete manifest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let encoded = serde_json::to_vec(self).map_err(|_| FactoryError::InvalidReference {
      relationship: "evidence manifest serialization",
    })?;
    Ok(FactoryDigest::sha256(
      "octacity.factory.evidence-manifest.v2",
      &[&encoded],
    ))
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceManifestWire {
  id: EvidenceManifestId,
  changeset_id: ChangeSetId,
  subject: CandidateSubject,
  constructed_at: Timestamp,
  requirements: Vec<EvidenceRequirement>,
  items: Vec<EvidenceItem>,
}

impl<'de> Deserialize<'de> for EvidenceManifest {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = EvidenceManifestWire::deserialize(deserializer)?;
    Self::from_parts(
      wire.id,
      wire.changeset_id,
      wire.subject,
      wire.constructed_at,
      wire.requirements,
      wire.items,
    )
    .map_err(D::Error::custom)
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
