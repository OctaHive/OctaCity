//! Named private documents share the Project text input owner and immutable byte identities.
use crate::IdempotencyKey;
use octacity_server_domain::{ArtifactName, ProjectId, Timestamp};
use octacity_server_factory::FactoryArtifactReference;
use serde::{Deserialize, Serialize};

/// Versioned mapping distinguishing standalone documents from task/acceptance inputs.
pub const PROJECT_CONTEXT_DOCUMENT_MAPPING: &str = "context-document.v1";

/// One reviewed standalone document; it cannot carry a Configuration, Run or execution authority.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDocumentSubmission {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Human name used by authorized document discovery.
  pub name: ArtifactName,
  /// Exact private UTF-8 input, never included in diagnostic output.
  pub content: String,
  /// Project/security-scoped replay key.
  pub idempotency_key: IdempotencyKey,
}
impl std::fmt::Debug for ProjectDocumentSubmission {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ProjectDocumentSubmission")
      .field("project_id", &self.project_id)
      .finish_non_exhaustive()
  }
}
/// Published immutable document metadata; selection grants no execution authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectDocumentResource {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Reviewed human name.
  pub name: ArtifactName,
  /// Exact retained private object, resolved by the existing Project input reader.
  pub artifact: FactoryArtifactReference,
  /// Original trusted publication time.
  pub published_at: Timestamp,
  /// Exclusive retention deadline, unchanged by replay or recovery.
  pub expires_at: Timestamp,
}

use crate::{
  FactoryInputObject, FactoryInputRole, ManagementMutation, ManagementSecurityScope, ProjectTextGuard, StoreError,
  StoreInputError, StoreOperation,
};
use octacity_server_domain::ArtifactId;
use octacity_server_factory::{FactoryDigest, FactoryKey};

impl ProjectDocumentSubmission {
  /// Validates private bytes without assigning instructions, execution permissions or a Work purpose.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.content.trim().is_empty()
      || self.content.len() > crate::MAX_SOURCE_INPUT_BYTES as usize
      || self
        .content
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\r' | '\n'))
    {
      return Err(StoreError::invalid(
        StoreOperation::ReserveProjectTextInput,
        StoreInputError::InvalidProjectTextInput,
      ));
    }
    Ok(())
  }
}
/// Standalone document intent in the shared Project text journal; it has no admission target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDocumentIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Reviewed display name.
  pub name: ArtifactName,
  /// Publisher-owned immutable format, never executable policy.
  pub mapping: FactoryKey,
  /// Original scoped replay key.
  pub idempotency_key: IdempotencyKey,
}
/// One journal entry for a named document, using the existing Project input lifecycle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectDocumentPreparation {
  /// Stable scoped publication identity.
  pub id: FactoryDigest,
  /// Original security partition, retained independently of current grants.
  #[serde(
    serialize_with = "crate::project_text_input::serialize_retained_scope",
    deserialize_with = "crate::project_text_input::deserialize_retained_scope"
  )]
  pub security_scope: ManagementSecurityScope,
  /// Original non-executable document intent.
  pub intent: ProjectDocumentIntent,
  /// Private object identities reserved before I/O.
  pub objects: Vec<FactoryInputObject>,
  /// Current publication owner generation.
  pub generation: u64,
  /// Current owner start time.
  pub claimed_at: Timestamp,
  /// Current exclusive lease deadline.
  pub lease_until: Timestamp,
  /// Original hard preparation deadline.
  pub preparation_deadline: Timestamp,
  /// Bounded consumed preparation attempts.
  pub attempts: u16,
  /// Original reservation time.
  pub reserved_at: Timestamp,
  /// Original private retention deadline.
  pub expires_at: Timestamp,
  /// Complete verified publication visibility point.
  pub published_at: Option<Timestamp>,
}
impl ProjectDocumentPreparation {
  fn lease(&self) -> crate::project_input_policy::InputLease {
    crate::project_input_policy::InputLease {
      generation: self.generation,
      attempts: self.attempts,
      claimed_at: self.claimed_at,
      lease_until: self.lease_until,
      preparation_deadline: self.preparation_deadline,
      expires_at: self.expires_at,
    }
  }
  /// Reserves stable identities in the existing scoped Project text namespace.
  pub fn reserve(
    submission: &ProjectDocumentSubmission,
    scope: ManagementSecurityScope,
    now: Timestamp,
  ) -> Result<Self, StoreError> {
    submission.validate()?;
    let id = crate::project_input_policy::operation_id(submission.project_id, &scope, &submission.idempotency_key);
    let lease = crate::project_input_policy::InputLease::reserve(now).ok_or_else(document_conflict)?;
    let object =
      crate::project_input_policy::input_object(id, FactoryInputRole::Document, submission.content.as_bytes())
        .ok_or_else(document_conflict)?;
    Ok(Self {
      id,
      security_scope: scope,
      intent: ProjectDocumentIntent {
        project_id: submission.project_id,
        name: submission.name.clone(),
        mapping: FactoryKey::new(PROJECT_CONTEXT_DOCUMENT_MAPPING).map_err(|_| document_conflict())?,
        idempotency_key: submission.idempotency_key.clone(),
      },
      objects: vec![object],
      generation: lease.generation,
      attempts: lease.attempts,
      claimed_at: lease.claimed_at,
      lease_until: lease.lease_until,
      preparation_deadline: lease.preparation_deadline,
      reserved_at: now,
      expires_at: lease.expires_at,
      published_at: None,
    })
  }
  /// Exact private replay comparison, without retaining document bodies in metadata.
  pub fn matches_submission(&self, submission: &ProjectDocumentSubmission) -> bool {
    self.intent.project_id == submission.project_id
      && self.intent.name == submission.name
      && self.intent.idempotency_key == submission.idempotency_key
      && self.objects.len() == 1
      && self.objects[0].artifact.encoded_size() == submission.content.len() as u64
      && self.objects[0].artifact.content_digest() == FactoryDigest::content_sha256(submission.content.as_bytes())
  }
  /// Current scoped owner guard, compatible with the shared publication journal.
  pub fn guard(&self, now: Timestamp) -> ProjectTextGuard {
    ProjectTextGuard {
      id: self.id,
      project_id: self.intent.project_id,
      generation: self.generation,
      now,
    }
  }
  /// Validates the current generation and original, non-renewable deadlines.
  pub fn require_owner(&self, guard: &ProjectTextGuard) -> Result<(), StoreError> {
    if guard.id != self.id
      || guard.project_id != self.intent.project_id
      || !self.lease().is_current(guard.generation, guard.now)
    {
      return Err(document_conflict());
    }
    Ok(())
  }
  /// Reclaims only an elapsed owner lease, preserving original object identities and retention.
  pub fn reclaim(&self, now: Timestamp) -> Result<Self, StoreError> {
    let lease = self.lease().reclaim(now).ok_or_else(document_conflict)?;
    let mut next = self.clone();
    next.generation = lease.generation;
    next.attempts = lease.attempts;
    next.claimed_at = lease.claimed_at;
    next.lease_until = lease.lease_until;
    Ok(next)
  }
  /// Public metadata only after complete verified publication.
  pub fn resource(&self) -> Result<ProjectDocumentResource, StoreError> {
    Ok(ProjectDocumentResource {
      project_id: self.intent.project_id,
      name: self.intent.name.clone(),
      artifact: self.objects.first().ok_or_else(document_conflict)?.artifact.clone(),
      published_at: self.published_at.ok_or_else(document_conflict)?,
      expires_at: self.expires_at,
    })
  }
}
/// Document reservation under existing management audit and scoped replay authority.
#[derive(Clone, Debug)]
pub struct ReserveProjectDocument {
  /// Exact reviewed input.
  pub submission: ProjectDocumentSubmission,
  /// Trusted observation time.
  pub now: Timestamp,
}
/// Standalone documents use the same Project input owner, byte store and cleanup journal.
#[async_trait::async_trait]
pub trait ProjectDocumentInputStore: crate::ProjectInputQueryStore {
  /// Lists complete live named documents under visibility applied before page boundaries.
  async fn list_project_documents(&self, request: ListProjectDocuments) -> Result<ProjectDocumentPage, StoreError>;
  /// Looks up the original scoped document operation; another operation kind is a conflict.
  async fn project_document_preparation(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<ProjectDocumentPreparation>, StoreError>;
  /// Reads the original private document only within its scoped operation.
  async fn project_document_content(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<String>, StoreError>;
  /// Reserves stable private identities before byte publication.
  async fn reserve_project_document(
    &self,
    request: ManagementMutation<ReserveProjectDocument>,
  ) -> Result<ProjectDocumentPreparation, StoreError>;
  /// Rechecks exact ownership immediately around private-byte effects.
  async fn require_project_document_owner(&self, guard: ProjectTextGuard) -> Result<(), StoreError>;
  /// Releases only the current owner after a recoverable failure.
  async fn release_project_document(&self, guard: ProjectTextGuard) -> Result<(), StoreError>;
  /// Makes a verified complete document visible; it does not admit Work or create a Build.
  async fn publish_project_document(
    &self,
    request: ManagementMutation<ProjectTextGuard>,
  ) -> Result<ProjectDocumentPreparation, StoreError>;
}
pub(crate) fn document_conflict() -> StoreError {
  StoreError::Conflict {
    entity: octacity_server_domain::EntityKind::Artifact,
  }
}

/// Bounded immutable document discovery; no private bodies or incomplete operations are returned.
#[derive(Clone, Debug)]
pub struct ListProjectDocuments {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exclusive immutable Artifact identity cursor.
  pub after: Option<ArtifactId>,
  /// Positive page size bounded by the existing current-definition limit.
  pub limit: u16,
  /// Current authorization-derived Artifact visibility.
  pub visibility: crate::ArtifactListVisibility,
  /// Trusted time for private retention filtering.
  pub observed_at: Timestamp,
}
/// Safe Project document choices; no total of hidden or expired inputs is disclosed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectDocumentPage {
  /// Visible complete immutable document metadata.
  pub items: Vec<ProjectDocumentResource>,
  /// Exclusive continuation, absent at the end.
  pub next_cursor: Option<ArtifactId>,
}

impl ListProjectDocuments {
  /// Validates the existing discovery ceiling at the storage seam as well as HTTP.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.limit == 0 || self.limit > crate::MAX_CURRENT_DEFINITION_PAGE_SIZE {
      return Err(StoreError::invalid(
        StoreOperation::ListProjectInputDocuments,
        StoreInputError::InvalidCurrentDefinitionPageSize,
      ));
    }
    Ok(())
  }
}
