//! Reviewed Project skill-folder import and exact protected-input materialization.
use crate::*;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactByteReader, ArtifactObject};
use octacity_protocol::{PROTECTED_INPUT_ROOT, ProtectedInputManifestV3, ProtectedInputV3};
use octacity_server_domain::{ArtifactId, ArtifactUploadId, ProjectId};
use octacity_server_factory::{
  FactoryArtifactReference, FactoryDigest, FactoryKey, FactoryRepositoryPath, FactoryRoleSkillBundle,
};
use octacity_server_store::{
  IdempotencyKey, ProjectSkillImportIntent, ProjectSkillImportRecord, ProjectSkillImportStore,
  PublishProjectSkillImport, PublishedExecutionInput,
};
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;

/// Explicit reviewed entry; never a path for the server to open on its host.
#[derive(Clone)]
pub enum ReviewedSkillEntry {
  /// A detected link is rejected, never dereferenced or uploaded.
  Symlink {
    /// Relative link name, excluded from diagnostics.
    path: String,
    /// Untrusted target; never opened.
    target: String,
  },
  /// Exact regular-file bytes and the digest the operator reviewed.
  File {
    /// Portable path relative to the selected folder.
    path: String,
    /// Reviewed expected SHA-256.
    sha256: FactoryDigest,
    /// Private content, excluded from Debug and audit.
    bytes: Vec<u8>,
  },
}
/// One explicit import; no local directory watching or ambient skill discovery.
#[derive(Clone)]
pub struct ImportProjectSkill {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Logical skill name; each import creates a separate immutable version.
  pub identity: FactoryKey,
  /// Accepted management-scope replay key.
  pub idempotency_key: IdempotencyKey,
  /// Complete reviewed folder contents.
  pub entries: Vec<ReviewedSkillEntry>,
}
/// Shared object storage supplies bytes; this owner only publishes their Project metadata.
pub struct ProjectSkillImport<S, B> {
  store: Arc<S>,
  bytes: Arc<B>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S: ProjectSkillImportStore, B: ArtifactByteReader> ProjectSkillImport<S, B> {
  /// Installs ordinary publication metadata, byte ports, authorization and trusted time.
  pub fn new(
    store: Arc<S>,
    bytes: Arc<B>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      store,
      bytes,
      policy,
      clock,
    }
  }
  /// Uses Project visibility before exposing safe exact choices; private files stay out of pages.
  pub async fn catalog(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    after: Option<octacity_server_factory::ImmutableReference>,
    limit: u16,
  ) -> Result<octacity_server_store::ProjectSkillImportPage, ApplicationError> {
    let resource = management_security::instance_resource(ManagementResourceKind::Project, project)
      .map_err(|_| ApplicationError::invalid())?;
    let grant = self
      .policy
      .authorize(context, ManagementAction::View, &resource)
      .await
      .map_err(|_| ApplicationError::invalid())?;
    crate::factory_role_profiles::require_project_visibility(&grant, project)?;
    Ok(self.store.list_project_skill_imports(project, after, limit).await?)
  }
  /// Publishes one complete immutable tree. Lost responses retry the same original receipt.
  pub async fn import(
    &self,
    context: &ManagementRequestContext,
    command: ImportProjectSkill,
  ) -> Result<ProjectSkillImportRecord, ApplicationError>
  where
    B: ArtifactBytePublisher,
  {
    let resource = management_security::instance_resource(ManagementResourceKind::Project, command.project_id)
      .map_err(|_| ApplicationError::invalid())?;
    let grant = self
      .policy
      .authorize(context, ManagementAction::Administer, &resource)
      .await
      .map_err(|_| ApplicationError::invalid())?;
    crate::factory_role_profiles::require_project_visibility(&grant, command.project_id)?;
    if command.entries.len() > octacity_server_factory::MAX_CONTEXT_MANIFEST_ENTRIES {
      return Err(ApplicationError::invalid());
    }
    let operation = FactoryDigest::sha256(
      "octacity.project-skill-import.v1",
      &[
        command.project_id.as_uuid().as_bytes(),
        context.security_scope().to_store().as_str().as_bytes(),
        command.idempotency_key.as_str().as_bytes(),
      ],
    );
    let namespace = Uuid::new_v5(&Uuid::NAMESPACE_OID, &operation.as_bytes());
    let mut files = BTreeMap::new();
    let mut uploads = BTreeMap::new();
    let mut bodies = BTreeMap::new();
    for entry in command.entries {
      let ReviewedSkillEntry::File { path, sha256, bytes } = entry else {
        return Err(ApplicationError::invalid());
      };
      if FactoryDigest::content_sha256(&bytes) != sha256 {
        return Err(ApplicationError::invalid());
      }
      let path = FactoryRepositoryPath::new(path).map_err(|_| ApplicationError::invalid())?;
      let artifact = ArtifactId::from_uuid(Uuid::new_v5(&namespace, path.as_str().as_bytes()))
        .map_err(|_| ApplicationError::invalid())?;
      if files
        .insert(
          path,
          FactoryArtifactReference::new(artifact, sha256, bytes.len() as u64)
            .map_err(|_| ApplicationError::invalid())?,
        )
        .is_some()
      {
        return Err(ApplicationError::invalid());
      }
      uploads.insert(
        artifact,
        ArtifactUploadId::from_uuid(Uuid::new_v5(&artifact.as_uuid(), b"retained-input.v1"))
          .map_err(|_| ApplicationError::invalid())?,
      );
      bodies.insert(artifact, bytes);
    }
    let bundle = FactoryRoleSkillBundle::new(
      command.identity,
      FactoryKey::new(operation.to_string()).map_err(|_| ApplicationError::invalid())?,
      files,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let intent = ProjectSkillImportIntent {
      project_id: command.project_id,
      bundle,
      uploads,
      idempotency_key: command.idempotency_key,
    };
    let request = |now| {
      crate::management_security::audited_mutation(
        context,
        PublishProjectSkillImport {
          intent: intent.clone(),
          now,
        },
      )
    };
    let record = self
      .store
      .reserve_project_skill_import(request(self.clock.now()?)?)
      .await?;
    if record.published_at.is_some() {
      return Ok(record);
    }
    for input in record.inputs()? {
      let object = object(&input)?;
      self
        .bytes
        .publish_verified(
          &object,
          bodies
            .remove(&input.artifact_id)
            .ok_or_else(ApplicationError::invalid)?,
        )
        .await
        .map_err(|_| ApplicationError::unavailable())?;
      self.read_verified(&input).await?;
    }
    Ok(
      self
        .store
        .publish_project_skill_import(request(self.clock.now()?)?)
        .await?,
    )
  }
  /// Resolves complete exact bundles to the existing signed transfer contract.
  /// The trusted compiler supplies the authorized Project and exact selections.
  /// The Agent's ordinary protected-input materializer writes these destinations.
  pub async fn materialize(
    &self,
    project: ProjectId,
    selected: BTreeMap<FactoryRepositoryPath, FactoryRoleSkillBundle>,
  ) -> Result<ProtectedInputManifestV3, ApplicationError> {
    let mut count = 0_usize;
    let mut total = 0_u64;
    for bundle in selected.values() {
      count = count
        .checked_add(bundle.files().len())
        .ok_or_else(ApplicationError::invalid)?;
      for file in bundle.files().values() {
        total = total
          .checked_add(file.encoded_size())
          .ok_or_else(ApplicationError::invalid)?;
      }
    }
    if count > octacity_server_factory::MAX_CONTEXT_MANIFEST_ENTRIES
      || total > octacity_server_factory::MAX_CONTEXT_MANIFEST_BYTES
    {
      return Err(ApplicationError::invalid());
    }
    let mut inputs = Vec::new();
    let mut retained = Vec::new();
    for (destination, bundle) in selected {
      let record = self
        .store
        .project_skill_import(project, bundle.reference())
        .await?
        .ok_or_else(ApplicationError::invalid)?;
      if record.intent.bundle != bundle {
        return Err(ApplicationError::invalid());
      }
      let objects: BTreeMap<_, _> = record
        .inputs()?
        .into_iter()
        .map(|input| (input.artifact_id, input))
        .collect();
      for (path, file) in bundle.files() {
        let input = objects.get(&file.artifact_id()).ok_or_else(ApplicationError::invalid)?;
        let relative = FactoryRepositoryPath::new(format!("{}/{}", destination.as_str(), path.as_str()))
          .map_err(|_| ApplicationError::invalid())?;
        retained.push(input.clone());
        inputs.push(ProtectedInputV3 {
          artifact_id: input.artifact_id.to_string(),
          size_bytes: input.size_bytes,
          sha256: input.digest.to_string(),
          media_type: input.media_type.as_str().into(),
          destination: format!("{PROTECTED_INPUT_ROOT}/{}", relative.as_str()),
        });
      }
    }
    inputs.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
    let manifest = ProtectedInputManifestV3 { inputs };
    manifest.validate().map_err(|_| ApplicationError::invalid())?;
    for input in &retained {
      self.read_verified(input).await?;
    }
    Ok(manifest)
  }
  async fn read_verified(&self, input: &PublishedExecutionInput) -> Result<(), ApplicationError> {
    let bytes = self
      .bytes
      .read_published(&object(input)?, input.size_bytes)
      .await
      .map_err(|_| ApplicationError::unavailable())?;
    if bytes.len() as u64 != input.size_bytes
      || FactoryDigest::content_sha256(&bytes).as_bytes() != input.digest.as_bytes()
    {
      return Err(ApplicationError::unavailable());
    }
    Ok(())
  }
}
fn object(input: &PublishedExecutionInput) -> Result<ArtifactObject, ApplicationError> {
  ArtifactObject::new(
    input.artifact_id,
    input.upload_id,
    input.size_bytes,
    input.digest.to_string(),
    input.media_type.as_str(),
  )
  .map_err(|_| ApplicationError::invalid())
}
