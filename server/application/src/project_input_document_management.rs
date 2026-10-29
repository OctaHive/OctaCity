//! Ordinary management contracts over the shared Project input owner.
use super::*;
use octacity_server_domain::ArtifactId;
pub use octacity_server_store::ProjectDocumentPage;

/// One reviewed named document; the path-owned Project and header replay key are assigned by transport.
#[derive(Clone, Debug)]
pub struct PublishProjectDocumentCommand {
  /// Exact private input, with no Configuration, Run or execution authority.
  pub submission: ProjectDocumentSubmission,
}
impl Command for PublishProjectDocumentCommand {
  type Outcome = ProjectDocumentResource;
}
impl ManagementAuthorizationTarget for PublishProjectDocumentCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Publish, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.submission.project_id)
  }
}
/// Read-only bounded choices under current Artifact visibility and the owning Project.
#[derive(Clone, Debug)]
pub struct ListProjectInputDocumentsQuery {
  /// Exclusive immutable identity cursor and existing common discovery ceiling.
  pub page: CurrentDefinitionPageInput<ArtifactId>,
}
impl Query for ListProjectInputDocumentsQuery {
  type Outcome = ProjectDocumentPage;
}
impl ManagementAuthorizationTarget for ListProjectInputDocumentsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::View,
    ManagementResourceKind::Artifact,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::owned_collection_resource(
      ManagementResourceKind::Artifact,
      ManagementResourceKind::Project,
      self.page.project_id(),
    )
  }
}
#[async_trait::async_trait]
impl<
  S: ProjectTextInputStore + ProjectDocumentInputStore + 'static,
  B: ArtifactBytePublisher + ArtifactByteReader + 'static,
> ManagementCommandUseCase<PublishProjectDocumentCommand> for ProjectTextIntake<S, B>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PublishProjectDocumentCommand,
  ) -> Result<ProjectDocumentResource, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, command.submission.project_id)?;
    self.publish_document(context, command.submission).await
  }
}
#[async_trait::async_trait]
impl<
  S: ProjectTextInputStore + ProjectDocumentInputStore + 'static,
  B: ArtifactBytePublisher + ArtifactByteReader + 'static,
> ManagementQueryUseCase<ListProjectInputDocumentsQuery> for ProjectTextIntake<S, B>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListProjectInputDocumentsQuery,
  ) -> Result<ProjectDocumentPage, ApplicationError> {
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.page.project_id(),
    )
    .await?;
    let visibility = grant
      .visibility_for::<ListProjectInputDocumentsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let result = self
      .store
      .list_project_documents(octacity_server_store::ListProjectDocuments {
        project_id: query.page.project_id(),
        after: query.page.after().copied(),
        limit: query.page.limit(),
        visibility,
        observed_at: self.clock.now()?,
      })
      .await?;
    // Recheck after the asynchronous read; a stale grant cannot disclose a newly hidden document.
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.page.project_id(),
    )
    .await?;
    for document in &result.items {
      crate::factory_configuration_choices::require_visible_reference(
        self.policy.as_ref(),
        context,
        ManagementResourceKind::Artifact,
        document.artifact.artifact_id(),
      )
      .await?;
    }
    Ok(result)
  }
}
