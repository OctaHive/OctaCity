use std::{fmt::Debug, str::FromStr};

use octacity_server_domain::{
  AgentId, AgentName, AgentVersion, ArtifactId, ArtifactName, ArtifactPolicy, ArtifactUploadId, ArtifactVersion,
  AttemptId, AttemptNumber, AttemptVersion, BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion,
  BuildId, BuildVersion, DomainError, DomainValueError, EntityKind, ImmutableRevision, IntegrationId, IntegrationName,
  IntegrationVersion, JobId, JobName, JobVersion, LeaseId, LeaseVersion, MAX_ARTIFACT_NAME_BYTES,
  MAX_CANONICAL_JSON_DEPTH, MAX_IMMUTABLE_REVISION_BYTES, MAX_PIPELINE_NODE_ID_BYTES, MAX_RESOURCE_NAME_BYTES,
  MAX_SOURCE_REFERENCE_BYTES, MAX_TIMESTAMP_MILLIS, MAX_TRIGGER_IDENTITY_BYTES, MIN_TIMESTAMP_MILLIS, NetworkHost,
  PipelineId, PipelineName, PipelineNodeId, PipelineVersion, PoolId, PoolName, PoolVersion, ProjectId, ProjectName,
  ProjectPolicyVersion, ProjectVersion, RepositoryId, RepositoryLocator, RepositoryVersion, SourceReference,
  TextErrorKind, Timestamp, TransitionError, TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
  VersionErrorKind, canonicalize_json,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;

const CANONICAL_ID: &str = "01890f3e-7b8c-7d6e-8f90-123456789abc";

fn assert_id_round_trip<T>()
where
  T: Debug + DeserializeOwned + Eq + FromStr<Err = DomainValueError> + Serialize + ToString,
{
  let value = T::from_str(CANONICAL_ID).expect("canonical identifier");
  assert_eq!(value.to_string(), CANONICAL_ID);
  let json = serde_json::to_string(&value).expect("serialize identifier");
  assert_eq!(json, format!(r#""{CANONICAL_ID}""#));
  assert_eq!(serde_json::from_str::<T>(&json).expect("deserialize identifier"), value);
}

#[test]
fn every_opaque_identifier_has_one_canonical_serialization() {
  assert_id_round_trip::<ProjectId>();
  assert_id_round_trip::<BuildConfigurationId>();
  assert_id_round_trip::<PipelineId>();
  assert_id_round_trip::<RepositoryId>();
  assert_id_round_trip::<BuildId>();
  assert_id_round_trip::<AttemptId>();
  assert_id_round_trip::<JobId>();
  assert_id_round_trip::<PoolId>();
  assert_id_round_trip::<AgentId>();
  assert_id_round_trip::<LeaseId>();
  assert_id_round_trip::<ArtifactId>();
  assert_id_round_trip::<ArtifactUploadId>();
  assert_id_round_trip::<IntegrationId>();
  assert_id_round_trip::<TriggerId>();
  assert_id_round_trip::<TriggerOccurrenceId>();
}

#[test]
fn identifiers_reject_nil_malformed_and_noncanonical_text() {
  assert!(matches!(
    ProjectId::from_str("00000000-0000-0000-0000-000000000000"),
    Err(DomainValueError::InvalidIdentifier { .. })
  ));
  assert!(ProjectId::from_str("not-a-uuid").is_err());
  assert!(ProjectId::from_str("01890F3E-7B8C-7D6E-8F90-123456789ABC").is_err());
  assert!(ProjectId::from_str("01890f3e7b8c7d6e8f90123456789abc").is_err());
  assert!(serde_json::from_str::<ProjectId>(r#""not-a-uuid""#).is_err());

  let generated = ProjectId::generate();
  assert!(!generated.as_uuid().is_nil());
  assert_eq!(
    ProjectId::from_str(&generated.to_string()).expect("generated identifier"),
    generated
  );
}

fn assert_name_round_trip<T>(value: &str)
where
  T: Debug + DeserializeOwned + Eq + FromStr<Err = DomainValueError> + Serialize + ToString,
{
  let name = T::from_str(value).expect("valid bounded name");
  let encoded = serde_json::to_string(&name).expect("serialize name");
  assert_eq!(serde_json::from_str::<T>(&encoded).expect("deserialize name"), name);
  assert_eq!(name.to_string(), value);
}

#[test]
fn typed_names_round_trip_without_losing_unicode() {
  assert_name_round_trip::<ProjectName>("Проект сборки");
  assert_name_round_trip::<BuildConfigurationName>("release");
  assert_name_round_trip::<PipelineName>("main pipeline");
  assert_name_round_trip::<JobName>("linux tests");
  assert_name_round_trip::<PoolName>("linux-amd64");
  assert_name_round_trip::<AgentName>("builder-01");
  assert_name_round_trip::<ArtifactName>("dist/отчёт.json");
  assert_name_round_trip::<IntegrationName>("Gerrit production");
}

#[test]
fn names_enforce_utf8_byte_and_content_boundaries() {
  assert!(ProjectName::new("a".repeat(MAX_RESOURCE_NAME_BYTES)).is_ok());
  assert!(ProjectName::new("é".repeat(MAX_RESOURCE_NAME_BYTES / 2)).is_ok());
  assert!(matches!(
    ProjectName::new("a".repeat(MAX_RESOURCE_NAME_BYTES + 1)),
    Err(DomainValueError::InvalidName {
      reason: TextErrorKind::TooLong,
      ..
    })
  ));
  assert!(matches!(
    ProjectName::new(""),
    Err(DomainValueError::InvalidName {
      reason: TextErrorKind::Empty,
      ..
    })
  ));
  assert!(matches!(
    ProjectName::new(" leading"),
    Err(DomainValueError::InvalidName {
      reason: TextErrorKind::SurroundingWhitespace,
      ..
    })
  ));
  assert!(ProjectName::new("line\nbreak").is_err());
  assert!(ArtifactName::new("a".repeat(MAX_ARTIFACT_NAME_BYTES)).is_ok());
  assert!(ArtifactName::new("a".repeat(MAX_ARTIFACT_NAME_BYTES + 1)).is_err());
  assert!(serde_json::from_str::<ProjectName>(r#"" trailing ""#).is_err());
}

#[test]
fn trigger_identity_is_bounded_and_serializable() {
  let identity = TriggerIdentity::new("github:installation-42:delivery-99").expect("trigger identity");
  let encoded = serde_json::to_string(&identity).expect("serialize trigger identity");
  assert_eq!(encoded, r#""github:installation-42:delivery-99""#);
  assert_eq!(
    serde_json::from_str::<TriggerIdentity>(&encoded).expect("deserialize trigger identity"),
    identity
  );
  assert!(TriggerIdentity::new("x".repeat(MAX_TRIGGER_IDENTITY_BYTES)).is_ok());
  assert!(TriggerIdentity::new("x".repeat(MAX_TRIGGER_IDENTITY_BYTES + 1)).is_err());
  assert!(TriggerIdentity::new("").is_err());
  assert!(TriggerIdentity::new(" delivery").is_err());
  assert!(TriggerIdentity::new("delivery\n1").is_err());
}

#[test]
fn source_references_share_one_bounded_value_type() {
  let reference = SourceReference::new("refs/heads/main").unwrap();
  assert_eq!(reference.as_str(), "refs/heads/main");
  assert!(SourceReference::new("x".repeat(MAX_SOURCE_REFERENCE_BYTES)).is_ok());
  assert!(SourceReference::new("x".repeat(MAX_SOURCE_REFERENCE_BYTES + 1)).is_err());
  assert!(serde_json::from_str::<SourceReference>(r#"" leading""#).is_err());
}

#[test]
fn immutable_revisions_share_one_bounded_value_type() {
  let revision = ImmutableRevision::new("0123456789abcdef").unwrap();
  assert_eq!(revision.as_str(), "0123456789abcdef");
  assert!(ImmutableRevision::new("x".repeat(MAX_IMMUTABLE_REVISION_BYTES)).is_ok());
  assert!(ImmutableRevision::new("x".repeat(MAX_IMMUTABLE_REVISION_BYTES + 1)).is_err());
  assert!(serde_json::from_str::<ImmutableRevision>(r#"" leading""#).is_err());
}

#[test]
fn repository_locators_are_provider_neutral_but_never_local_or_credential_bearing() {
  for value in ["octahive/octacity", "https://example.test/team/repository.git"] {
    assert_eq!(RepositoryLocator::new(value).unwrap().as_str(), value);
  }
  for value in [
    "/srv/repository",
    "~/repository",
    r"C:\repository",
    "C:repository",
    "../repository",
    "team/../repository",
    "https://user@example.test/repository.git",
    "file:///srv/repository",
    "https://example.test/%2e%2e/repository",
  ] {
    assert!(
      RepositoryLocator::new(value).is_err(),
      "accepted unsafe locator {value}"
    );
  }
}

#[test]
fn network_hosts_are_exact_and_canonical() {
  assert_eq!(NetworkHost::new("EXAMPLE.COM").unwrap().as_str(), "example.com");
  assert_eq!(NetworkHost::new("2001:db8::1").unwrap().as_str(), "2001:db8::1");
  for invalid in [
    "",
    "not a host",
    "https://example.com/path",
    "*.example.com",
    "example.com:443",
  ] {
    assert!(NetworkHost::new(invalid).is_err(), "accepted {invalid}");
  }
}

#[test]
fn artifact_policy_validates_its_shared_shape() {
  let valid = ArtifactPolicy {
    artifact_count: 1,
    artifact_bytes: 1024,
    report_count: 0,
    report_bytes: 0,
    single_output_bytes: 512,
  };
  assert!(valid.validate().is_ok());
  assert!(
    ArtifactPolicy {
      artifact_bytes: 0,
      ..valid
    }
    .validate()
    .is_err()
  );
}

#[test]
fn canonical_json_rejects_excessive_nesting() {
  let mut value = json!(null);
  for _ in 0..=MAX_CANONICAL_JSON_DEPTH {
    value = json!([value]);
  }
  assert!(canonicalize_json(value).is_err());
}

#[test]
fn pipeline_node_identity_has_a_portable_alphabet() {
  let node = PipelineNodeId::new("linux.release_1-tests").expect("pipeline node identity");
  let encoded = serde_json::to_string(&node).expect("serialize pipeline node identity");
  assert_eq!(
    serde_json::from_str::<PipelineNodeId>(&encoded).expect("deserialize pipeline node identity"),
    node
  );
  assert!(PipelineNodeId::new("a".repeat(MAX_PIPELINE_NODE_ID_BYTES)).is_ok());
  assert!(PipelineNodeId::new("a".repeat(MAX_PIPELINE_NODE_ID_BYTES + 1)).is_err());
  assert!(PipelineNodeId::new("").is_err());
  assert!(PipelineNodeId::new("-starts-with-separator").is_err());
  assert!(PipelineNodeId::new("contains/slash").is_err());
  assert!(PipelineNodeId::new("contains space").is_err());
  assert!(PipelineNodeId::new("кириллица").is_err());
}

macro_rules! assert_version {
  ($type:ty) => {{
    let initial = <$type>::INITIAL;
    assert_eq!(initial.get(), 1);
    assert_eq!(initial.next().expect("next version").get(), 2);
    assert_eq!(serde_json::to_string(&initial).expect("serialize version"), "1");
    assert_eq!(
      serde_json::from_str::<$type>("1").expect("deserialize version"),
      initial
    );
    assert!(serde_json::from_str::<$type>("0").is_err());
    assert!(matches!(
      <$type>::new(u64::MAX).expect("maximum version").next(),
      Err(DomainValueError::InvalidVersion {
        reason: VersionErrorKind::Overflow,
        ..
      })
    ));
  }};
}

#[test]
fn entity_versions_are_positive_bounded_and_typed() {
  assert_version!(ProjectVersion);
  assert_version!(ProjectPolicyVersion);
  assert_version!(BuildConfigurationVersion);
  assert_version!(PipelineVersion);
  assert_version!(RepositoryVersion);
  assert_version!(BuildVersion);
  assert_version!(AttemptVersion);
  assert_version!(JobVersion);
  assert_version!(PoolVersion);
  assert_version!(AgentVersion);
  assert_version!(LeaseVersion);
  assert_version!(ArtifactVersion);
  assert_version!(IntegrationVersion);
  assert_version!(TriggerVersion);
}

#[test]
fn attempt_numbers_are_positive_monotonic_and_serializable() {
  assert_eq!(AttemptNumber::FIRST.get(), 1);
  assert_eq!(AttemptNumber::FIRST.next().expect("second attempt").get(), 2);
  assert_eq!(
    serde_json::to_string(&AttemptNumber::FIRST).expect("serialize attempt"),
    "1"
  );
  assert_eq!(
    serde_json::from_str::<AttemptNumber>("1").expect("deserialize attempt"),
    AttemptNumber::FIRST
  );
  assert!(AttemptNumber::new(0).is_err());
  assert!(matches!(
    AttemptNumber::new(u64::MAX).expect("maximum attempt").next(),
    Err(DomainValueError::InvalidAttemptNumber {
      reason: VersionErrorKind::Overflow
    })
  ));
}

#[test]
fn timestamps_enforce_a_stable_utc_millisecond_range() {
  for value in [MIN_TIMESTAMP_MILLIS, 0, MAX_TIMESTAMP_MILLIS] {
    let timestamp = Timestamp::from_unix_millis(value).expect("bounded timestamp");
    assert_eq!(timestamp.unix_millis(), value);
    let encoded = serde_json::to_string(&timestamp).expect("serialize timestamp");
    assert_eq!(
      serde_json::from_str::<Timestamp>(&encoded).expect("deserialize timestamp"),
      timestamp
    );
  }
  assert!(Timestamp::from_unix_millis(MIN_TIMESTAMP_MILLIS - 1).is_err());
  assert!(Timestamp::from_unix_millis(MAX_TIMESTAMP_MILLIS + 1).is_err());
  assert!(serde_json::from_str::<Timestamp>(&format!("{}", MAX_TIMESTAMP_MILLIS + 1)).is_err());
}

#[test]
fn domain_errors_have_stable_typed_serialization() {
  let error = DomainError::VersionConflict {
    entity: EntityKind::Project,
    expected: 4,
    actual: 5,
  };
  let encoded = serde_json::to_value(&error).expect("serialize domain error");
  assert_eq!(
    encoded,
    json!({"code": "version_conflict", "entity": "project", "expected": 4, "actual": 5})
  );
  assert_eq!(
    serde_json::from_value::<DomainError>(encoded).expect("deserialize domain error"),
    error
  );
  assert!(
    serde_json::from_value::<DomainError>(json!({"code": "not_found", "entity": "project", "extra": true})).is_err()
  );

  let value_error: DomainError = DomainValueError::InvalidName {
    entity: EntityKind::Project,
    reason: TextErrorKind::Empty,
  }
  .into();
  assert!(matches!(value_error, DomainError::InvalidValue { .. }));
}

#[test]
fn transition_errors_retain_typed_context_and_map_to_safe_domain_errors() {
  let error = TransitionError::new(EntityKind::Build, "succeeded", "start");
  assert_eq!(error.entity(), EntityKind::Build);
  assert_eq!(error.state(), &"succeeded");
  assert_eq!(error.event(), &"start");
  assert_eq!(
    DomainError::from(error),
    DomainError::InvalidTransition {
      entity: EntityKind::Build
    }
  );
}
