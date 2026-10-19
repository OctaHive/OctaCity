//! Private source input publication without fabricated Build or Agent provenance.
use crate::{
  FactorySourceStore, MAX_SOURCE_INPUT_BYTES, MAX_SOURCE_RETENTION_MILLIS, StoreError, WorkSourceScanClaim,
  source_conflict,
};
use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, WorkArtifacts, WorkSourceInputId, WorkSourceInputVersion};
use serde::{Deserialize, Serialize};

/// Explicit input document role; shared with Project text preparation.
pub type SourceArtifactRole = crate::FactoryInputRole;
/// Exact private byte identity; shared with Project text preparation.
pub type SourceArtifactObject = crate::FactoryInputObject;
/// Original immutable source materialization, with private retention.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMaterialization {
  /// Stable original preparation operation.
  pub preparation_id: FactoryDigest,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Original input definition.
  pub input_id: WorkSourceInputId,
  /// Original immutable input version.
  pub input_version: WorkSourceInputVersion,
  /// Exact normalized mapping and preparation policy digest.
  pub mapping_digest: FactoryDigest,
  /// Complete task and acceptance metadata; no private byte content.
  pub objects: Vec<SourceArtifactObject>,
  /// Original retention duration, never renewed by retry.
  pub retention_millis: u64,
  /// Original metadata reservation time.
  pub reserved_at: Timestamp,
  /// Exclusive private read and fresh publication deadline.
  pub expires_at: Timestamp,
  /// Safe rejected candidate disposition, without any private provider diagnostics.
  #[serde(default)]
  pub rejection: Option<octacity_server_factory::FactoryKey>,
  /// First completed cleanup; tombstones remain available for reconciliation.
  #[serde(default)]
  pub cleaned_at: Option<Timestamp>,
  /// Next scheduled late-write reconciliation; private expiry never moves.
  #[serde(default)]
  pub next_cleanup_at: Option<Timestamp>,
  /// Atomic visibility point after all bytes have been independently verified.
  pub published_at: Option<Timestamp>,
}
impl SourceMaterialization {
  /// Produces exact ordinary Work references, without adding specifications.
  pub fn work_artifacts(&self) -> Result<WorkArtifacts, StoreError> {
    let role = |role| {
      self
        .objects
        .iter()
        .find(|v| v.role == role)
        .map(|v| v.artifact.clone())
        .ok_or_else(source_conflict)
    };
    WorkArtifacts::new(
      role(SourceArtifactRole::Task)?,
      role(SourceArtifactRole::Acceptance)?,
      vec![],
    )
    .map_err(|_| source_conflict())
  }
}
/// Reserve before writing bytes; all caller facts originate in trusted mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveSourceArtifacts {
  /// Current source scan ownership.
  pub claim: WorkSourceScanClaim,
  /// Original preparation operation.
  pub preparation_id: FactoryDigest,
  /// Digest binding original mapping and all preparation choices.
  pub mapping_digest: FactoryDigest,
  /// Complete immutable task and acceptance metadata.
  pub objects: Vec<SourceArtifactObject>,
  /// Bounded requested private lifetime.
  pub retention_millis: u64,
  /// Trusted observation time.
  pub now: Timestamp,
}
impl ReserveSourceArtifacts {
  /// Validates byte/role/identity bounds without I/O.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.objects.len() != 2 || !(1..=MAX_SOURCE_RETENTION_MILLIS).contains(&self.retention_millis) {
      return Err(source_conflict());
    }
    let first = &self.objects[0];
    let second = &self.objects[1];
    if first.role == second.role
      || first.artifact.artifact_id() == second.artifact.artifact_id()
      || first.upload_id == second.upload_id
      || self.objects.iter().any(|v| {
        v.artifact.encoded_size() == 0
          || v.artifact.encoded_size() > u64::from(MAX_SOURCE_INPUT_BYTES)
          || v.media_type.as_str() != "application/json"
      })
    {
      return Err(source_conflict());
    }
    Ok(())
  }
  /// Freezes original metadata or recovers the identical unexpired reservation.
  pub fn retain(
    &self,
    preparation: &crate::WorkSourcePreparation,
    current: Option<&SourceMaterialization>,
  ) -> Result<SourceMaterialization, StoreError> {
    self.validate()?;
    if self.now >= preparation.snapshot_expires_at || preparation.snapshot.is_empty() {
      return Err(source_conflict());
    }
    if let Some(current) = current {
      if current.cleaned_at.is_some()
        || current.rejection.is_some()
        || current.mapping_digest != self.mapping_digest
        || current.objects != self.objects
        || current.retention_millis != self.retention_millis
        || self.now < current.reserved_at
        || self.now >= current.expires_at
      {
        return Err(source_conflict());
      }
      return Ok(current.clone());
    }
    let expires = self
      .now
      .unix_millis()
      .checked_add(i64::try_from(self.retention_millis).map_err(|_| source_conflict())?)
      .ok_or_else(source_conflict)?;
    Ok(SourceMaterialization {
      preparation_id: self.preparation_id,
      project_id: preparation.input.project_id(),
      input_id: preparation.input.id(),
      input_version: preparation.input.version(),
      mapping_digest: self.mapping_digest,
      objects: self.objects.clone(),
      retention_millis: self.retention_millis,
      reserved_at: self.now,
      expires_at: Timestamp::from_unix_millis(expires)
        .map_err(|_| source_conflict())?
        .min(preparation.snapshot_expires_at),
      rejection: None,
      cleaned_at: None,
      next_cleanup_at: None,
      published_at: None,
    })
  }
}
/// Publishes the already reserved complete set after trusted byte verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishSourceArtifacts {
  /// Current scan ownership.
  pub claim: WorkSourceScanClaim,
  /// Original preparation identity.
  pub preparation_id: FactoryDigest,
  /// Exact original mapping/choices digest.
  pub mapping_digest: FactoryDigest,
  /// Trusted publication time.
  pub now: Timestamp,
}
impl SourceMaterialization {
  /// Publishes only the original unexpired metadata, preserving first publication time.
  pub fn publish(&self, request: &PublishSourceArtifacts) -> Result<Self, StoreError> {
    if self.cleaned_at.is_some()
      || self.rejection.is_some()
      || self.preparation_id != request.preparation_id
      || self.project_id != request.claim.project_id
      || self.mapping_digest != request.mapping_digest
      || request.now < self.reserved_at
      || request.now >= self.expires_at
    {
      return Err(source_conflict());
    }
    let mut result = self.clone();
    result.published_at.get_or_insert(request.now);
    Ok(result)
  }
}
/// Source-owned private input, distinct from an ordinary Build output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PublishedSourceArtifact {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact original source operation.
  pub preparation_id: FactoryDigest,
  /// Original mapping/input provenance, without private content.
  pub input_version: WorkSourceInputVersion,
  /// Stable input identity.
  pub input_id: WorkSourceInputId,
  /// Exact original mapping digest.
  pub mapping_digest: FactoryDigest,
  /// Immutable verified object metadata.
  pub object: SourceArtifactObject,
  /// First authoritative visibility time.
  pub published_at: Timestamp,
  /// Exclusive private read deadline.
  pub expires_at: Timestamp,
}
impl SourceMaterialization {
  /// Returns metadata only after atomic publication; ordinary output discovery never reads this.
  pub fn artifact(&self, id: ArtifactId) -> Option<PublishedSourceArtifact> {
    if self.cleaned_at.is_some() {
      return None;
    }
    Some(PublishedSourceArtifact {
      project_id: self.project_id,
      preparation_id: self.preparation_id,
      input_version: self.input_version,
      input_id: self.input_id,
      mapping_digest: self.mapping_digest,
      object: self.objects.iter().find(|v| v.artifact.artifact_id() == id)?.clone(),
      published_at: self.published_at?,
      expires_at: self.expires_at,
    })
  }
}
/// Query-only source input metadata, independent of configuration, scanning and admission.
#[async_trait]
pub trait SourceArtifactQueryStore: Send + Sync {
  /// Resolves only Project-owned published source input metadata.
  async fn source_artifact(
    &self,
    project: ProjectId,
    artifact: ArtifactId,
  ) -> Result<Option<PublishedSourceArtifact>, StoreError>;
}
/// Trusted source byte lifecycle, separate from Agent output publication.
#[async_trait]
pub trait WorkSourceArtifactStore: FactorySourceStore + SourceArtifactQueryStore {
  /// Classifies a rejected candidate with a bounded private snapshot lifetime.
  async fn reject_source_preparation(
    &self,
    request: RejectSourcePreparation,
  ) -> Result<SourceMaterialization, StoreError>;
  /// Lists bounded expired metadata, including unpublished objects and revisitable tombstones.
  async fn expired_source_preparations(
    &self,
    now: Timestamp,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<SourcePreparationCleanup>, StoreError>;
  /// Clears private snapshot bytes only after exact physical deletion was acknowledged.
  async fn complete_source_preparation_cleanup(
    &self,
    expected: SourcePreparationCleanup,
    now: Timestamp,
    recheck_at: Timestamp,
  ) -> Result<(), StoreError>;
  /// Reserves immutable cleanup metadata before any physical write.
  async fn reserve_source_artifacts(
    &self,
    request: ReserveSourceArtifacts,
  ) -> Result<SourceMaterialization, StoreError>;
  /// Publishes the complete verified set atomically under current ownership.
  async fn publish_source_artifacts(
    &self,
    request: PublishSourceArtifacts,
  ) -> Result<SourceMaterialization, StoreError>;
  /// Reads retained metadata for restart; returns no private snapshot content.
  async fn source_materialization(
    &self,
    project: ProjectId,
    preparation: FactoryDigest,
  ) -> Result<Option<SourceMaterialization>, StoreError>;
}

/// Safe classified mapping failure before any byte publication or admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectSourcePreparation {
  /// Current authorized source owner.
  pub claim: WorkSourceScanClaim,
  /// Original private operation.
  pub preparation_id: FactoryDigest,
  /// Original installed mapping/choices digest.
  pub mapping_digest: FactoryDigest,
  /// Safe bounded classification, never raw provider text.
  pub reason: octacity_server_factory::FactoryKey,
  /// Original bounded private snapshot lifetime.
  pub retention_millis: u64,
  /// Trusted observation time.
  pub now: Timestamp,
}
impl RejectSourcePreparation {
  /// Freezes a failure without changing an already reserved successful publication.
  pub fn retain(
    &self,
    preparation: &crate::WorkSourcePreparation,
    current: Option<&SourceMaterialization>,
  ) -> Result<SourceMaterialization, StoreError> {
    if !(1..=MAX_SOURCE_RETENTION_MILLIS).contains(&self.retention_millis)
      || self.now >= preparation.snapshot_expires_at
      || preparation.snapshot.is_empty()
    {
      return Err(source_conflict());
    }
    if let Some(current) = current {
      if current.rejection.as_ref() != Some(&self.reason)
        || current.mapping_digest != self.mapping_digest
        || current.retention_millis != self.retention_millis
        || current.cleaned_at.is_some()
        || self.now >= current.expires_at
      {
        return Err(source_conflict());
      }
      return Ok(current.clone());
    }
    let expires = self
      .now
      .unix_millis()
      .checked_add(i64::try_from(self.retention_millis).map_err(|_| source_conflict())?)
      .ok_or_else(source_conflict)?;
    Ok(SourceMaterialization {
      preparation_id: self.preparation_id,
      project_id: preparation.input.project_id(),
      input_id: preparation.input.id(),
      input_version: preparation.input.version(),
      mapping_digest: self.mapping_digest,
      objects: vec![],
      retention_millis: self.retention_millis,
      reserved_at: self.now,
      expires_at: Timestamp::from_unix_millis(expires)
        .map_err(|_| source_conflict())?
        .min(preparation.snapshot_expires_at),
      published_at: None,
      rejection: Some(self.reason.clone()),
      cleaned_at: None,
      next_cleanup_at: None,
    })
  }
}
/// Exact private byte cleanup identities, with no snapshot content or credentials.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePreparationCleanup {
  /// Original preparation identity.
  pub preparation_id: FactoryDigest,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exclusive original retention deadline.
  pub expires_at: Timestamp,
  /// Every reserved object, whether publication was acknowledged or interrupted.
  pub objects: Vec<SourceArtifactObject>,
  /// First cleanup acknowledgement, retained across reconciliations.
  pub cleaned_at: Option<Timestamp>,
  /// Compare-and-swap schedule; a duplicate acknowledgement cannot defer another pass.
  pub next_cleanup_at: Option<Timestamp>,
}
impl SourcePreparationCleanup {
  /// Excludes completed raw/rejected preparations and defers late-write reconciliation.
  pub fn is_due(&self, now: Timestamp) -> bool {
    self.expires_at <= now
      && self
        .cleaned_at
        .is_none_or(|_| !self.objects.is_empty() && self.next_cleanup_at.is_none_or(|next| next <= now))
  }
  /// Validates immutable deletion targets and treats already-applied acknowledgements as no-ops.
  pub fn requires_acknowledgement(
    &self,
    current: &Self,
    now: Timestamp,
    recheck_at: Timestamp,
  ) -> Result<bool, StoreError> {
    if self.preparation_id != current.preparation_id
      || self.project_id != current.project_id
      || self.expires_at != current.expires_at
      || self.objects != current.objects
      || now < current.expires_at
    {
      return Err(source_conflict());
    }
    if self != current {
      return if current.cleaned_at.is_some() {
        Ok(false)
      } else {
        Err(source_conflict())
      };
    }
    if !current.is_due(now) || recheck_at <= now {
      return Err(source_conflict());
    }
    Ok(true)
  }
  /// Projects stable cleanup metadata without extending retention on retry.
  pub fn from_preparation(
    preparation: &crate::WorkSourcePreparation,
    manifest: Option<&SourceMaterialization>,
  ) -> Self {
    Self {
      preparation_id: preparation.id,
      project_id: preparation.input.project_id(),
      expires_at: manifest.map_or(preparation.snapshot_expires_at, |m| {
        m.expires_at.min(preparation.snapshot_expires_at)
      }),
      objects: manifest.map_or_else(Vec::new, |m| m.objects.clone()),
      cleaned_at: manifest.and_then(|m| m.cleaned_at),
      next_cleanup_at: manifest.and_then(|m| m.next_cleanup_at),
    }
  }
}

impl SourceMaterialization {
  /// Fresh admission cannot consume unpublished, expired, cleaned or substituted inputs.
  pub fn require_admission(&self, artifacts: &WorkArtifacts, now: Timestamp) -> Result<(), StoreError> {
    if self.published_at.is_none()
      || self.rejection.is_some()
      || self.cleaned_at.is_some()
      || now < self.reserved_at
      || now >= self.expires_at
      || self.work_artifacts()? != *artifacts
    {
      return Err(source_conflict());
    }
    Ok(())
  }
}
