use std::{collections::BTreeMap, fmt::Debug, str::FromStr};

use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, BuildId, ImmutableRevision, SourceReference, Timestamp, TriggerId,
  TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_trigger::{
  MAX_TRIGGER_METADATA_BYTES, MAX_TRIGGER_METADATA_ENTRIES, NormalizedTriggerOccurrence, TriggerCausality,
  TriggerCause, TriggerDefinitionRef, TriggerEventKind, TriggerInputError, TriggerKind, TriggerMetadata, TriggerTarget,
};
use serde_json::{Value, json};

#[test]
fn every_origin_normalizes_to_one_strict_provider_neutral_shape() {
  let manual = root(
    1,
    "manual:request-1",
    TriggerCause::Manual {},
    TriggerMetadata::default(),
  );
  let scheduled = root(
    2,
    "schedule:1700000000000",
    TriggerCause::Scheduled {},
    TriggerMetadata::default(),
  );
  let external = root(
    3,
    "delivery:provider-42",
    TriggerCause::External {
      integration_id: id(20),
      repository_id: id(21),
      event_kind: TriggerEventKind::new("repository.push").unwrap(),
      reference: Some(SourceReference::new("refs/heads/main").unwrap()),
      revision: Some(ImmutableRevision::new("0123456789abcdef").unwrap()),
    },
    TriggerMetadata::new(BTreeMap::from([("actor_display".to_owned(), json!("builder"))])).unwrap(),
  );
  let internal_id = id::<TriggerOccurrenceId>(4);
  let internal = NormalizedTriggerOccurrence::derived(
    internal_id,
    definition(),
    target(),
    TriggerIdentity::new("build-terminal:40:succeeded").unwrap(),
    TriggerCause::Internal {
      source_build_id: id::<BuildId>(40),
      event_kind: TriggerEventKind::new("build.succeeded").unwrap(),
    },
    TriggerCausality::derived(manual.id, manual.id, 1),
    time(400),
  )
  .unwrap();

  for (occurrence, kind) in [
    (manual, TriggerKind::Manual),
    (scheduled, TriggerKind::Scheduled),
    (external, TriggerKind::External),
    (internal, TriggerKind::Internal),
  ] {
    assert_eq!(occurrence.cause.kind(), kind);
    occurrence.validate().unwrap();
    let encoded = serde_json::to_vec(&occurrence).unwrap();
    assert_eq!(
      serde_json::from_slice::<NormalizedTriggerOccurrence>(&encoded).unwrap(),
      occurrence
    );
  }
}

#[test]
fn rejects_unknown_occurrence_and_cause_fields() {
  let occurrence = root(
    1,
    "external:delivery-1",
    TriggerCause::External {
      integration_id: id(20),
      repository_id: id(21),
      event_kind: TriggerEventKind::new("repository.push").unwrap(),
      reference: None,
      revision: None,
    },
    TriggerMetadata::default(),
  );
  let mut encoded = serde_json::to_value(&occurrence).unwrap();
  encoded["unexpected"] = json!(true);
  assert!(serde_json::from_value::<NormalizedTriggerOccurrence>(encoded).is_err());

  let mut encoded = serde_json::to_value(&occurrence).unwrap();
  encoded["cause"]["unexpected"] = json!(true);
  assert!(serde_json::from_value::<NormalizedTriggerOccurrence>(encoded).is_err());

  for cause in [TriggerCause::Manual {}, TriggerCause::Scheduled {}] {
    let mut encoded = serde_json::to_value(root(2, "strict-cause", cause, TriggerMetadata::default())).unwrap();
    encoded["cause"]["unexpected"] = json!(true);
    assert!(serde_json::from_value::<NormalizedTriggerOccurrence>(encoded).is_err());
  }
}

#[test]
fn deduplication_is_source_scoped_and_independent_of_occurrence_identity() {
  let first = root(
    1,
    "delivery:stable",
    TriggerCause::Manual {},
    TriggerMetadata::default(),
  );
  let second = root(
    2,
    "delivery:stable",
    TriggerCause::Manual {},
    TriggerMetadata::default(),
  );
  let other_source = root(3, "delivery:other", TriggerCause::Manual {}, TriggerMetadata::default());

  assert_eq!(first.deduplication_key(), second.deduplication_key());
  assert_ne!(first.deduplication_key(), other_source.deduplication_key());
  assert_eq!(first.target, target());
}

#[test]
fn causality_and_source_specific_fields_are_revalidated_after_mutation() {
  let root_occurrence = root(1, "manual:1", TriggerCause::Manual {}, TriggerMetadata::default());
  assert_eq!(
    NormalizedTriggerOccurrence::derived(
      id(2),
      definition(),
      target(),
      TriggerIdentity::new("invalid-derived-root").unwrap(),
      TriggerCause::Manual {},
      TriggerCausality::derived(root_occurrence.id, root_occurrence.id, 1),
      time(2),
    )
    .unwrap_err(),
    TriggerInputError::InvalidCausality
  );

  let metadata = TriggerMetadata::new(BTreeMap::from([("provider".to_owned(), json!("value"))])).unwrap();
  assert_eq!(
    NormalizedTriggerOccurrence::root(
      id(3),
      definition(),
      target(),
      TriggerIdentity::new("manual:metadata").unwrap(),
      TriggerCause::Manual {},
      metadata,
      time(3),
    )
    .unwrap_err(),
    TriggerInputError::UnexpectedProviderMetadata
  );

  let external = root(
    4,
    "external:invalid-reference",
    TriggerCause::External {
      integration_id: id(20),
      repository_id: id(21),
      event_kind: TriggerEventKind::new("repository.push").unwrap(),
      reference: Some(SourceReference::new("refs/heads/main").unwrap()),
      revision: None,
    },
    TriggerMetadata::default(),
  );
  let mut encoded = serde_json::to_value(external).unwrap();
  encoded["cause"]["reference"] = json!("");
  assert!(serde_json::from_value::<NormalizedTriggerOccurrence>(encoded).is_err());
}

#[test]
fn provider_metadata_enforces_entry_and_encoded_byte_bounds() {
  let too_many = (0..=MAX_TRIGGER_METADATA_ENTRIES)
    .map(|index| (format!("key-{index}"), Value::Null))
    .collect();
  assert_eq!(
    TriggerMetadata::new(too_many).unwrap_err(),
    TriggerInputError::InvalidMetadata
  );

  let too_large = BTreeMap::from([("value".to_owned(), json!("x".repeat(MAX_TRIGGER_METADATA_BYTES)))]);
  assert_eq!(
    TriggerMetadata::new(too_large).unwrap_err(),
    TriggerInputError::MetadataTooLarge
  );
}

fn root(
  occurrence: u128,
  identity: &str,
  cause: TriggerCause,
  metadata: TriggerMetadata,
) -> NormalizedTriggerOccurrence {
  NormalizedTriggerOccurrence::root(
    id(occurrence),
    definition(),
    target(),
    TriggerIdentity::new(identity).unwrap(),
    cause,
    metadata,
    time(i64::try_from(occurrence).unwrap()),
  )
  .unwrap()
}

fn definition() -> TriggerDefinitionRef {
  TriggerDefinitionRef {
    id: id::<TriggerId>(10),
    version: TriggerVersion::INITIAL,
  }
}

fn target() -> TriggerTarget {
  TriggerTarget {
    configuration_id: id::<BuildConfigurationId>(11),
    configuration_version: BuildConfigurationVersion::INITIAL,
  }
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

fn id<T>(value: u128) -> T
where
  T: FromStr,
  T::Err: Debug,
{
  format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
}
