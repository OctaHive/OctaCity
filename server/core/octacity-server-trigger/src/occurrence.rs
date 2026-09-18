use std::{collections::BTreeMap, fmt};

use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, BuildId, IntegrationId, RepositoryId, Timestamp, TriggerId,
  TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_domain::{ImmutableRevision, SourceReference};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::Value;
use thiserror::Error;

/// Maximum provider-owned metadata entries retained on one occurrence.
pub const MAX_TRIGGER_METADATA_ENTRIES: usize = 64;
/// Maximum encoded bytes retained as provider-owned occurrence metadata.
pub const MAX_TRIGGER_METADATA_BYTES: usize = 32 * 1_024;
/// Maximum UTF-8 bytes in one provider metadata key.
pub const MAX_TRIGGER_METADATA_KEY_BYTES: usize = 128;
/// Maximum UTF-8 bytes in one normalized event classification.
pub const MAX_TRIGGER_EVENT_KIND_BYTES: usize = 128;
/// Maximum UTF-8 bytes in one normalized immutable revision.
pub const MAX_TRIGGER_REVISION_BYTES: usize = octacity_server_domain::MAX_IMMUTABLE_REVISION_BYTES;

/// Invalid normalized Trigger input rejected before persistence or evaluation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TriggerInputError {
  /// A normalized event classification is empty, malformed, or too long.
  #[error("the normalized trigger event kind is invalid")]
  InvalidEventKind,
  /// An immutable source revision is empty, malformed, or too long.
  #[error("the normalized trigger revision is invalid")]
  InvalidRevision,
  /// Provider metadata has too many entries or an invalid key.
  #[error("the normalized trigger metadata shape is invalid")]
  InvalidMetadata,
  /// Encoded provider metadata exceeds its contract bound.
  #[error("the normalized trigger metadata exceeds its byte bound")]
  MetadataTooLarge,
  /// Root and derived occurrence identities do not form a valid lineage.
  #[error("the normalized trigger causality is invalid")]
  InvalidCausality,
  /// Provider metadata is attached to a non-external occurrence.
  #[error("provider metadata is valid only for an external trigger")]
  UnexpectedProviderMetadata,
}

/// One of the four normalized origins understood by the Trigger Engine.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
  /// Trusted-network management command.
  Manual,
  /// Durable time-based occurrence.
  Scheduled,
  /// Authenticated provider event.
  External,
  /// Server-generated domain event.
  Internal,
}

impl TriggerKind {
  /// Returns the stable persistence and diagnostic representation.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Manual => "manual",
      Self::Scheduled => "scheduled",
      Self::External => "external",
      Self::Internal => "internal",
    }
  }
}

/// Exact immutable Trigger definition selected during normalization.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerDefinitionRef {
  /// Stable Trigger identity.
  pub id: TriggerId,
  /// Exact immutable Trigger version.
  pub version: TriggerVersion,
}

/// Exact Build Configuration selected for occurrence evaluation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerTarget {
  /// Stable Build Configuration identity.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable Build Configuration version.
  pub configuration_version: BuildConfigurationVersion,
}

/// Bounded provider-neutral event classification.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TriggerEventKind(String);

impl TriggerEventKind {
  /// Constructs a non-empty normalized event classification.
  pub fn new(value: impl Into<String>) -> Result<Self, TriggerInputError> {
    let value = value.into();
    if valid_text(&value, MAX_TRIGGER_EVENT_KIND_BYTES) {
      Ok(Self(value))
    } else {
      Err(TriggerInputError::InvalidEventKind)
    }
  }

  /// Borrows the normalized event classification.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for TriggerEventKind {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl Serialize for TriggerEventKind {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for TriggerEventKind {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// Bounded opaque metadata supplied by an authenticated provider adapter.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TriggerMetadata(BTreeMap<String, Value>);

impl TriggerMetadata {
  /// Validates provider metadata without interpreting provider-owned values.
  pub fn new(entries: BTreeMap<String, Value>) -> Result<Self, TriggerInputError> {
    validate_metadata(&entries)?;
    Ok(Self(entries))
  }

  /// Returns true when no provider metadata was retained.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  /// Iterates provider-owned metadata in canonical lexical key order.
  pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
    self.0.iter().map(|(key, value)| (key.as_str(), value))
  }

  fn validate(&self) -> Result<(), TriggerInputError> {
    validate_metadata(&self.0)
  }
}

impl Serialize for TriggerMetadata {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    self.0.serialize(serializer)
  }
}

impl<'de> Deserialize<'de> for TriggerMetadata {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    BTreeMap::<String, Value>::deserialize(deserializer)
      .and_then(|entries| Self::new(entries).map_err(D::Error::custom))
  }
}

/// Provider-neutral facts explaining why an occurrence exists.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TriggerCause {
  /// Trusted-network management intent.
  Manual {},
  /// A persisted schedule reached its recorded occurrence time.
  Scheduled {},
  /// An authenticated external repository event.
  External {
    /// Server-owned integration that authenticated the delivery.
    integration_id: IntegrationId,
    /// Repository named by the normalized event.
    repository_id: RepositoryId,
    /// Provider-neutral event classification.
    event_kind: TriggerEventKind,
    /// Optional mutable reference reported by the provider.
    reference: Option<SourceReference>,
    /// Optional immutable revision reported by the provider.
    revision: Option<ImmutableRevision>,
  },
  /// A server-owned domain event derived from an earlier Build.
  Internal {
    /// Build whose durable event produced this occurrence.
    source_build_id: BuildId,
    /// Documented server event classification.
    event_kind: TriggerEventKind,
  },
}

impl TriggerCause {
  /// Returns the normalized origin kind.
  #[must_use]
  pub const fn kind(&self) -> TriggerKind {
    match self {
      Self::Manual {} => TriggerKind::Manual,
      Self::Scheduled {} => TriggerKind::Scheduled,
      Self::External { .. } => TriggerKind::External,
      Self::Internal { .. } => TriggerKind::Internal,
    }
  }

  fn validate(&self) -> Result<(), TriggerInputError> {
    Ok(())
  }
}

/// Stable lineage carried by every normalized occurrence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerCausality {
  /// First occurrence in the causal chain.
  pub root_occurrence_id: TriggerOccurrenceId,
  /// Direct parent occurrence for internal derivation.
  pub parent_occurrence_id: Option<TriggerOccurrenceId>,
  /// Number of internal derivations from the root occurrence.
  pub depth: u16,
}

impl TriggerCausality {
  /// Constructs lineage for a manual, scheduled, or external root occurrence.
  #[must_use]
  pub const fn root(occurrence_id: TriggerOccurrenceId) -> Self {
    Self {
      root_occurrence_id: occurrence_id,
      parent_occurrence_id: None,
      depth: 0,
    }
  }

  /// Constructs lineage for an internal occurrence derived from a parent.
  #[must_use]
  pub const fn derived(
    root_occurrence_id: TriggerOccurrenceId,
    parent_occurrence_id: TriggerOccurrenceId,
    depth: u16,
  ) -> Self {
    Self {
      root_occurrence_id,
      parent_occurrence_id: Some(parent_occurrence_id),
      depth,
    }
  }

  fn validate(&self, occurrence_id: TriggerOccurrenceId, kind: TriggerKind) -> Result<(), TriggerInputError> {
    let valid = match kind {
      TriggerKind::Internal => {
        self.depth > 0
          && self.root_occurrence_id != occurrence_id
          && self.parent_occurrence_id.is_some_and(|parent| parent != occurrence_id)
      }
      TriggerKind::Manual | TriggerKind::Scheduled | TriggerKind::External => {
        self.root_occurrence_id == occurrence_id && self.parent_occurrence_id.is_none() && self.depth == 0
      }
    };
    if valid {
      Ok(())
    } else {
      Err(TriggerInputError::InvalidCausality)
    }
  }
}

/// Stable source-scoped key that deduplicates one Trigger occurrence.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TriggerDeduplicationKey {
  /// Exact Trigger definition whose occurrence is deduplicated.
  pub trigger: TriggerDefinitionRef,
  /// Stable identity assigned by the source-specific normalizer.
  pub identity: TriggerIdentity,
}

/// Complete provider-neutral Trigger occurrence accepted by the Trigger Engine.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedTriggerOccurrence {
  /// Stable occurrence identity.
  pub id: TriggerOccurrenceId,
  /// Exact immutable Trigger definition.
  pub trigger: TriggerDefinitionRef,
  /// Exact immutable Build Configuration selected for evaluation.
  pub target: TriggerTarget,
  /// Stable source-scoped deduplication identity.
  pub deduplication_identity: TriggerIdentity,
  /// Provider-neutral initiating facts.
  pub cause: TriggerCause,
  /// Stable root and parent occurrence identities.
  pub causality: TriggerCausality,
  /// Bounded opaque metadata retained only for authenticated external events.
  pub provider_metadata: TriggerMetadata,
  /// Time observed at the source, independent of server acceptance time.
  pub source_time: Timestamp,
}

/// Stable caller intent used to compare deduplicated Trigger occurrences.
///
/// A manual occurrence excludes its server-observed arrival time because a
/// retry after a lost response observes a new time but retains the same
/// source-scoped identity. Provider and schedule timestamps remain part of
/// their immutable source facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TriggerOccurrenceIntent {
  id: TriggerOccurrenceId,
  trigger: TriggerDefinitionRef,
  target: TriggerTarget,
  deduplication_identity: TriggerIdentity,
  cause: TriggerCause,
  causality: TriggerCausality,
  provider_metadata: TriggerMetadata,
  source_time: Option<Timestamp>,
}

impl NormalizedTriggerOccurrence {
  /// Normalizes a manual, scheduled, or external root occurrence.
  pub fn root(
    id: TriggerOccurrenceId,
    trigger: TriggerDefinitionRef,
    target: TriggerTarget,
    deduplication_identity: TriggerIdentity,
    cause: TriggerCause,
    provider_metadata: TriggerMetadata,
    source_time: Timestamp,
  ) -> Result<Self, TriggerInputError> {
    Self::validated(Self {
      id,
      trigger,
      target,
      deduplication_identity,
      cause,
      causality: TriggerCausality::root(id),
      provider_metadata,
      source_time,
    })
  }

  /// Normalizes an internal occurrence derived from an earlier occurrence.
  pub fn derived(
    id: TriggerOccurrenceId,
    trigger: TriggerDefinitionRef,
    target: TriggerTarget,
    deduplication_identity: TriggerIdentity,
    cause: TriggerCause,
    causality: TriggerCausality,
    source_time: Timestamp,
  ) -> Result<Self, TriggerInputError> {
    Self::validated(Self {
      id,
      trigger,
      target,
      deduplication_identity,
      cause,
      causality,
      provider_metadata: TriggerMetadata::default(),
      source_time,
    })
  }

  /// Revalidates every normalized fact at an adapter seam.
  pub fn validate(&self) -> Result<(), TriggerInputError> {
    self.cause.validate()?;
    self.provider_metadata.validate()?;
    let kind = self.cause.kind();
    self.causality.validate(self.id, kind)?;
    if kind != TriggerKind::External && !self.provider_metadata.is_empty() {
      return Err(TriggerInputError::UnexpectedProviderMetadata);
    }
    Ok(())
  }

  /// Returns the stable source-scoped deduplication key.
  #[must_use]
  pub fn deduplication_key(&self) -> TriggerDeduplicationKey {
    TriggerDeduplicationKey {
      trigger: self.trigger,
      identity: self.deduplication_identity.clone(),
    }
  }

  /// Returns the stable intent compared for exact idempotent replay.
  #[must_use]
  pub fn intent(&self) -> TriggerOccurrenceIntent {
    TriggerOccurrenceIntent {
      id: self.id,
      trigger: self.trigger,
      target: self.target,
      deduplication_identity: self.deduplication_identity.clone(),
      cause: self.cause.clone(),
      causality: self.causality,
      provider_metadata: self.provider_metadata.clone(),
      source_time: (self.cause.kind() != TriggerKind::Manual).then_some(self.source_time),
    }
  }

  fn validated(occurrence: Self) -> Result<Self, TriggerInputError> {
    occurrence.validate()?;
    Ok(occurrence)
  }
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
  !value.is_empty() && value.len() <= maximum_bytes && value.trim() == value && !value.chars().any(char::is_control)
}

fn validate_metadata(entries: &BTreeMap<String, Value>) -> Result<(), TriggerInputError> {
  if entries.len() > MAX_TRIGGER_METADATA_ENTRIES
    || entries
      .keys()
      .any(|key| !valid_text(key, MAX_TRIGGER_METADATA_KEY_BYTES))
  {
    return Err(TriggerInputError::InvalidMetadata);
  }
  if !serde_json::to_vec(entries).is_ok_and(|encoded| encoded.len() <= MAX_TRIGGER_METADATA_BYTES) {
    return Err(TriggerInputError::MetadataTooLarge);
  }
  Ok(())
}
