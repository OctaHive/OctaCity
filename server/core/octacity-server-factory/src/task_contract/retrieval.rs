use octacity_server_domain::ImmutableRevision;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{FactoryDigest, FactoryError, FactorySafeText, ImmutableReference, RetrievalReceiptId};

use super::common::*;

/// Schema version of the first Repository Knowledge retrieval contract.
pub const RETRIEVAL_RECEIPT_VERSION: u16 = 1;
/// Maximum fragments frozen by one bounded retrieval operation.
pub const MAX_RETRIEVAL_FRAGMENTS: usize = 64;

/// One ranked immutable repository fragment retained outside the Factory core.
///
/// Fragment bytes are untrusted repository content. The receipt stores only an
/// Artifact reference and exact range, never text that could be interpreted as
/// Factory policy, permissions, or a lifecycle instruction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepositoryFragment {
  rank: u16,
  range: FactoryRepositoryRange,
  artifact: FactoryArtifactReference,
}

impl RepositoryFragment {
  /// Constructs one one-based ranked fragment.
  pub fn new(
    rank: u16,
    range: FactoryRepositoryRange,
    artifact: FactoryArtifactReference,
  ) -> Result<Self, FactoryError> {
    if rank == 0 {
      return Err(invalid("retrieval fragment rank"));
    }
    Ok(Self { rank, range, artifact })
  }

  /// Returns the stable one-based retrieval rank.
  #[must_use]
  pub const fn rank(&self) -> u16 {
    self.rank
  }

  /// Returns the exact repository range represented by the fragment.
  #[must_use]
  pub const fn range(&self) -> &FactoryRepositoryRange {
    &self.range
  }

  /// Returns the retained untrusted fragment bytes.
  #[must_use]
  pub const fn artifact(&self) -> &FactoryArtifactReference {
    &self.artifact
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryFragmentWire {
  rank: u16,
  range: FactoryRepositoryRange,
  artifact: FactoryArtifactReference,
}

impl<'de> Deserialize<'de> for RepositoryFragment {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = RepositoryFragmentWire::deserialize(deserializer)?;
    Self::new(wire.rank, wire.range, wire.artifact).map_err(D::Error::custom)
  }
}

/// Immutable provenance for one future Repository Knowledge retrieval.
///
/// This is a value contract only. It intentionally introduces no retrieval
/// service, provider port, index worker, or production RAG implementation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RetrievalReceipt {
  schema_version: u16,
  id: RetrievalReceiptId,
  subject: FactoryTaskSubject,
  revision: ImmutableRevision,
  index: ImmutableReference,
  embedding: ImmutableReference,
  normalized_query: FactorySafeText,
  retrieval_policy: ImmutableReference,
  fragments: Vec<RepositoryFragment>,
  fragments_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl RetrievalReceipt {
  /// Constructs one revision-bound, canonically ranked retrieval result.
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    id: RetrievalReceiptId,
    subject: FactoryTaskSubject,
    revision: ImmutableRevision,
    index: ImmutableReference,
    embedding: ImmutableReference,
    normalized_query: FactorySafeText,
    retrieval_policy: ImmutableReference,
    mut fragments: Vec<RepositoryFragment>,
    provenance_digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if !subject.permits_revision(&revision) {
      return Err(FactoryError::InconsistentSubject);
    }
    if normalized_query
      .as_str()
      .split_whitespace()
      .collect::<Vec<_>>()
      .join(" ")
      != normalized_query.as_str()
    {
      return Err(invalid("retrieval normalized query"));
    }
    validate_count(&fragments, 1, MAX_RETRIEVAL_FRAGMENTS, "retrieval fragments")?;
    fragments.sort_by_key(RepositoryFragment::rank);
    for (position, fragment) in fragments.iter().enumerate() {
      let expected_rank = u16::try_from(position + 1).map_err(|_| invalid("retrieval fragment rank"))?;
      if fragment.rank != expected_rank {
        return Err(invalid("retrieval fragment rank"));
      }
      if fragment.range.repository_id() != subject.exact().repository_id() || fragment.range.revision() != &revision {
        return Err(FactoryError::InconsistentSubject);
      }
    }
    if fragments.iter().enumerate().any(|(index, fragment)| {
      fragments
        .iter()
        .skip(index + 1)
        .any(|other| other.range == fragment.range || other.artifact.artifact_id() == fragment.artifact.artifact_id())
    }) {
      return Err(invalid("retrieval fragment identity"));
    }
    let total_bytes = fragments.iter().try_fold(0_u64, |total, fragment| {
      total
        .checked_add(fragment.artifact.encoded_size())
        .ok_or_else(|| invalid("retrieval fragment bytes"))
    })?;
    if total_bytes > MAX_CONTEXT_MANIFEST_BYTES {
      return Err(invalid("retrieval fragment bytes"));
    }
    let fragments_digest = record_digest("octacity.factory.retrieval-fragments.v1", &fragments)?;
    Ok(Self {
      schema_version: RETRIEVAL_RECEIPT_VERSION,
      id,
      subject,
      revision,
      index,
      embedding,
      normalized_query,
      retrieval_policy,
      fragments,
      fragments_digest,
      provenance_digest,
    })
  }

  /// Returns the immutable receipt identity.
  #[must_use]
  pub const fn id(&self) -> RetrievalReceiptId {
    self.id
  }

  /// Returns the exact Project-owned repository subject.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the exact immutable revision searched by the producer.
  #[must_use]
  pub const fn revision(&self) -> &ImmutableRevision {
    &self.revision
  }

  /// Returns the exact repository index identity.
  #[must_use]
  pub const fn index(&self) -> &ImmutableReference {
    &self.index
  }

  /// Returns the exact embedding model identity.
  #[must_use]
  pub const fn embedding(&self) -> &ImmutableReference {
    &self.embedding
  }

  /// Returns the bounded normalized retrieval query.
  #[must_use]
  pub const fn normalized_query(&self) -> &FactorySafeText {
    &self.normalized_query
  }

  /// Returns the exact immutable retrieval policy.
  #[must_use]
  pub const fn retrieval_policy(&self) -> &ImmutableReference {
    &self.retrieval_policy
  }

  /// Returns fragments in frozen stable-rank order.
  #[must_use]
  pub fn fragments(&self) -> &[RepositoryFragment] {
    &self.fragments
  }

  /// Returns the digest of the complete ranked fragment list.
  #[must_use]
  pub const fn fragments_digest(&self) -> FactoryDigest {
    self.fragments_digest
  }

  /// Returns the producer provenance digest.
  #[must_use]
  pub const fn provenance_digest(&self) -> FactoryDigest {
    self.provenance_digest
  }

  /// Creates the only repository-fragment reference accepted for this rank.
  pub fn fragment_reference(&self, rank: u16) -> Result<FactoryRepositoryFragmentReference, FactoryError> {
    let fragment = self
      .fragments
      .iter()
      .find(|fragment| fragment.rank == rank)
      .ok_or_else(|| invalid("retrieval fragment rank"))?;
    FactoryRepositoryFragmentReference::new(
      self.id,
      self.digest()?,
      self.subject.clone(),
      fragment.rank,
      fragment.range.clone(),
      fragment.artifact.clone(),
    )
  }

  /// Checks that a manifest reference came from this unchanged receipt.
  pub fn contains_reference(&self, reference: &FactoryRepositoryFragmentReference) -> Result<bool, FactoryError> {
    if reference.retrieval_receipt_id() != self.id
      || reference.retrieval_receipt_digest() != self.digest()?
      || reference.subject() != &self.subject
    {
      return Ok(false);
    }
    Ok(self.fragments.iter().any(|fragment| {
      fragment.rank == reference.rank()
        && &fragment.range == reference.range()
        && &fragment.artifact == reference.artifact()
    }))
  }

  /// Returns deterministic canonical JSON bytes.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    canonical_json(self, "retrieval receipt serialization")
  }

  /// Returns the content-addressed canonical receipt digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    record_digest("octacity.factory.retrieval-receipt.v1", self)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetrievalReceiptWire {
  schema_version: u16,
  id: RetrievalReceiptId,
  subject: FactoryTaskSubject,
  revision: ImmutableRevision,
  index: ImmutableReference,
  embedding: ImmutableReference,
  normalized_query: FactorySafeText,
  retrieval_policy: ImmutableReference,
  fragments: Vec<RepositoryFragment>,
  fragments_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl<'de> Deserialize<'de> for RetrievalReceipt {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = RetrievalReceiptWire::deserialize(deserializer)?;
    if wire.schema_version != RETRIEVAL_RECEIPT_VERSION {
      return Err(D::Error::custom(invalid("retrieval receipt schema version")));
    }
    let expected_fragments_digest = wire.fragments_digest;
    let receipt = Self::new(
      wire.id,
      wire.subject,
      wire.revision,
      wire.index,
      wire.embedding,
      wire.normalized_query,
      wire.retrieval_policy,
      wire.fragments,
      wire.provenance_digest,
    )
    .map_err(D::Error::custom)?;
    if receipt.fragments_digest != expected_fragments_digest {
      return Err(D::Error::custom(invalid("retrieval fragment digest")));
    }
    Ok(receipt)
  }
}
