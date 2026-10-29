//! Pure identity and lease rules shared by purposes in the existing Project input journal.
use crate::{ArtifactMediaType, FactoryInputObject, FactoryInputRole, IdempotencyKey, ManagementSecurityScope};
use octacity_server_domain::{ArtifactId, ArtifactUploadId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest};

pub(crate) fn operation_id(project: ProjectId, scope: &ManagementSecurityScope, key: &IdempotencyKey) -> FactoryDigest {
  FactoryDigest::sha256(
    "octacity.project-text.v1",
    &[
      project.to_string().as_bytes(),
      scope.as_str().as_bytes(),
      key.as_str().as_bytes(),
    ],
  )
}
pub(crate) fn input_object(id: FactoryDigest, role: FactoryInputRole, bytes: &[u8]) -> Option<FactoryInputObject> {
  let name = match role {
    FactoryInputRole::Task => "task",
    FactoryInputRole::Acceptance => "acceptance",
    FactoryInputRole::Document => "document",
  };
  let namespace = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &id.as_bytes());
  let artifact = ArtifactId::from_uuid(uuid::Uuid::new_v5(&namespace, name.as_bytes())).ok()?;
  Some(FactoryInputObject {
    role,
    artifact: FactoryArtifactReference::new(artifact, FactoryDigest::content_sha256(bytes), bytes.len() as u64).ok()?,
    upload_id: ArtifactUploadId::from_uuid(uuid::Uuid::new_v5(&artifact.as_uuid(), b"project-input.v1")).ok()?,
    media_type: ArtifactMediaType::new("text/plain").ok()?,
  })
}

/// A policy value projected from journal headers; it does not retain or publish state.
#[derive(Clone, Copy)]
pub(crate) struct InputLease {
  pub generation: u64,
  pub attempts: u16,
  pub claimed_at: Timestamp,
  pub lease_until: Timestamp,
  pub preparation_deadline: Timestamp,
  pub expires_at: Timestamp,
}
impl InputLease {
  pub fn reserve(now: Timestamp) -> Option<Self> {
    Some(Self {
      generation: 1,
      attempts: 1,
      claimed_at: now,
      lease_until: advance(now, crate::project_text_input::PREPARATION_LEASE_MILLIS)?,
      preparation_deadline: advance(now, crate::PROJECT_INPUT_PREPARATION_DEADLINE_MILLIS)?,
      expires_at: advance(now, crate::MAX_SOURCE_RETENTION_MILLIS as i64)?,
    })
  }
  pub fn is_current(self, generation: u64, now: Timestamp) -> bool {
    generation == self.generation
      && now >= self.claimed_at
      && now < self.lease_until
      && now < self.preparation_deadline
      && now < self.expires_at
  }
  pub fn reclaim(mut self, now: Timestamp) -> Option<Self> {
    if now < self.claimed_at
      || now < self.lease_until
      || now >= self.preparation_deadline
      || self.attempts >= crate::project_text_input::MAX_PREPARATION_ATTEMPTS
    {
      return None;
    }
    self.generation = self.generation.checked_add(1)?;
    self.attempts += 1;
    self.claimed_at = now;
    self.lease_until =
      advance(now, crate::project_text_input::PREPARATION_LEASE_MILLIS)?.min(self.preparation_deadline);
    Some(self)
  }
}
fn advance(now: Timestamp, millis: i64) -> Option<Timestamp> {
  Timestamp::from_unix_millis(now.unix_millis().checked_add(millis)?).ok()
}
