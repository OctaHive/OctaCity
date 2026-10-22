//! Project-owned skill publication over ordinary immutable input objects.
use crate::*;
use octacity_server_domain::{ArtifactId, ArtifactUploadId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryRoleSkillBundle, ImmutableReference};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Reviewed metadata only; private bytes remain in the existing object store.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSkillImportIntent {
  /// Owning Project; import does not create Work or execution authority.
  pub project_id: ProjectId,
  /// Complete immutable relative file inventory.
  pub bundle: FactoryRoleSkillBundle,
  /// Ordinary upload identities for each exact Artifact reference.
  pub uploads: BTreeMap<ArtifactId, ArtifactUploadId>,
  /// Replay key partitioned by Project and accepted management security scope.
  pub idempotency_key: IdempotencyKey,
}
/// Reservation/publication request. Time comes from the trusted application clock.
#[derive(Clone, Debug)]
pub struct PublishProjectSkillImport {
  /// Complete reviewed immutable intent.
  pub intent: ProjectSkillImportIntent,
  /// Trusted observation time.
  pub now: Timestamp,
}
/// Original import receipt and pending cleanup inventory, with no private content.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSkillImportRecord {
  /// Exact reviewed metadata.
  pub intent: ProjectSkillImportIntent,
  /// Original staging time, preserved by retries.
  pub reserved_at: Timestamp,
  /// Exclusive staging deadline; publication cannot race expired-input cleanup.
  pub staging_until: Timestamp,
  /// Atomic visibility point, present only for the complete verified tree.
  pub published_at: Option<Timestamp>,
}
impl ProjectSkillImportRecord {
  /// Exact bounded transfer identities; published inputs have profile-owned retention.
  pub fn inputs(&self) -> Result<Vec<PublishedExecutionInput>, StoreError> {
    self
      .intent
      .bundle
      .files()
      .values()
      .map(|file| {
        Ok(PublishedExecutionInput {
          artifact_id: file.artifact_id(),
          upload_id: *self
            .intent
            .uploads
            .get(&file.artifact_id())
            .ok_or_else(skill_import_invalid)?,
          size_bytes: file.encoded_size(),
          digest: ArtifactContentDigest::from_bytes(file.content_digest().as_bytes()),
          media_type: ArtifactMediaType::new("application/octet-stream").map_err(|_| skill_import_invalid())?,
          expires_at: self.published_at.is_none().then_some(self.staging_until),
        })
      })
      .collect()
  }
  /// Validates complete one-to-one metadata before reserving physical identities.
  pub fn reserve(intent: ProjectSkillImportIntent, now: Timestamp) -> Result<Self, StoreError> {
    if intent.uploads.len() != intent.bundle.files().len()
      || intent
        .bundle
        .files()
        .values()
        .map(|file| file.artifact_id())
        .collect::<BTreeSet<_>>()
        .len()
        != intent.bundle.files().len()
    {
      return Err(skill_import_invalid());
    }
    let staging_until = now
      .unix_millis()
      .checked_add(PROJECT_INPUT_PREPARATION_DEADLINE_MILLIS)
      .and_then(|time| Timestamp::from_unix_millis(time).ok())
      .ok_or_else(skill_import_invalid)?;
    let record = Self {
      intent,
      reserved_at: now,
      staging_until,
      published_at: None,
    };
    record.inputs()?;
    Ok(record)
  }
  /// Identical retries share immutable object writes; elapsed staging cannot be revived.
  pub fn require_live(&self, intent: &ProjectSkillImportIntent, now: Timestamp) -> Result<(), StoreError> {
    if &self.intent != intent || now < self.reserved_at || (self.published_at.is_none() && now >= self.staging_until) {
      return Err(skill_import_conflict());
    }
    Ok(())
  }
}
/// Safe exact skill choices over the existing bounded role/profile page contract.
pub type ProjectSkillImportPage = FactoryRoleCatalogPage<ImmutableReference, ImmutableReference>;

/// Narrow atomic import owner; no alternate byte store, Build or provider dependency.
#[async_trait::async_trait]
pub trait ProjectSkillImportStore: Send + Sync {
  /// Lists only complete owned bundle references, ordered by identity/version.
  async fn list_project_skill_imports(
    &self,
    project: ProjectId,
    after: Option<ImmutableReference>,
    limit: u16,
  ) -> Result<ProjectSkillImportPage, StoreError>;
  /// Reserves the entire cleanup inventory before external writes; exact replay returns current state.
  async fn reserve_project_skill_import(
    &self,
    request: ManagementMutation<PublishProjectSkillImport>,
  ) -> Result<ProjectSkillImportRecord, StoreError>;
  /// Publishes the complete tree and original receipt together after verified byte writes.
  async fn publish_project_skill_import(
    &self,
    request: ManagementMutation<PublishProjectSkillImport>,
  ) -> Result<ProjectSkillImportRecord, StoreError>;
  /// Reads only an exact complete publication in its owning Project; never falls back to ambient skills.
  async fn project_skill_import(
    &self,
    project: ProjectId,
    reference: &ImmutableReference,
  ) -> Result<Option<ProjectSkillImportRecord>, StoreError>;
}
pub(crate) fn skill_import_invalid() -> StoreError {
  StoreError::invalid(
    StoreOperation::PublishProjectSkillImport,
    StoreInputError::InvalidProjectSkillImport,
  )
}
pub(crate) fn skill_import_conflict() -> StoreError {
  StoreError::Conflict {
    entity: octacity_server_domain::EntityKind::Artifact,
  }
}
