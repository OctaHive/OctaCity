use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{ContextManifestId, FactoryDigest, FactoryError, FactoryKey, FactorySafeText};

use super::common::*;

/// One immutable, subject-bound input selected for a reasoning call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContextManifestEntry {
  source_kind: ContextSourceKind,
  logical_identity: FactoryKey,
  subject: FactoryTaskSubject,
  source: FactoryContextReference,
  content_digest: FactoryDigest,
  encoded_size: u64,
  inclusion_reason: FactorySafeText,
  inclusion_reason_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl ContextManifestEntry {
  /// Constructs one bounded entry and validates its exact subject binding.
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    source_kind: ContextSourceKind,
    logical_identity: FactoryKey,
    subject: FactoryTaskSubject,
    source: FactoryContextReference,
    content_digest: FactoryDigest,
    encoded_size: u64,
    inclusion_reason: FactorySafeText,
    provenance_digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    validate_encoded_size(encoded_size)?;
    validate_context_source(&subject, source_kind, &source, content_digest, encoded_size)?;
    let inclusion_reason_digest = safe_text_digest(
      "octacity.factory.context-manifest.inclusion-reason.v1",
      &inclusion_reason,
    );
    Ok(Self {
      source_kind,
      logical_identity,
      subject,
      source,
      content_digest,
      encoded_size,
      inclusion_reason,
      inclusion_reason_digest,
      provenance_digest,
    })
  }

  /// Returns the declared source kind.
  #[must_use]
  pub const fn source_kind(&self) -> ContextSourceKind {
    self.source_kind
  }

  /// Returns the stable logical input identity.
  #[must_use]
  pub const fn logical_identity(&self) -> &FactoryKey {
    &self.logical_identity
  }

  /// Returns the exact subject binding.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the retained source reference.
  #[must_use]
  pub const fn source(&self) -> &FactoryContextReference {
    &self.source
  }

  /// Returns the exact selected content digest.
  #[must_use]
  pub const fn content_digest(&self) -> FactoryDigest {
    self.content_digest
  }

  /// Returns the exact selected encoded size.
  #[must_use]
  pub const fn encoded_size(&self) -> u64 {
    self.encoded_size
  }

  /// Returns the bounded reason this input was included.
  #[must_use]
  pub const fn inclusion_reason(&self) -> &FactorySafeText {
    &self.inclusion_reason
  }

  /// Returns the exact inclusion-reason digest.
  #[must_use]
  pub const fn inclusion_reason_digest(&self) -> FactoryDigest {
    self.inclusion_reason_digest
  }

  /// Returns the exact producer provenance digest.
  #[must_use]
  pub const fn provenance_digest(&self) -> FactoryDigest {
    self.provenance_digest
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextManifestEntryWire {
  source_kind: ContextSourceKind,
  logical_identity: FactoryKey,
  subject: FactoryTaskSubject,
  source: FactoryContextReference,
  content_digest: FactoryDigest,
  encoded_size: u64,
  inclusion_reason: FactorySafeText,
  inclusion_reason_digest: FactoryDigest,
  provenance_digest: FactoryDigest,
}

impl<'de> Deserialize<'de> for ContextManifestEntry {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = ContextManifestEntryWire::deserialize(deserializer)?;
    let expected_reason_digest = wire.inclusion_reason_digest;
    let entry = Self::new(
      wire.source_kind,
      wire.logical_identity,
      wire.subject,
      wire.source,
      wire.content_digest,
      wire.encoded_size,
      wire.inclusion_reason,
      wire.provenance_digest,
    )
    .map_err(D::Error::custom)?;
    if entry.inclusion_reason_digest != expected_reason_digest {
      return Err(D::Error::custom(invalid("context inclusion-reason digest")));
    }
    Ok(entry)
  }
}

/// Immutable canonically ordered input manifest for one reasoning call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContextManifest {
  schema_version: u16,
  id: ContextManifestId,
  subject: FactoryTaskSubject,
  construction_policy_digest: FactoryDigest,
  entries: Vec<ContextManifestEntry>,
}

impl ContextManifest {
  /// Constructs a non-empty subject-consistent manifest in canonical entry order.
  pub fn new(
    id: ContextManifestId,
    subject: FactoryTaskSubject,
    construction_policy_digest: FactoryDigest,
    entries: Vec<ContextManifestEntry>,
  ) -> Result<Self, FactoryError> {
    validate_count(&entries, 1, MAX_CONTEXT_MANIFEST_ENTRIES, "context manifest entries")?;
    if entries.iter().any(|entry| entry.subject != subject) {
      return Err(FactoryError::InconsistentSubject);
    }
    if entries
      .iter()
      .all(|entry| entry.source_kind == ContextSourceKind::RepositoryFragment)
    {
      return Err(invalid("context mandatory entries"));
    }
    let total_bytes = entries.iter().try_fold(0_u64, |total, entry| {
      total
        .checked_add(entry.encoded_size)
        .ok_or_else(|| invalid("context manifest bytes"))
    })?;
    if total_bytes > MAX_CONTEXT_MANIFEST_BYTES {
      return Err(invalid("context manifest bytes"));
    }
    let mut ordered_entries = entries
      .into_iter()
      .map(|entry| canonical_json(&entry, "context entry ordering").map(|key| (key, entry)))
      .collect::<Result<Vec<_>, _>>()?;
    ordered_entries.sort_by(|left, right| left.0.cmp(&right.0));
    let entries = ordered_entries.into_iter().map(|(_, entry)| entry).collect::<Vec<_>>();
    if entries
      .iter()
      .map(|entry| (entry.source_kind, entry.logical_identity.clone()))
      .collect::<HashSet<_>>()
      .len()
      != entries.len()
    {
      return Err(invalid("context manifest logical identity"));
    }
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      id,
      subject,
      construction_policy_digest,
      entries,
    })
  }

  /// Returns the immutable manifest identity.
  #[must_use]
  pub const fn id(&self) -> ContextManifestId {
    self.id
  }

  /// Returns the exact common subject.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the immutable context-construction policy digest.
  #[must_use]
  pub const fn construction_policy_digest(&self) -> FactoryDigest {
    self.construction_policy_digest
  }

  /// Returns entries in canonical stable order.
  #[must_use]
  pub fn entries(&self) -> &[ContextManifestEntry] {
    &self.entries
  }

  /// Returns deterministic canonical JSON bytes.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    canonical_json(self, "context manifest serialization")
  }

  /// Returns the content-addressed canonical manifest digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    record_digest("octacity.factory.context-manifest.v1", self)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextManifestWire {
  schema_version: u16,
  id: ContextManifestId,
  subject: FactoryTaskSubject,
  construction_policy_digest: FactoryDigest,
  entries: Vec<ContextManifestEntry>,
}

impl<'de> Deserialize<'de> for ContextManifest {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = ContextManifestWire::deserialize(deserializer)?;
    require_version(wire.schema_version).map_err(D::Error::custom)?;
    Self::new(wire.id, wire.subject, wire.construction_policy_digest, wire.entries).map_err(D::Error::custom)
  }
}

fn validate_context_source(
  subject: &FactoryTaskSubject,
  source_kind: ContextSourceKind,
  source: &FactoryContextReference,
  content_digest: FactoryDigest,
  encoded_size: u64,
) -> Result<(), FactoryError> {
  match source {
    FactoryContextReference::Artifact(reference) => {
      if matches!(
        source_kind,
        ContextSourceKind::RepositoryRange | ContextSourceKind::RepositoryFragment
      ) || reference.content_digest() != content_digest
        || reference.encoded_size() != encoded_size
      {
        return Err(invalid("context artifact reference"));
      }
    }
    FactoryContextReference::RepositoryRange(reference) => {
      if source_kind != ContextSourceKind::RepositoryRange
        || reference.repository_id() != subject.exact().repository_id()
        || !subject.permits_revision(reference.revision())
      {
        return Err(FactoryError::InconsistentSubject);
      }
    }
    FactoryContextReference::RepositoryFragment(reference) => {
      if source_kind != ContextSourceKind::RepositoryFragment
        || reference.artifact().content_digest() != content_digest
        || reference.artifact().encoded_size() != encoded_size
      {
        return Err(invalid("context repository fragment"));
      }
      if reference.subject() != subject
        || reference.range().repository_id() != subject.exact().repository_id()
        || !subject.permits_revision(reference.range().revision())
      {
        return Err(FactoryError::InconsistentSubject);
      }
    }
  }
  Ok(())
}
