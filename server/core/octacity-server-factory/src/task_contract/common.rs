use octacity_server_domain::{ArtifactId, ImmutableRevision, RepositoryId};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{FactoryDigest, FactoryError, FactorySafeText, RetrievalReceiptId};

/// Schema version shared by the first immutable Factory task contracts.
pub const FACTORY_TASK_CONTRACT_VERSION: u16 = 1;
/// Maximum task or result Artifact references in one contract.
pub const MAX_TASK_ARTIFACTS: usize = 64;
/// Maximum declared deliverables in one Task Envelope.
pub const MAX_TASK_DELIVERABLES: usize = 64;
/// Maximum prior finding digests carried into one Task Envelope or handoff.
pub const MAX_TASK_PRIOR_FINDINGS: usize = 128;
/// Maximum typed findings in one evaluation result.
pub const MAX_TASK_EVALUATION_FINDINGS: usize = 128;
/// Maximum entries in one Context Manifest.
pub const MAX_CONTEXT_MANIFEST_ENTRIES: usize = 256;
/// Maximum encoded bytes referenced by one Context Manifest entry.
pub const MAX_CONTEXT_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
/// Maximum aggregate encoded bytes referenced by one Context Manifest.
pub const MAX_CONTEXT_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum bytes in a normalized repository-relative path.
pub const MAX_FACTORY_REPOSITORY_PATH_BYTES: usize = 1024;
/// Maximum entries in any one Stage Handoff narrative collection.
pub const MAX_STAGE_HANDOFF_ITEMS: usize = 128;

/// Program-owned purpose of one immutable Factory Task Envelope.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryTaskMode {
  /// Create a first candidate from the exact base revision.
  Implement,
  /// Evaluate one exact immutable candidate without mutation authority.
  Evaluate,
  /// Produce a replacement candidate from a previous exact candidate.
  Rework,
}

/// Strict result schema expected from the selected task.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryTaskResultSchema {
  /// Version-one implementation result.
  ImplementationV1,
  /// Version-one independent evaluation result.
  EvaluationV1,
}

/// Terminal classification emitted by an implementation or rework task.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationOutcome {
  /// A verified candidate and every required deliverable were published.
  Succeeded,
  /// The task completed without a usable candidate.
  Failed,
  /// The task could not determine a safe result from its bounded inputs.
  Indeterminate,
}

/// Terminal classification recorded in a Stage Handoff.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StageHandoffOutcome {
  /// The stage completed its declared responsibility.
  Succeeded,
  /// The stage terminated with a typed failure.
  Failed,
  /// The stage could not determine a safe result.
  Indeterminate,
  /// The stage was cancelled before completion.
  Cancelled,
}

/// Declared origin of one immutable Context Manifest entry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKind {
  /// Admitted task body or acceptance input.
  Task,
  /// Exact specification input.
  Specification,
  /// Typed output from a declared predecessor stage.
  StageHandoff,
  /// Typed result from a declared parent or child call.
  TypedResult,
  /// Bounded summary selected instead of a full result.
  BoundedSummary,
  /// Deterministic evidence required by the current call.
  Evidence,
  /// Immutable policy or criterion input.
  Policy,
  /// Exact repository range selected by deterministic policy.
  RepositoryRange,
  /// Untrusted repository content frozen by an immutable Retrieval Receipt.
  RepositoryFragment,
}

/// Exact immutable subject used by a task, result, handoff, or context record.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value", deny_unknown_fields)]
pub enum FactoryTaskSubject {
  /// Exact Project-owned repository base.
  Exact(crate::ExactSubject),
  /// Exact candidate derived from a Project-owned repository base.
  Candidate(crate::CandidateSubject),
}

impl FactoryTaskSubject {
  /// Returns the common exact base subject.
  #[must_use]
  pub const fn exact(&self) -> &crate::ExactSubject {
    match self {
      Self::Exact(subject) => subject,
      Self::Candidate(subject) => subject.exact(),
    }
  }

  /// Returns the candidate subject when this record is candidate-bound.
  #[must_use]
  pub const fn candidate(&self) -> Option<&crate::CandidateSubject> {
    match self {
      Self::Exact(_) => None,
      Self::Candidate(subject) => Some(subject),
    }
  }

  pub(super) fn permits_revision(&self, revision: &ImmutableRevision) -> bool {
    revision == self.exact().base_revision()
      || self
        .candidate()
        .is_some_and(|candidate| revision == candidate.candidate_revision())
  }
}

/// Immutable bounded Artifact reference used by task contracts.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct FactoryArtifactReference {
  artifact_id: ArtifactId,
  content_digest: FactoryDigest,
  encoded_size: u64,
}

impl FactoryArtifactReference {
  /// Constructs a non-empty Artifact reference within one-entry context bounds.
  pub fn new(artifact_id: ArtifactId, content_digest: FactoryDigest, encoded_size: u64) -> Result<Self, FactoryError> {
    validate_encoded_size(encoded_size)?;
    Ok(Self {
      artifact_id,
      content_digest,
      encoded_size,
    })
  }

  /// Returns the logical Artifact identity.
  #[must_use]
  pub const fn artifact_id(&self) -> ArtifactId {
    self.artifact_id
  }

  /// Returns the exact retained byte digest.
  #[must_use]
  pub const fn content_digest(&self) -> FactoryDigest {
    self.content_digest
  }

  /// Returns the exact retained byte count.
  #[must_use]
  pub const fn encoded_size(&self) -> u64 {
    self.encoded_size
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryArtifactReferenceWire {
  artifact_id: ArtifactId,
  content_digest: FactoryDigest,
  encoded_size: u64,
}

impl<'de> Deserialize<'de> for FactoryArtifactReference {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = FactoryArtifactReferenceWire::deserialize(deserializer)?;
    Self::new(wire.artifact_id, wire.content_digest, wire.encoded_size).map_err(D::Error::custom)
  }
}

/// Normalized repository-relative path without ambiguous or escaping segments.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FactoryRepositoryPath(String);

impl FactoryRepositoryPath {
  /// Parses a portable slash-separated repository-relative path.
  pub fn new(value: impl Into<String>) -> Result<Self, FactoryError> {
    let value = value.into();
    let valid = !value.is_empty()
      && value.len() <= MAX_FACTORY_REPOSITORY_PATH_BYTES
      && !value.starts_with('/')
      && !value.ends_with('/')
      && !value.contains('\\')
      && !value.chars().any(char::is_control)
      && value
        .split('/')
        .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."));
    if !valid {
      return Err(invalid("repository path"));
    }
    Ok(Self(value))
  }

  /// Borrows the normalized repository-relative path.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl Serialize for FactoryRepositoryPath {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for FactoryRepositoryPath {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// Exact bounded repository range selected as call context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FactoryRepositoryRange {
  repository_id: RepositoryId,
  revision: ImmutableRevision,
  path: FactoryRepositoryPath,
  first_line: u32,
  last_line: u32,
}

impl FactoryRepositoryRange {
  /// Constructs an inclusive non-zero source line range.
  pub fn new(
    repository_id: RepositoryId,
    revision: ImmutableRevision,
    path: FactoryRepositoryPath,
    first_line: u32,
    last_line: u32,
  ) -> Result<Self, FactoryError> {
    if first_line == 0 || last_line < first_line {
      return Err(invalid("repository range"));
    }
    Ok(Self {
      repository_id,
      revision,
      path,
      first_line,
      last_line,
    })
  }

  /// Returns the exact Repository identity.
  #[must_use]
  pub const fn repository_id(&self) -> RepositoryId {
    self.repository_id
  }

  /// Returns the exact visible revision.
  #[must_use]
  pub const fn revision(&self) -> &ImmutableRevision {
    &self.revision
  }

  /// Returns the normalized repository-relative path.
  #[must_use]
  pub const fn path(&self) -> &FactoryRepositoryPath {
    &self.path
  }

  /// Returns the first inclusive one-based line.
  #[must_use]
  pub const fn first_line(&self) -> u32 {
    self.first_line
  }

  /// Returns the last inclusive one-based line.
  #[must_use]
  pub const fn last_line(&self) -> u32 {
    self.last_line
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryRepositoryRangeWire {
  repository_id: RepositoryId,
  revision: ImmutableRevision,
  path: FactoryRepositoryPath,
  first_line: u32,
  last_line: u32,
}

impl<'de> Deserialize<'de> for FactoryRepositoryRange {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = FactoryRepositoryRangeWire::deserialize(deserializer)?;
    Self::new(
      wire.repository_id,
      wire.revision,
      wire.path,
      wire.first_line,
      wire.last_line,
    )
    .map_err(D::Error::custom)
  }
}

/// One untrusted repository fragment frozen by an immutable Retrieval Receipt.
///
/// The reference carries only exact identities, ranges, digests, and sizes. It
/// deliberately carries neither retrieved text nor any permission or lifecycle
/// field, so repository instructions remain data rather than control input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FactoryRepositoryFragmentReference {
  retrieval_receipt_id: RetrievalReceiptId,
  retrieval_receipt_digest: FactoryDigest,
  subject: FactoryTaskSubject,
  rank: u16,
  range: FactoryRepositoryRange,
  artifact: FactoryArtifactReference,
}

impl FactoryRepositoryFragmentReference {
  pub(super) fn new(
    retrieval_receipt_id: RetrievalReceiptId,
    retrieval_receipt_digest: FactoryDigest,
    subject: FactoryTaskSubject,
    rank: u16,
    range: FactoryRepositoryRange,
    artifact: FactoryArtifactReference,
  ) -> Result<Self, FactoryError> {
    if rank == 0 {
      return Err(invalid("retrieval fragment rank"));
    }
    if range.repository_id() != subject.exact().repository_id() || !subject.permits_revision(range.revision()) {
      return Err(FactoryError::InconsistentSubject);
    }
    Ok(Self {
      retrieval_receipt_id,
      retrieval_receipt_digest,
      subject,
      rank,
      range,
      artifact,
    })
  }

  /// Returns the immutable Retrieval Receipt identity.
  #[must_use]
  pub const fn retrieval_receipt_id(&self) -> RetrievalReceiptId {
    self.retrieval_receipt_id
  }

  /// Returns the exact canonical Retrieval Receipt digest.
  #[must_use]
  pub const fn retrieval_receipt_digest(&self) -> FactoryDigest {
    self.retrieval_receipt_digest
  }

  /// Returns the exact Project-owned repository subject.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the stable one-based ranking frozen by the receipt.
  #[must_use]
  pub const fn rank(&self) -> u16 {
    self.rank
  }

  /// Returns the exact repository range represented by the retained bytes.
  #[must_use]
  pub const fn range(&self) -> &FactoryRepositoryRange {
    &self.range
  }

  /// Returns the exact retained fragment bytes.
  #[must_use]
  pub const fn artifact(&self) -> &FactoryArtifactReference {
    &self.artifact
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryRepositoryFragmentReferenceWire {
  retrieval_receipt_id: RetrievalReceiptId,
  retrieval_receipt_digest: FactoryDigest,
  subject: FactoryTaskSubject,
  rank: u16,
  range: FactoryRepositoryRange,
  artifact: FactoryArtifactReference,
}

impl<'de> Deserialize<'de> for FactoryRepositoryFragmentReference {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = FactoryRepositoryFragmentReferenceWire::deserialize(deserializer)?;
    Self::new(
      wire.retrieval_receipt_id,
      wire.retrieval_receipt_digest,
      wire.subject,
      wire.rank,
      wire.range,
      wire.artifact,
    )
    .map_err(D::Error::custom)
  }
}

/// Bounded immutable location from which one context entry is loaded.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value", deny_unknown_fields)]
pub enum FactoryContextReference {
  /// Retained Artifact bytes.
  Artifact(FactoryArtifactReference),
  /// Exact repository range.
  RepositoryRange(FactoryRepositoryRange),
  /// Untrusted fragment selected by an immutable Repository Knowledge receipt.
  RepositoryFragment(Box<FactoryRepositoryFragmentReference>),
}

impl FactoryContextReference {
  /// Wraps a receipt-bound repository fragment without inflating every context reference.
  #[must_use]
  pub fn repository_fragment(reference: FactoryRepositoryFragmentReference) -> Self {
    Self::RepositoryFragment(Box::new(reference))
  }
}

/// One schema-versioned bounded summary safe to pass between model calls.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BoundedSummary {
  schema_version: u16,
  subject: FactoryTaskSubject,
  content: FactorySafeText,
  content_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl BoundedSummary {
  /// Constructs a summary and binds its exact safe textual content.
  #[must_use]
  pub fn new(subject: FactoryTaskSubject, content: FactorySafeText, provenance_digest: FactoryDigest) -> Self {
    let content_digest = safe_text_digest("octacity.factory.bounded-summary.content.v1", &content);
    Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      subject,
      content,
      content_digest,
      provenance_digest,
    }
  }

  /// Returns the exact subject summarized.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the bounded secret-checked summary.
  #[must_use]
  pub const fn content(&self) -> &FactorySafeText {
    &self.content
  }

  /// Returns the exact summary content digest.
  #[must_use]
  pub const fn content_digest(&self) -> FactoryDigest {
    self.content_digest
  }

  /// Returns the producer provenance digest.
  #[must_use]
  pub const fn provenance_digest(&self) -> FactoryDigest {
    self.provenance_digest
  }

  /// Returns deterministic canonical JSON bytes.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    canonical_json(self, "bounded summary serialization")
  }

  /// Returns the content-addressed canonical record digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    record_digest("octacity.factory.bounded-summary.v1", self)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundedSummaryWire {
  schema_version: u16,
  subject: FactoryTaskSubject,
  content: FactorySafeText,
  content_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl<'de> Deserialize<'de> for BoundedSummary {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = BoundedSummaryWire::deserialize(deserializer)?;
    require_version(wire.schema_version).map_err(D::Error::custom)?;
    let summary = Self::new(wire.subject, wire.content, wire.provenance_digest);
    if summary.content_digest != wire.content_digest {
      return Err(D::Error::custom(invalid("summary content digest")));
    }
    Ok(summary)
  }
}

pub(super) fn validate_encoded_size(encoded_size: u64) -> Result<(), FactoryError> {
  if encoded_size == 0 || encoded_size > MAX_CONTEXT_ENTRY_BYTES {
    return Err(invalid("encoded size"));
  }
  Ok(())
}

pub(super) fn validate_count<T>(
  values: &[T],
  minimum: usize,
  maximum: usize,
  collection: &'static str,
) -> Result<(), FactoryError> {
  if values.len() < minimum {
    return Err(invalid(collection));
  }
  if values.len() > maximum {
    return Err(FactoryError::CollectionLimitExceeded { collection });
  }
  Ok(())
}

pub(super) fn reject_duplicates<T: Eq>(values: &[T], field: &'static str) -> Result<(), FactoryError> {
  if values.windows(2).any(|pair| pair[0] == pair[1]) {
    return Err(invalid(field));
  }
  Ok(())
}

pub(super) fn strictly_sorted<T: Ord>(values: &[T]) -> bool {
  values.windows(2).all(|pair| pair[0] < pair[1])
}

pub(super) fn require_version(version: u16) -> Result<(), FactoryError> {
  if version == FACTORY_TASK_CONTRACT_VERSION {
    Ok(())
  } else {
    Err(invalid("schema version"))
  }
}

pub(super) fn canonical_json(value: &impl Serialize, field: &'static str) -> Result<Vec<u8>, FactoryError> {
  serde_json::to_vec(value).map_err(|_| invalid(field))
}

pub(super) fn record_digest(domain: &str, value: &impl Serialize) -> Result<FactoryDigest, FactoryError> {
  let bytes = canonical_json(value, "canonical record serialization")?;
  Ok(FactoryDigest::sha256(domain, &[&bytes]))
}

pub(super) fn safe_text_digest(domain: &str, value: &FactorySafeText) -> FactoryDigest {
  FactoryDigest::sha256(domain, &[value.as_str().as_bytes()])
}

pub(super) const fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidTaskContract { field }
}
