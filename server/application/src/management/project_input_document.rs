//! Standalone context publication stays within the shared private Project input owner.
use super::*;
use crate::factory::validation::factory_verification_supplement::{
  MAX_BYTES, ResolvedVerificationRepository, VerificationRepositorySelection, VerificationSubmission,
};
use octacity_server_domain::RepositoryId;
use octacity_server_store::{
  ProjectDocumentInputStore, ProjectDocumentPreparation, ProjectDocumentQueryStore, ReserveProjectDocument,
};

impl<S: ProjectTextInputStore + ProjectDocumentInputStore + 'static, B: ArtifactBytePublisher + ArtifactByteReader>
  ProjectTextIntake<S, B>
{
  /// Publishes one reviewed private document; creates no admission, Work, Run, Build or execution grant.
  pub async fn publish_document(
    &self,
    context: &ManagementRequestContext,
    mut submission: ProjectDocumentSubmission,
  ) -> Result<ProjectDocumentResource, ApplicationError> {
    submission.validate()?;
    self.authorize_document(context, submission.project_id).await?;
    let scope = context.security_scope().to_store();
    let original = self
      .store
      .project_document_preparation(submission.project_id, &scope, &submission.idempotency_key)
      .await?;
    let verification = submission.purpose == octacity_server_store::ProjectDocumentPurpose::Verification;
    let selection_digest = verification.then(|| {
      octacity_server_factory::FactoryDigest::sha256(
        "octacity.project.verification-selection.v1",
        &[submission.name.as_str().as_bytes(), submission.content.as_bytes()],
      )
    });
    let conflict = || octacity_server_store::StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::Artifact,
    };
    let parse_selection =
      |content: &str| serde_json::from_str::<VerificationSubmission>(content).map_err(|_| ApplicationError::invalid());
    let repository_id = if let Some(record) = original
      .as_ref()
      .filter(|record| record.intent.selection_digest.is_some())
    {
      if record.intent.selection_digest != selection_digest {
        return Err(conflict().into());
      }
      if record.published_at.is_some() {
        return Ok(record.resource()?);
      }
      let repository_id = parse_selection(&submission.content)?
        .repository
        .map(|selection| selection.id);
      if let Some(id) = repository_id {
        self.authorize_verification_repository(context, id).await?;
      }
      submission.content = self
        .store
        .project_document_content(submission.project_id, &scope, &submission.idempotency_key)
        .await?
        .ok_or_else(ApplicationError::unavailable)?;
      repository_id
    } else if verification {
      let input = parse_selection(&submission.content)?;
      input.validate_paths()?;
      let repository_id = input.repository.as_ref().map(|selection| selection.id);
      let repository = match &input.repository {
        Some(selection) => Some(
          self
            .read_verification_repository(context, submission.project_id, selection)
            .await?,
        ),
        None => None,
      };
      submission.content =
        serde_json::to_string(&input.freeze(repository)?).map_err(|_| ApplicationError::invalid())?;
      repository_id
    } else {
      None
    };
    if let Some(original) = original {
      if !original.matches_submission(&submission) {
        return Err(conflict().into());
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
          issue_id: None,
          submission,
          selection_digest,
          now: self.clock.now()?,
        },
      )?)
      .await?;
    let result = self.publish_context_document(context, &record, repository_id).await;
    if result.is_err() {
      let _ = self
        .store
        .release_project_document(record.guard(self.clock.now()?))
        .await;
    }
    Ok(result?.resource()?)
  }
  async fn read_verification_repository(
    &self,
    context: &ManagementRequestContext,
    project: octacity_server_domain::ProjectId,
    selection: &VerificationRepositorySelection,
  ) -> Result<ResolvedVerificationRepository, ApplicationError> {
    use octacity_server_domain::RepositorySourceSelection;
    self.authorize_verification_repository(context, selection.id).await?;
    let repository = self
      .repositories
      .as_ref()
      .ok_or_else(ApplicationError::unavailable)?
      .repository_version(selection.id, selection.version)
      .await?;
    if repository.project_id != project {
      return Err(ApplicationError::invalid());
    }
    crate::triggers::manual_trigger::select_source(
      &RepositorySourceSelection::ExactRevision(selection.revision.clone()),
      &repository,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let files = self
      .revisions
      .read_files(&repository, &selection.revision, &selection.paths, MAX_BYTES as u32)
      .await
      .map_err(|error| match error {
        crate::RevisionResolutionError::NotFound | crate::RevisionResolutionError::Invalid => {
          ApplicationError::invalid()
        }
        _ => ApplicationError::unavailable(),
      })?;
    if files.len() != selection.paths.len() || selection.paths.iter().any(|path| !files.contains_key(path)) {
      return Err(ApplicationError::invalid());
    }
    self.authorize_verification_repository(context, selection.id).await?;
    Ok(ResolvedVerificationRepository {
      reference: crate::factory::execution::factory_code_sources::repository_reference(&repository)?,
      files,
    })
  }
  async fn authorize_verification_repository(
    &self,
    context: &ManagementRequestContext,
    repository_id: RepositoryId,
  ) -> Result<(), ApplicationError> {
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Repository,
      repository_id,
    )
    .await
  }
  async fn authorize_document(
    &self,
    context: &ManagementRequestContext,
    project: octacity_server_domain::ProjectId,
  ) -> Result<(), ApplicationError> {
    crate::factory::authoring::factory_configuration_choices::require_authorized_reference(
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
    repository_id: Option<RepositoryId>,
  ) -> Result<ProjectDocumentPreparation, ApplicationError> {
    let authorize = || async {
      if let Some(owner) = &record.intent.issue {
        self
          .authorize_issue_document(
            context,
            record.intent.project_id,
            owner.issue_id,
            ManagementAction::Update,
          )
          .await?;
      } else {
        self.authorize_document(context, record.intent.project_id).await?;
      }
      if let Some(id) = repository_id {
        self.authorize_verification_repository(context, id).await?;
      }
      Ok::<(), ApplicationError>(())
    };
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
      authorize().await?;
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
    authorize().await?;
    Ok(
      self
        .store
        .publish_project_document(crate::management_security::audited_mutation(
          context,
          record.guard(self.clock.now()?),
        )?)
        .await?,
    )
  }
}

#[path = "project_input_document_management.rs"]
mod management;
pub use management::*;

#[path = "issue_input_document.rs"]
mod issue_documents;
pub use issue_documents::*;
