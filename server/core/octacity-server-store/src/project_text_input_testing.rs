//! In-memory shared text input journal, using ordinary Factory admission authority.
use crate::*;
use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId};
use octacity_server_factory::FactoryDigest;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct ProjectTextMemoryState {
  records: BTreeMap<FactoryDigest, RetainedProjectInput>,
  documents: BTreeMap<FactoryDigest, RetainedProjectInputBody>,
  keys: BTreeMap<(ProjectId, ManagementIdempotencyKey), FactoryDigest>,
}
#[async_trait::async_trait]
impl ProjectInputQueryStore for crate::testing::InMemoryFactoryConfigurationStore {
  async fn project_input_artifact(
    &self,
    project: ProjectId,
    artifact: ArtifactId,
  ) -> Result<Option<PublishedProjectInput>, StoreError> {
    let state = self.lock()?;
    Ok(
      state
        .project_text
        .records
        .values()
        .find_map(|record| record.published_input(project, artifact)),
    )
  }
}
#[async_trait::async_trait]
impl ProjectTextInputStore for crate::testing::InMemoryFactoryConfigurationStore {
  async fn project_text_preparation(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<ProjectTextPreparation>, StoreError> {
    let state = self.lock()?;
    Ok(
      state
        .project_text
        .keys
        .get(&(project, ManagementIdempotencyKey::new(scope.clone(), key.clone())))
        .and_then(|id| state.project_text.records.get(id))
        .map(|record| record.work().cloned())
        .transpose()?,
    )
  }
  async fn reserve_project_text(
    &self,
    request: ManagementMutation<ReserveProjectText>,
  ) -> Result<ProjectTextPreparation, StoreError> {
    let (request, audit) = request.into_parts();
    let s = request.submission;
    s.documents.validate()?;
    let key = (
      s.intent.target.project_id,
      audit.scoped_idempotency_key(&s.intent.idempotency_key),
    );
    let mut state = self.lock()?;
    if let Some(id) = state.project_text.keys.get(&key).copied() {
      let original = state.project_text.records[&id].work()?;
      if !original.matches_submission(&s) {
        return Err(project_text_conflict());
      }
      if original.admission.is_some() {
        return Ok(original.clone());
      }
      let next = original.reclaim(request.now)?;
      state
        .project_text
        .records
        .insert(id, RetainedProjectInput::Work(Box::new(next.clone())));
      return Ok(next);
    }
    let documents = s.documents.clone();
    let result = ProjectTextPreparation::reserve(s, audit.security_scope().clone(), request.now)?;
    let id = result.id;
    state
      .project_text
      .documents
      .insert(id, RetainedProjectInputBody::Work(documents));
    state
      .project_text
      .records
      .insert(id, RetainedProjectInput::Work(Box::new(result.clone())));
    state.project_text.keys.insert(key, id);
    Ok(result)
  }
  async fn project_text_documents(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<ProjectTextDocuments>, StoreError> {
    let state = self.lock()?;
    Ok(
      state
        .project_text
        .keys
        .get(&(project, ManagementIdempotencyKey::new(scope.clone(), key.clone())))
        .and_then(|id| state.project_text.documents.get(id))
        .map(|body| match body {
          RetainedProjectInputBody::Work(documents) => Ok(documents.clone()),
          RetainedProjectInputBody::Document(_) => Err(project_text_conflict()),
        })
        .transpose()?,
    )
  }
  async fn require_project_text_owner(&self, guard: ProjectTextGuard) -> Result<(), StoreError> {
    self
      .lock()?
      .project_text
      .records
      .get(&guard.id)
      .ok_or_else(project_text_conflict)?
      .work()?
      .require_owner(&guard)
  }
  async fn release_project_text(&self, guard: ProjectTextGuard) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if let Some(record) = state
      .project_text
      .records
      .get_mut(&guard.id)
      .and_then(|record| record.work_mut().ok())
      && record.generation == guard.generation
      && record.intent.target.project_id == guard.project_id
    {
      record.lease_until = record.lease_until.min(guard.now.max(record.claimed_at));
    }
    Ok(())
  }
  async fn freeze_project_text(
    &self,
    guard: ProjectTextGuard,
    revision: ImmutableRevision,
  ) -> Result<ProjectTextPreparation, StoreError> {
    let mut state = self.lock()?;
    let record = state
      .project_text
      .records
      .get_mut(&guard.id)
      .ok_or_else(project_text_conflict)?
      .work_mut()?;
    record.require_owner(&guard)?;
    if record.frozen_revision.as_ref().is_some_and(|r| r != &revision) {
      return Err(project_text_conflict());
    }
    record.frozen_revision.get_or_insert(revision);
    Ok(record.clone())
  }
  async fn publish_project_text(&self, guard: ProjectTextGuard) -> Result<ProjectTextPreparation, StoreError> {
    let mut state = self.lock()?;
    let record = state
      .project_text
      .records
      .get_mut(&guard.id)
      .ok_or_else(project_text_conflict)?
      .work_mut()?;
    record.require_owner(&guard)?;
    if record.frozen_revision.is_none() {
      return Err(project_text_conflict());
    }
    record.published_at.get_or_insert(guard.now);
    Ok(record.clone())
  }
  async fn admit_project_text(
    &self,
    guard: ProjectTextGuard,
    request: ManagementMutation<AdmitFactoryWork>,
  ) -> Result<FactoryAdmissionMutationOutcome, StoreError> {
    let mut state = self.lock()?;
    state
      .project_text
      .records
      .get(&guard.id)
      .ok_or_else(project_text_conflict)?
      .work()?
      .validate_admission(&guard, &request)?;
    let outcome = crate::factory_configuration_testing::admit_factory_work_locked(&mut state, request)?;
    state
      .project_text
      .records
      .get_mut(&guard.id)
      .ok_or_else(project_text_conflict)?
      .work_mut()?
      .admission = Some(outcome.admission.clone());
    Ok(outcome)
  }
}

// Both input kinds occupy the same journal, private-body map and scoped replay key space.
enum RetainedProjectInput {
  Work(Box<ProjectTextPreparation>),
  Document(Box<ProjectDocumentPreparation>),
}
enum RetainedProjectInputBody {
  Work(ProjectTextDocuments),
  Document(String),
}
impl RetainedProjectInput {
  fn work(&self) -> Result<&ProjectTextPreparation, StoreError> {
    match self {
      Self::Work(record) => Ok(record),
      Self::Document(_) => Err(project_text_conflict()),
    }
  }
  fn work_mut(&mut self) -> Result<&mut ProjectTextPreparation, StoreError> {
    match self {
      Self::Work(record) => Ok(record),
      Self::Document(_) => Err(project_text_conflict()),
    }
  }
  fn document(&self) -> Result<&ProjectDocumentPreparation, StoreError> {
    match self {
      Self::Document(record) => Ok(record),
      Self::Work(_) => Err(crate::project_input_document::document_conflict()),
    }
  }
  fn document_mut(&mut self) -> Result<&mut ProjectDocumentPreparation, StoreError> {
    match self {
      Self::Document(record) => Ok(record),
      Self::Work(_) => Err(crate::project_input_document::document_conflict()),
    }
  }
  fn published_input(&self, project: ProjectId, artifact: ArtifactId) -> Option<PublishedProjectInput> {
    let (owner, id, mapping, objects, published_at, expires_at) = match self {
      Self::Work(p) => (
        p.intent.target.project_id,
        p.id,
        &p.intent.mapping,
        &p.objects,
        p.published_at,
        p.expires_at,
      ),
      Self::Document(p) => (
        p.intent.project_id,
        p.id,
        &p.intent.mapping,
        &p.objects,
        p.published_at,
        p.expires_at,
      ),
    };
    (owner == project).then_some(())?;
    Some(PublishedProjectInput {
      project_id: owner,
      preparation_id: id,
      mapping: mapping.clone(),
      object: objects.iter().find(|o| o.artifact.artifact_id() == artifact)?.clone(),
      published_at: published_at?,
      expires_at,
    })
  }
}
#[path = "project_input_document_testing.rs"]
mod documents;
