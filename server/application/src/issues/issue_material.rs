//! Authorized explicit selections; source bodies stay under their independent owners and permissions.
use crate::factory::authoring::factory_configuration_choices::{
  require_authorized_reference, require_visible_reference,
};
use crate::*;
use octacity_server_domain::{IssueId, ProjectId};
use octacity_server_store::{
  EditIssueMaterials, IdempotencyKey, IssueDiscussionStore, IssueDocumentInputStore, IssueMaterialStore,
  MutationAuditContext, MutationDisposition, ProjectDocumentQueryStore,
};
pub use octacity_server_store::{
  EditIssueMaterialsIntent, IssueMaterialReference, IssueMaterialSelection, IssueMaterials, MAX_ISSUE_MATERIALS,
};
use std::sync::Arc;

/// Complete reviewed replacement; actor and time are assigned by the server.
#[derive(Clone, Debug)]
pub struct EditIssueMaterialsCommand {
  /// Exact owning Issue, current version and desired source references.
  pub intent: EditIssueMaterialsIntent,
  /// Original scoped replay identity.
  pub idempotency_key: IdempotencyKey,
}
impl Command for EditIssueMaterialsCommand {
  type Outcome = IssueMutationOutcome;
}
impl ManagementAuthorizationTarget for EditIssueMaterialsCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = EditIssueCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.id)
  }
}
/// Composes Issue revision authority with independently owned collaboration sources.
pub struct IssueMaterialHandlers<I, D> {
  issues: Arc<I>,
  documents: Arc<D>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<I, D> IssueMaterialHandlers<I, D> {
  /// Reuses existing Issue/discussion and document ports; no execution or provider dependencies.
  pub fn new(
    issues: Arc<I>,
    documents: Arc<D>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      issues,
      documents,
      policy,
      clock,
    }
  }
  async fn authorize_issue(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    issue: IssueId,
  ) -> Result<(), ApplicationError> {
    require_visible_reference(self.policy.as_ref(), context, ManagementResourceKind::Project, project).await?;
    require_visible_reference(self.policy.as_ref(), context, ManagementResourceKind::Issue, issue).await?;
    require_authorized_reference(
      self.policy.as_ref(),
      context,
      ManagementAction::Update,
      ManagementResourceKind::Issue,
      issue,
    )
    .await
  }
  async fn authorize_source(
    &self,
    context: &ManagementRequestContext,
    reference: &IssueMaterialReference,
  ) -> Result<(), ApplicationError> {
    match reference {
      IssueMaterialReference::Comment { issue_id, .. } => {
        require_visible_reference(
          self.policy.as_ref(),
          context,
          ManagementResourceKind::IssueDiscussion,
          *issue_id,
        )
        .await
      }
      IssueMaterialReference::IssueDocument { artifact, .. }
      | IssueMaterialReference::ProjectDocument { artifact, .. } => {
        require_authorized_reference(
          self.policy.as_ref(),
          context,
          ManagementAction::Download,
          ManagementResourceKind::Artifact,
          artifact.artifact_id(),
        )
        .await
      }
    }
  }
}
#[async_trait::async_trait]
impl<I: IssueMaterialStore + IssueDiscussionStore, D: IssueDocumentInputStore + ProjectDocumentQueryStore>
  ManagementCommandUseCase<EditIssueMaterialsCommand> for IssueMaterialHandlers<I, D>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: EditIssueMaterialsCommand,
  ) -> Result<IssueMutationOutcome, ApplicationError> {
    let intent = &command.intent;
    self.authorize_issue(context, intent.project_id, intent.id).await?;
    intent.validate()?;
    let audit = MutationAuditContext::try_from(context).map_err(|_| ApplicationError::invalid())?;
    if let Some(receipt) = self
      .issues
      .issue_material_edit(audit.security_scope(), &command.idempotency_key)
      .await?
    {
      if receipt.intent != command.intent {
        return Err(
          octacity_server_store::StoreError::Conflict {
            entity: octacity_server_domain::EntityKind::Issue,
          }
          .into(),
        );
      }
      self.authorize_issue(context, intent.project_id, intent.id).await?;
      return Ok(IssueMutationOutcome {
        disposition: MutationDisposition::Replayed,
        ..receipt.outcome
      });
    }
    let current = self.issues.issue(intent.project_id, intent.id, None).await?;
    if current.version != intent.expected_version {
      return Err(
        octacity_server_store::StoreError::Conflict {
          entity: octacity_server_domain::EntityKind::Issue,
        }
        .into(),
      );
    }
    let mut expires_at: Option<octacity_server_domain::Timestamp> = None;
    for reference in &intent.references {
      self.authorize_source(context, reference).await?;
      match reference {
        IssueMaterialReference::Comment {
          project_id,
          issue_id,
          comment_id,
          body_digest,
        } => {
          let content = self
            .issues
            .issue_comment_body(*project_id, *issue_id, *comment_id)
            .await?;
          if content.comment.body_digest != *body_digest {
            return Err(ApplicationError::invalid());
          }
        }
        IssueMaterialReference::IssueDocument {
          project_id,
          issue_id,
          artifact,
        } => {
          let metadata = self
            .documents
            .issue_document_metadata(*project_id, *issue_id, artifact.artifact_id())
            .await?
            .ok_or_else(ApplicationError::invalid)?;
          let input = self
            .documents
            .issue_document_input(*project_id, *issue_id, artifact.artifact_id())
            .await?
            .ok_or_else(ApplicationError::invalid)?;
          if metadata.artifact != *artifact
            || metadata.expires_at <= self.clock.now()?
            || input.object.artifact != *artifact
            || input.mapping.as_str() != octacity_server_store::ISSUE_DOCUMENT_MAPPING
          {
            return Err(ApplicationError::invalid());
          }
          expires_at = Some(expires_at.map_or(metadata.expires_at, |until| until.min(metadata.expires_at)));
        }
        IssueMaterialReference::ProjectDocument { project_id, artifact } => {
          let metadata = self
            .documents
            .project_document_metadata(*project_id, artifact.artifact_id())
            .await?
            .ok_or_else(ApplicationError::invalid)?;
          let input = self
            .documents
            .project_input_artifact(*project_id, artifact.artifact_id())
            .await?
            .ok_or_else(ApplicationError::invalid)?;
          if metadata.artifact != *artifact
            || metadata.expires_at <= self.clock.now()?
            || input.object.artifact != *artifact
            || input.mapping.as_str() != octacity_server_store::PROJECT_CONTEXT_DOCUMENT_MAPPING
          {
            return Err(ApplicationError::invalid());
          }
          expires_at = Some(expires_at.map_or(metadata.expires_at, |until| until.min(metadata.expires_at)));
        }
      }
    }
    self.authorize_issue(context, intent.project_id, intent.id).await?;
    for reference in &intent.references {
      self.authorize_source(context, reference).await?;
    }
    let edited_at = self.clock.now()?;
    if expires_at.is_some_and(|until| edited_at >= until) {
      return Err(ApplicationError::invalid());
    }
    Ok(
      self
        .issues
        .edit_issue_materials(crate::management_security::audited_mutation(
          context,
          EditIssueMaterials {
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            edited_at,
          },
        )?)
        .await?,
    )
  }
}
