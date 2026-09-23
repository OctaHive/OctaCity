use std::{collections::BTreeMap, sync::Mutex};

use async_trait::async_trait;
use octacity_server_artifacts::{ArtifactEvent, ArtifactIdentity, ArtifactRetentionPolicy, ArtifactState};
use octacity_server_domain::{ArtifactId, ArtifactUploadId, EntityKind, Timestamp};

use crate::{
  ArtifactRecord, ArtifactRecordStore, ArtifactTransitionAuthority, ArtifactUploadRecord, ArtifactVerificationResult,
  BeginArtifactUpload, BeginArtifactUploadOutcome, LeaseAccess, ListPublishedArtifacts, MutationDisposition,
  ReserveArtifact, StoreError, StoreInputError, StoreOperation, TransitionArtifact, VerifyArtifactUpload,
  artifact_authority_is_valid,
};

/// Seeded current-Lease facts for deterministic Artifact store contracts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactLeaseFixture {
  /// Exact current fenced authority.
  pub access: LeaseAccess,
  /// Build owning the leased Job.
  pub build_id: octacity_server_domain::BuildId,
  /// Attempt owning the leased Job.
  pub attempt_id: octacity_server_domain::AttemptId,
  /// Positive Attempt number bound into the Lease.
  pub attempt: octacity_server_domain::AttemptNumber,
  /// Leased Job.
  pub job_id: octacity_server_domain::JobId,
  /// Authoritative Lease expiry.
  pub expires_at: Timestamp,
}

#[derive(Default)]
struct ArtifactMemoryState {
  records: BTreeMap<ArtifactId, ArtifactRecord>,
  job_names: BTreeMap<(octacity_server_domain::JobId, String), ArtifactId>,
  uploads: BTreeMap<ArtifactUploadId, ArtifactUploadRecord>,
  upload_keys: BTreeMap<(octacity_server_domain::LeaseId, String), ArtifactUploadId>,
}

/// Deterministic logical Artifact adapter used by the shared behavioral contract.
pub struct InMemoryArtifactRecordStore {
  lease: ArtifactLeaseFixture,
  state: Mutex<ArtifactMemoryState>,
}

impl InMemoryArtifactRecordStore {
  /// Creates an empty adapter with one current Lease authority.
  #[must_use]
  pub fn new(lease: ArtifactLeaseFixture) -> Self {
    Self {
      lease,
      state: Mutex::new(ArtifactMemoryState::default()),
    }
  }

  fn require_lease(
    &self,
    access: LeaseAccess,
    identity: &crate::ArtifactIdentity,
    observed_at: Timestamp,
  ) -> Result<(), StoreError> {
    if access != self.lease.access
      || identity.lease_id != access.lease_id
      || identity.build_id != self.lease.build_id
      || identity.attempt_id != self.lease.attempt_id
      || identity.job_id != self.lease.job_id
    {
      return Err(StoreError::Fenced { lease: access.lease_id });
    }
    if observed_at >= self.lease.expires_at {
      return Err(StoreError::Expired { lease: access.lease_id });
    }
    Ok(())
  }
}

#[async_trait]
impl ArtifactRecordStore for InMemoryArtifactRecordStore {
  async fn reserve_artifact(&self, request: ReserveArtifact) -> Result<ArtifactRecord, StoreError> {
    self.require_lease(request.lease, &request.identity, request.reserved_at)?;
    let record = request
      .pending_record()
      .map_err(|_| invalid(StoreOperation::ReserveArtifact))?;
    let key = (request.identity.job_id, request.identity.logical_name.to_string());
    let mut state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    if state.records.contains_key(&request.identity.artifact_id) || state.job_names.contains_key(&key) {
      return Err(StoreError::Duplicate {
        entity: EntityKind::Artifact,
      });
    }
    state.job_names.insert(key, request.identity.artifact_id);
    state.records.insert(request.identity.artifact_id, record.clone());
    Ok(record)
  }

  async fn artifact(&self, artifact_id: ArtifactId) -> Result<ArtifactRecord, StoreError> {
    self
      .state
      .lock()
      .map_err(|_| StoreError::Unavailable)?
      .records
      .get(&artifact_id)
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Artifact,
      })
  }

  async fn transition_artifact(&self, request: TransitionArtifact) -> Result<ArtifactRecord, StoreError> {
    if let ArtifactTransitionAuthority::Lease(access) = request.authority {
      self.require_lease(access, &request.expected_identity, request.transitioned_at)?;
    }
    let mut state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    let current = state
      .records
      .get(&request.expected_identity.artifact_id)
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Artifact,
      })?;
    if !artifact_authority_is_valid(request.authority, current.identity(), current.state(), request.event) {
      return Err(invalid(StoreOperation::TransitionArtifact));
    }
    let updated = current
      .transition(
        &request.expected_identity,
        request.expected_version,
        request.event,
        request.transitioned_at,
      )
      .map_err(|_| StoreError::Conflict {
        entity: EntityKind::Artifact,
      })?;
    state
      .records
      .insert(request.expected_identity.artifact_id, updated.clone());
    Ok(updated)
  }

  async fn begin_artifact_upload(
    &self,
    request: BeginArtifactUpload,
  ) -> Result<BeginArtifactUploadOutcome, StoreError> {
    let placeholder = ArtifactIdentity {
      artifact_id: request.artifact_id,
      build_id: self.lease.build_id,
      attempt_id: self.lease.attempt_id,
      job_id: request.job_id,
      lease_id: request.lease.lease_id,
      logical_name: request.logical_name.clone(),
      artifact_type: request.artifact_type.clone(),
      media_type: request.media_type.clone(),
      size_bytes: request.size_bytes,
      digest: request.digest,
      retention: ArtifactRetentionPolicy::Keep,
    };
    self.require_lease(request.lease, &placeholder, request.reserved_at)?;
    if request.job_id != self.lease.job_id
      || request.attempt != self.lease.attempt
      || request.capability_expires_at <= request.reserved_at
    {
      return Err(invalid(StoreOperation::BeginArtifactUpload));
    }
    let key = (request.lease.lease_id, request.idempotency_key.as_str().to_owned());
    let mut state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    if let Some(existing_id) = state.upload_keys.get(&key) {
      let existing = state
        .uploads
        .get(existing_id)
        .expect("upload key must reference a record");
      if upload_matches(existing, &request) {
        return Ok(BeginArtifactUploadOutcome {
          upload: existing.clone(),
          disposition: MutationDisposition::Replayed,
        });
      }
      return Err(StoreError::Conflict {
        entity: EntityKind::ArtifactUpload,
      });
    }
    let name_key = (request.job_id, request.logical_name.to_string());
    if state.records.contains_key(&request.artifact_id)
      || state.uploads.contains_key(&request.upload_id)
      || state.job_names.contains_key(&name_key)
    {
      return Err(StoreError::Duplicate {
        entity: EntityKind::Artifact,
      });
    }
    let artifact = ArtifactRecord::pending(placeholder, request.reserved_at)
      .map_err(|_| invalid(StoreOperation::BeginArtifactUpload))?;
    let upload = ArtifactUploadRecord {
      upload_id: request.upload_id,
      idempotency_key: request.idempotency_key,
      artifact: artifact.clone(),
      producer_run_id: request.producer_run_id,
      producer_task_id: request.producer_task_id,
      transport_media_type: request.transport_media_type,
      capability_expires_at: request.capability_expires_at,
    };
    state.job_names.insert(name_key, request.artifact_id);
    state.records.insert(request.artifact_id, artifact);
    state.upload_keys.insert(key, request.upload_id);
    state.uploads.insert(request.upload_id, upload.clone());
    Ok(BeginArtifactUploadOutcome {
      upload,
      disposition: MutationDisposition::Applied,
    })
  }

  async fn artifact_upload(&self, upload_id: ArtifactUploadId) -> Result<ArtifactUploadRecord, StoreError> {
    self
      .state
      .lock()
      .map_err(|_| StoreError::Unavailable)?
      .uploads
      .get(&upload_id)
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::ArtifactUpload,
      })
  }

  async fn begin_artifact_verification(
    &self,
    request: VerifyArtifactUpload,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    self.verify_upload(request, None)
  }

  async fn finish_artifact_verification(
    &self,
    request: VerifyArtifactUpload,
    result: ArtifactVerificationResult,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    self.verify_upload(request, Some(result))
  }

  async fn published_artifact(&self, artifact_id: ArtifactId) -> Result<ArtifactUploadRecord, StoreError> {
    self
      .state
      .lock()
      .map_err(|_| StoreError::Unavailable)?
      .uploads
      .values()
      .find(|upload| upload.artifact.identity().artifact_id == artifact_id && upload.artifact.state().is_visible())
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Artifact,
      })
  }

  async fn list_published_artifacts(
    &self,
    query: ListPublishedArtifacts,
  ) -> Result<Vec<ArtifactUploadRecord>, StoreError> {
    if query.limit == 0 || query.limit > crate::MAX_ARTIFACT_PAGE_SIZE {
      return Err(invalid(StoreOperation::ListPublishedArtifacts));
    }
    Ok(
      self
        .state
        .lock()
        .map_err(|_| StoreError::Unavailable)?
        .uploads
        .values()
        .filter(|upload| upload.artifact.identity().build_id == query.build_id && upload.artifact.state().is_visible())
        .take(usize::from(query.limit))
        .cloned()
        .collect(),
    )
  }
}

impl InMemoryArtifactRecordStore {
  fn verify_upload(
    &self,
    request: VerifyArtifactUpload,
    result: Option<ArtifactVerificationResult>,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    let mut state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    let current = state
      .uploads
      .get(&request.upload_id)
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::ArtifactUpload,
      })?;
    self.require_lease(request.lease, current.artifact.identity(), request.observed_at)?;
    if request.job_id != current.artifact.identity().job_id || request.attempt != self.lease.attempt {
      return Err(StoreError::Fenced {
        lease: request.lease.lease_id,
      });
    }
    let event = match (result, current.artifact.state()) {
      (None, ArtifactState::Pending) => Some(ArtifactEvent::BeginVerification),
      (None, ArtifactState::Verifying | ArtifactState::Published) => None,
      (Some(ArtifactVerificationResult::Verified), ArtifactState::Verifying) => Some(ArtifactEvent::Publish),
      (Some(ArtifactVerificationResult::Verified), ArtifactState::Published) => None,
      (Some(ArtifactVerificationResult::Rejected), ArtifactState::Verifying) => Some(ArtifactEvent::VerificationFailed),
      (Some(ArtifactVerificationResult::Rejected), ArtifactState::Pending) => None,
      _ => {
        return Err(StoreError::Conflict {
          entity: EntityKind::ArtifactUpload,
        });
      }
    };
    let mut updated = current;
    if let Some(event) = event {
      updated.artifact = updated
        .artifact
        .transition(
          updated.artifact.identity(),
          updated.artifact.version(),
          event,
          request.observed_at,
        )
        .map_err(|_| StoreError::Conflict {
          entity: EntityKind::ArtifactUpload,
        })?;
      state
        .records
        .insert(updated.artifact.identity().artifact_id, updated.artifact.clone());
      state.uploads.insert(request.upload_id, updated.clone());
    }
    Ok(updated)
  }
}

fn upload_matches(existing: &ArtifactUploadRecord, request: &BeginArtifactUpload) -> bool {
  let identity = existing.artifact.identity();
  identity.lease_id == request.lease.lease_id
    && identity.job_id == request.job_id
    && identity.logical_name == request.logical_name
    && existing.producer_run_id == request.producer_run_id
    && existing.producer_task_id == request.producer_task_id
    && identity.artifact_type == request.artifact_type
    && identity.media_type == request.media_type
    && existing.transport_media_type == request.transport_media_type
    && identity.size_bytes == request.size_bytes
    && identity.digest == request.digest
}

const fn invalid(operation: StoreOperation) -> StoreError {
  StoreError::InvalidInput {
    operation,
    source: StoreInputError::InvalidArtifact,
  }
}

#[cfg(test)]
mod tests {
  use std::{
    future::Future,
    task::{Context, Poll, Waker},
  };

  use octacity_server_domain::ArtifactName;

  use super::*;
  use crate::artifact_contract_testing::{
    ArtifactRecordStoreContractFixture, ArtifactUploadStoreContractFixture, verify_artifact_record_store_contract,
    verify_artifact_upload_store_contract,
  };
  use crate::test_support::{id, time};
  use crate::{
    ArtifactContentDigest, ArtifactIdentity, ArtifactMediaType, ArtifactRetentionPolicy, ArtifactType, LeaseFence,
    RegistrationEpoch,
  };

  #[test]
  fn deterministic_adapter_satisfies_the_artifact_record_contract() {
    let access = LeaseAccess {
      lease_id: id(5),
      fence: LeaseFence::from_bytes([5; 32]),
      agent_id: id(6),
      registration_epoch: RegistrationEpoch::new(1).unwrap(),
    };
    let identity = ArtifactIdentity {
      artifact_id: id(1),
      build_id: id(2),
      attempt_id: id(3),
      job_id: id(4),
      lease_id: access.lease_id,
      logical_name: ArtifactName::new("dist/result.tar").unwrap(),
      artifact_type: ArtifactType::Artifact,
      media_type: ArtifactMediaType::new("application/x-tar").unwrap(),
      size_bytes: 42,
      digest: ArtifactContentDigest::from_bytes([0xaa; 32]),
      retention: ArtifactRetentionPolicy::DeleteAfter(time(5_000)),
    };
    let store = InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
      access,
      build_id: identity.build_id,
      attempt_id: identity.attempt_id,
      attempt: octacity_server_domain::AttemptNumber::new(1).unwrap(),
      job_id: identity.job_id,
      expires_at: time(10_000),
    });
    block_on(verify_artifact_record_store_contract(
      &store,
      ArtifactRecordStoreContractFixture {
        identity: identity.clone(),
        duplicate_artifact_id: id(7),
        lease: access,
        reserved_at: time(1_100),
        verification_at: time(1_300),
        publication_at: time(1_400),
        early_expiry_at: time(4_999),
        expiry_at: time(5_000),
        deletion_at: time(5_100),
      },
    ));
    block_on(verify_artifact_upload_store_contract(
      &store,
      ArtifactUploadStoreContractFixture {
        artifact_id: id(8),
        replay_artifact_id: id(9),
        upload_id: id(10),
        replay_upload_id: id(11),
        lease: access,
        job_id: identity.job_id,
        attempt: octacity_server_domain::AttemptNumber::new(1).unwrap(),
        build_id: identity.build_id,
        reserved_at: time(1_600),
        capability_expires_at: time(2_600),
        verification_at: time(1_700),
        rejection_at: time(1_800),
        publication_at: time(1_900),
      },
    ));
  }

  fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(output) => output,
      Poll::Pending => panic!("deterministic in-memory contract unexpectedly awaited I/O"),
    }
  }
}
