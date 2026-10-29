//! Standalone context publication stays within the shared private Project input owner.
use super::*;
use octacity_server_store::{ProjectDocumentInputStore, ProjectDocumentPreparation, ReserveProjectDocument};

impl<S: ProjectTextInputStore + ProjectDocumentInputStore + 'static, B: ArtifactBytePublisher + ArtifactByteReader>
  ProjectTextIntake<S, B>
{
  /// Publishes one reviewed private document; creates no admission, Work, Run, Build or execution grant.
  pub async fn publish_document(
    &self,
    context: &ManagementRequestContext,
    submission: ProjectDocumentSubmission,
  ) -> Result<ProjectDocumentResource, ApplicationError> {
    submission.validate()?;
    self.authorize_document(context, submission.project_id).await?;
    let original = self
      .store
      .project_document_preparation(
        submission.project_id,
        &context.security_scope().to_store(),
        &submission.idempotency_key,
      )
      .await?;
    if let Some(original) = original {
      if !original.matches_submission(&submission) {
        return Err(
          octacity_server_store::StoreError::Conflict {
            entity: octacity_server_domain::EntityKind::Artifact,
          }
          .into(),
        );
      }
      if original.published_at.is_some() {
        return Ok(original.resource()?);
      }
    }
    let record = self
      .store
      .reserve_project_document(crate::management_security::audited_mutation(
        context,
        ReserveProjectDocument {
          submission,
          now: self.clock.now()?,
        },
      )?)
      .await?;
    let result = self.publish_context_document(context, &record).await;
    if result.is_err() {
      let _ = self
        .store
        .release_project_document(record.guard(self.clock.now()?))
        .await;
    }
    result
  }
  async fn authorize_document(
    &self,
    context: &ManagementRequestContext,
    project: octacity_server_domain::ProjectId,
  ) -> Result<(), ApplicationError> {
    crate::factory_configuration_choices::require_authorized_reference(
      self.policy.as_ref(),
      context,
      ManagementAction::Publish,
      ManagementResourceKind::Project,
      project,
    )
    .await
  }
  async fn publish_context_document(
    &self,
    context: &ManagementRequestContext,
    record: &ProjectDocumentPreparation,
  ) -> Result<ProjectDocumentResource, ApplicationError> {
    let content = self
      .store
      .project_document_content(
        record.intent.project_id,
        &context.security_scope().to_store(),
        &record.intent.idempotency_key,
      )
      .await?
      .ok_or_else(ApplicationError::unavailable)?;
    let object = record.objects.first().ok_or_else(ApplicationError::invalid)?;
    self
      .store
      .require_project_document_owner(record.guard(self.clock.now()?))
      .await?;
    if !publication::is_input_published(self.bytes.as_ref(), object).await? {
      self.authorize_document(context, record.intent.project_id).await?;
      self
        .store
        .require_project_document_owner(record.guard(self.clock.now()?))
        .await?;
      self
        .bytes
        .publish_verified(&publication::input_object(object)?, content.into_bytes())
        .await
        .map_err(|_| ApplicationError::unavailable())?;
      self
        .store
        .require_project_document_owner(record.guard(self.clock.now()?))
        .await?;
      if !publication::is_input_published(self.bytes.as_ref(), object).await? {
        return Err(ApplicationError::unavailable());
      }
    }
    self.authorize_document(context, record.intent.project_id).await?;
    Ok(
      self
        .store
        .publish_project_document(crate::management_security::audited_mutation(
          context,
          record.guard(self.clock.now()?),
        )?)
        .await?
        .resource()?,
    )
  }
}

#[path = "project_input_document_management.rs"]
mod management;
pub use management::*;
