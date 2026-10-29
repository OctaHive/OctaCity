//! The document variant uses the existing in-memory Project text journal.
use super::*;
use crate::project_input_document::document_conflict;
#[async_trait::async_trait]
impl ProjectDocumentInputStore for crate::testing::InMemoryFactoryConfigurationStore {
  async fn list_project_documents(&self, request: ListProjectDocuments) -> Result<ProjectDocumentPage, StoreError> {
    request.validate()?;
    let state = self.lock()?;
    if !state.projects.contains(&request.project_id) {
      return Err(StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Project,
      });
    }
    let mut items = state
      .project_text
      .records
      .values()
      .filter_map(|row| row.document().ok())
      .filter(|row| row.intent.project_id == request.project_id && row.expires_at > request.observed_at)
      .filter_map(|row| row.resource().ok())
      .filter(|row| {
        request.visibility.allows(&row.artifact.artifact_id())
          && request.after.is_none_or(|after| row.artifact.artifact_id() > after)
      })
      .collect::<Vec<_>>();
    items.sort_by_key(|row| row.artifact.artifact_id());
    let next_cursor = crate::pagination::finish_bounded_page(
      &mut items,
      std::num::NonZeroU16::new(request.limit).ok_or_else(document_conflict)?,
      |row| row.artifact.artifact_id(),
    );
    Ok(ProjectDocumentPage { items, next_cursor })
  }
  async fn project_document_preparation(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<ProjectDocumentPreparation>, StoreError> {
    let state = self.lock()?;
    state
      .project_text
      .keys
      .get(&(project, ManagementIdempotencyKey::new(scope.clone(), key.clone())))
      .and_then(|id| state.project_text.records.get(id))
      .map(|record| record.document().cloned())
      .transpose()
  }
  async fn project_document_content(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<String>, StoreError> {
    let state = self.lock()?;
    state
      .project_text
      .keys
      .get(&(project, ManagementIdempotencyKey::new(scope.clone(), key.clone())))
      .and_then(|id| state.project_text.documents.get(id))
      .map(|body| match body {
        RetainedProjectInputBody::Document(content) => Ok(content.clone()),
        RetainedProjectInputBody::Work(_) => Err(document_conflict()),
      })
      .transpose()
  }
  async fn reserve_project_document(
    &self,
    request: ManagementMutation<ReserveProjectDocument>,
  ) -> Result<ProjectDocumentPreparation, StoreError> {
    let (request, audit) = request.into_parts();
    request.submission.validate()?;
    let key = (
      request.submission.project_id,
      audit.scoped_idempotency_key(&request.submission.idempotency_key),
    );
    let mut state = self.lock()?;
    if !state.projects.contains(&request.submission.project_id) {
      return Err(StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Project,
      });
    }
    if let Some(id) = state.project_text.keys.get(&key).copied() {
      let original = state.project_text.records[&id].document()?;
      if !original.matches_submission(&request.submission) {
        return Err(document_conflict());
      }
      if original.published_at.is_some() {
        return Ok(original.clone());
      }
      let next = original.reclaim(request.now)?;
      state
        .project_text
        .records
        .insert(id, RetainedProjectInput::Document(Box::new(next.clone())));
      return Ok(next);
    }
    let record = ProjectDocumentPreparation::reserve(&request.submission, audit.security_scope().clone(), request.now)?;
    state.project_text.documents.insert(
      record.id,
      RetainedProjectInputBody::Document(request.submission.content),
    );
    state.project_text.keys.insert(key, record.id);
    state
      .project_text
      .records
      .insert(record.id, RetainedProjectInput::Document(Box::new(record.clone())));
    Ok(record)
  }
  async fn require_project_document_owner(&self, guard: ProjectTextGuard) -> Result<(), StoreError> {
    self
      .lock()?
      .project_text
      .records
      .get(&guard.id)
      .ok_or_else(document_conflict)?
      .document()?
      .require_owner(&guard)
  }
  async fn release_project_document(&self, guard: ProjectTextGuard) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if let Some(record) = state
      .project_text
      .records
      .get_mut(&guard.id)
      .and_then(|record| record.document_mut().ok())
      && record.intent.project_id == guard.project_id
      && record.generation == guard.generation
    {
      record.lease_until = record.lease_until.min(guard.now.max(record.claimed_at));
    }
    Ok(())
  }
  async fn publish_project_document(
    &self,
    request: ManagementMutation<ProjectTextGuard>,
  ) -> Result<ProjectDocumentPreparation, StoreError> {
    let (guard, audit) = request.into_parts();
    let mut state = self.lock()?;
    let record = state
      .project_text
      .records
      .get_mut(&guard.id)
      .ok_or_else(document_conflict)?
      .document_mut()?;
    record.require_owner(&guard)?;
    if record.security_scope != *audit.security_scope() {
      return Err(document_conflict());
    }
    if record.published_at.is_some() {
      return Ok(record.clone());
    }
    record.published_at.get_or_insert(guard.now);
    let record = record.clone();
    state.evidence.insert(format!("publish-project-document:{}", record.id));
    state.audit.insert(crate::testing::recorded_management_audit(
      &audit,
      StoreOperation::PublishProjectDocument,
      octacity_server_domain::EntityKind::Artifact,
      record.resource()?.artifact.artifact_id(),
    ));
    Ok(record)
  }
}
