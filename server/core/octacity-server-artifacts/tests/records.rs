use std::{fmt::Debug, str::FromStr};

use octacity_server_artifacts::{
  ArtifactContentDigest, ArtifactEvent, ArtifactIdentity, ArtifactMediaType, ArtifactRecord, ArtifactRecordError,
  ArtifactReportFormat, ArtifactRetentionPolicy, ArtifactState, ArtifactType,
};
use octacity_server_domain::{ArtifactName, ArtifactVersion, Timestamp};

#[test]
fn lifecycle_preserves_identity_and_controls_visibility_by_retention() {
  let identity = identity(ArtifactRetentionPolicy::DeleteAfter(time(40)));
  let pending = ArtifactRecord::pending(identity.clone(), time(10)).unwrap();
  let verifying = transition(&pending, &identity, ArtifactEvent::BeginVerification, time(20));
  let published = transition(&verifying, &identity, ArtifactEvent::Publish, time(30));

  assert_eq!(published.state(), ArtifactState::Published);
  assert!(published.state().is_visible());
  assert_eq!(published.published_at(), Some(time(30)));
  assert_eq!(published.identity(), &identity);
  assert_eq!(
    published.transition(&identity, published.version(), ArtifactEvent::Expire, time(39)),
    Err(ArtifactRecordError::RetentionNotReached)
  );

  let expired = transition(&published, &identity, ArtifactEvent::Expire, time(40));
  assert!(!expired.state().is_visible());
  let deleted = transition(&expired, &identity, ArtifactEvent::Delete, time(41));
  assert_eq!(deleted.state(), ArtifactState::Deleted);
  assert_eq!(deleted.deleted_at(), Some(time(41)));
  assert_eq!(deleted.identity(), &identity);
}

#[test]
fn immutable_identity_and_version_preconditions_reject_mutation_attempts() {
  let identity = identity(ArtifactRetentionPolicy::Keep);
  let pending = ArtifactRecord::pending(identity.clone(), time(10)).unwrap();
  let mut changed = identity.clone();
  changed.digest = ArtifactContentDigest::from_bytes([0xbb; 32]);

  assert_eq!(
    pending.transition(
      &changed,
      ArtifactVersion::INITIAL,
      ArtifactEvent::BeginVerification,
      time(20)
    ),
    Err(ArtifactRecordError::ImmutableIdentityMismatch)
  );
  assert_eq!(
    pending.transition(
      &identity,
      ArtifactVersion::new(2).unwrap(),
      ArtifactEvent::BeginVerification,
      time(20)
    ),
    Err(ArtifactRecordError::VersionConflict)
  );
  assert_eq!(pending.identity(), &identity);
  assert_eq!(pending.state(), ArtifactState::Pending);
}

#[test]
fn bounded_metadata_and_persisted_shapes_are_revalidated() {
  assert_eq!(
    ArtifactContentDigest::from_lower_hex(&"A".repeat(64)),
    Err(ArtifactRecordError::InvalidDigest)
  );
  assert_eq!(
    ArtifactMediaType::new("text/plain\nprivate"),
    Err(ArtifactRecordError::InvalidMediaType)
  );
  assert_eq!(
    ArtifactReportFormat::new(" "),
    Err(ArtifactRecordError::InvalidReportFormat)
  );

  let identity = identity(ArtifactRetentionPolicy::DeleteAfter(time(20)));
  assert_eq!(
    ArtifactRecord::restore(
      identity,
      ArtifactState::Published,
      ArtifactVersion::INITIAL,
      time(10),
      None,
      None
    ),
    Err(ArtifactRecordError::InvalidPersistedShape)
  );
}

fn transition(
  record: &ArtifactRecord,
  identity: &ArtifactIdentity,
  event: ArtifactEvent,
  at: Timestamp,
) -> ArtifactRecord {
  record.transition(identity, record.version(), event, at).unwrap()
}

fn identity(retention: ArtifactRetentionPolicy) -> ArtifactIdentity {
  ArtifactIdentity {
    artifact_id: id(1),
    build_id: id(2),
    attempt_id: id(3),
    job_id: id(4),
    lease_id: id(5),
    logical_name: ArtifactName::new("dist/results.xml").unwrap(),
    artifact_type: ArtifactType::Report(ArtifactReportFormat::new("junit").unwrap()),
    media_type: ArtifactMediaType::new("application/xml").unwrap(),
    size_bytes: 42,
    digest: ArtifactContentDigest::from_bytes([0xaa; 32]),
    retention,
  }
}

fn time(milliseconds: i64) -> Timestamp {
  Timestamp::from_unix_millis(milliseconds).unwrap()
}

fn id<T>(value: u64) -> T
where
  T: FromStr,
  T::Err: Debug,
{
  format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
}
