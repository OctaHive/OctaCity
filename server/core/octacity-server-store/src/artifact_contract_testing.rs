use octacity_server_domain::{ArtifactId, ArtifactName, ArtifactUploadId, AttemptNumber, EntityKind, JobId, Timestamp};

use crate::{
  ArtifactContentDigest, ArtifactEvent, ArtifactIdentity, ArtifactMediaType, ArtifactRecord, ArtifactRecordStore,
  ArtifactReportFormat, ArtifactTransitionAuthority, ArtifactType, ArtifactVerificationResult, BeginArtifactUpload,
  IdempotencyKey, LeaseAccess, LeaseFence, ListPublishedArtifacts, MutationDisposition, ReserveArtifact, StoreError,
  TransitionArtifact, VerifyArtifactUpload,
};

/// Inputs shared by every idempotent Artifact upload persistence contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactUploadStoreContractFixture {
  /// Fresh logical identity.
  pub artifact_id: ArtifactId,
  /// Different fresh identity used to prove exact replay ignores regenerated IDs.
  pub replay_artifact_id: ArtifactId,
  /// Fresh physical upload identity.
  pub upload_id: ArtifactUploadId,
  /// Different fresh identity used on exact replay.
  pub replay_upload_id: ArtifactUploadId,
  /// Current fenced Lease authority.
  pub lease: LeaseAccess,
  /// Leased Job identity.
  pub job_id: JobId,
  /// Leased Attempt number.
  pub attempt: AttemptNumber,
  /// Build used by the visible-output query.
  pub build_id: octacity_server_domain::BuildId,
  /// Reservation time.
  pub reserved_at: Timestamp,
  /// Initial capability expiry.
  pub capability_expires_at: Timestamp,
  /// Verification transition time.
  pub verification_at: Timestamp,
  /// Rejection transition time.
  pub rejection_at: Timestamp,
  /// Publication transition time.
  pub publication_at: Timestamp,
}

/// Inputs shared by every logical Artifact persistence adapter contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactRecordStoreContractFixture {
  /// Immutable identity reserved by the contract.
  pub identity: ArtifactIdentity,
  /// Different identity used to exercise the Job/name uniqueness constraint.
  pub duplicate_artifact_id: ArtifactId,
  /// Current fenced Lease authority.
  pub lease: LeaseAccess,
  /// Reservation time.
  pub reserved_at: Timestamp,
  /// Verification start time.
  pub verification_at: Timestamp,
  /// Publication time.
  pub publication_at: Timestamp,
  /// Time immediately before retention permits expiry.
  pub early_expiry_at: Timestamp,
  /// Time at which retention permits expiry.
  pub expiry_at: Timestamp,
  /// Physical deletion completion time.
  pub deletion_at: Timestamp,
}

/// Verifies fencing, immutable identity, transitions, and uniqueness for one adapter.
pub async fn verify_artifact_record_store_contract<S>(store: &S, fixture: ArtifactRecordStoreContractFixture)
where
  S: ArtifactRecordStore,
{
  let mut fenced = fixture.lease;
  fenced.fence = LeaseFence::from_bytes([0xff; 32]);
  assert_eq!(
    store
      .reserve_artifact(ReserveArtifact {
        identity: fixture.identity.clone(),
        lease: fenced,
        reserved_at: fixture.reserved_at,
      })
      .await
      .unwrap_err(),
    StoreError::Fenced {
      lease: fixture.lease.lease_id
    }
  );

  let pending = store
    .reserve_artifact(ReserveArtifact {
      identity: fixture.identity.clone(),
      lease: fixture.lease,
      reserved_at: fixture.reserved_at,
    })
    .await
    .unwrap();
  assert_eq!(store.artifact(fixture.identity.artifact_id).await.unwrap(), pending);

  let duplicate_identity = ArtifactIdentity {
    artifact_id: fixture.duplicate_artifact_id,
    ..fixture.identity.clone()
  };
  assert_eq!(
    store
      .reserve_artifact(ReserveArtifact {
        identity: duplicate_identity,
        lease: fixture.lease,
        reserved_at: fixture.reserved_at,
      })
      .await
      .unwrap_err(),
    StoreError::Duplicate {
      entity: EntityKind::Artifact
    }
  );

  let mut changed_identity = fixture.identity.clone();
  changed_identity.digest = ArtifactContentDigest::from_bytes([0xbb; 32]);
  assert_eq!(
    store
      .transition_artifact(TransitionArtifact {
        expected_identity: changed_identity,
        expected_version: pending.version(),
        event: ArtifactEvent::BeginVerification,
        authority: ArtifactTransitionAuthority::Lease(fixture.lease),
        transitioned_at: fixture.verification_at,
      })
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Artifact
    }
  );

  let verifying = transition(
    store,
    &pending,
    ArtifactEvent::BeginVerification,
    fixture.lease,
    fixture.verification_at,
  )
  .await;
  let published = transition(
    store,
    &verifying,
    ArtifactEvent::Publish,
    fixture.lease,
    fixture.publication_at,
  )
  .await;
  assert!(published.state().is_visible());
  assert_eq!(published.identity(), &fixture.identity);

  assert_eq!(
    store
      .transition_artifact(TransitionArtifact {
        expected_identity: fixture.identity.clone(),
        expected_version: published.version(),
        event: ArtifactEvent::Expire,
        authority: ArtifactTransitionAuthority::Retention,
        transitioned_at: fixture.early_expiry_at,
      })
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Artifact
    }
  );
  let expired = store
    .transition_artifact(TransitionArtifact {
      expected_identity: fixture.identity.clone(),
      expected_version: published.version(),
      event: ArtifactEvent::Expire,
      authority: ArtifactTransitionAuthority::Retention,
      transitioned_at: fixture.expiry_at,
    })
    .await
    .unwrap();
  let deleted = store
    .transition_artifact(TransitionArtifact {
      expected_identity: fixture.identity,
      expected_version: expired.version(),
      event: ArtifactEvent::Delete,
      authority: ArtifactTransitionAuthority::Retention,
      transitioned_at: fixture.deletion_at,
    })
    .await
    .unwrap();
  assert_eq!(store.artifact(deleted.identity().artifact_id).await.unwrap(), deleted);
}

/// Verifies atomic reservation, exact replay, verification recovery, publication, and visibility.
pub async fn verify_artifact_upload_store_contract<S>(store: &S, fixture: ArtifactUploadStoreContractFixture)
where
  S: ArtifactRecordStore,
{
  let request = BeginArtifactUpload {
    artifact_id: fixture.artifact_id,
    upload_id: fixture.upload_id,
    idempotency_key: IdempotencyKey::new("output:report").unwrap(),
    lease: fixture.lease,
    job_id: fixture.job_id,
    attempt: fixture.attempt,
    logical_name: ArtifactName::new("reports/results.xml").unwrap(),
    producer_run_id: 17,
    producer_task_id: 23,
    artifact_type: ArtifactType::Report(ArtifactReportFormat::new("junit/custom-v2").unwrap()),
    media_type: ArtifactMediaType::new("application/xml").unwrap(),
    transport_media_type: ArtifactMediaType::new("application/xml").unwrap(),
    size_bytes: 42,
    digest: ArtifactContentDigest::from_bytes([0x42; 32]),
    reserved_at: fixture.reserved_at,
    capability_expires_at: fixture.capability_expires_at,
  };
  let applied = store.begin_artifact_upload(request.clone()).await.unwrap();
  assert_eq!(applied.disposition, MutationDisposition::Applied);

  let replayed = store
    .begin_artifact_upload(BeginArtifactUpload {
      artifact_id: fixture.replay_artifact_id,
      upload_id: fixture.replay_upload_id,
      ..request.clone()
    })
    .await
    .unwrap();
  assert_eq!(replayed.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed.upload, applied.upload);

  let mut conflicting = request.clone();
  conflicting.digest = ArtifactContentDigest::from_bytes([0x43; 32]);
  assert_eq!(
    store.begin_artifact_upload(conflicting).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::ArtifactUpload
    }
  );

  let verification = VerifyArtifactUpload {
    upload_id: fixture.upload_id,
    lease: fixture.lease,
    job_id: fixture.job_id,
    attempt: fixture.attempt,
    observed_at: fixture.verification_at,
  };
  let verifying = store.begin_artifact_verification(verification).await.unwrap();
  assert_eq!(verifying.artifact.state(), crate::ArtifactState::Verifying);
  assert_eq!(
    store.begin_artifact_verification(verification).await.unwrap(),
    verifying
  );
  let pending = store
    .finish_artifact_verification(
      VerifyArtifactUpload {
        observed_at: fixture.rejection_at,
        ..verification
      },
      ArtifactVerificationResult::Rejected,
    )
    .await
    .unwrap();
  assert_eq!(pending.artifact.state(), crate::ArtifactState::Pending);

  let verification = VerifyArtifactUpload {
    observed_at: fixture.publication_at,
    ..verification
  };
  store.begin_artifact_verification(verification).await.unwrap();
  let published = store
    .finish_artifact_verification(verification, ArtifactVerificationResult::Verified)
    .await
    .unwrap();
  assert!(published.artifact.state().is_visible());
  assert_eq!(
    store
      .finish_artifact_verification(verification, ArtifactVerificationResult::Verified)
      .await
      .unwrap(),
    published
  );
  assert_eq!(store.published_artifact(fixture.artifact_id).await.unwrap(), published);
  assert_eq!(
    store
      .list_published_artifacts(ListPublishedArtifacts {
        build_id: fixture.build_id,
        limit: 10,
      })
      .await
      .unwrap(),
    vec![published]
  );
}

async fn transition<S>(
  store: &S,
  current: &ArtifactRecord,
  event: ArtifactEvent,
  access: LeaseAccess,
  at: Timestamp,
) -> ArtifactRecord
where
  S: ArtifactRecordStore,
{
  store
    .transition_artifact(TransitionArtifact {
      expected_identity: current.identity().clone(),
      expected_version: current.version(),
      event,
      authority: ArtifactTransitionAuthority::Lease(access),
      transitioned_at: at,
    })
    .await
    .unwrap()
}
