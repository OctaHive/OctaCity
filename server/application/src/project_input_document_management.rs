//! Ordinary management contracts over the shared Project input owner.
use super::*;
use octacity_server_domain::ArtifactId;
pub use octacity_server_store::ProjectDocumentPage;

/// Exact retained document content, returned only after current Project and Artifact authorization.
#[derive(Clone, serde::Serialize)]
pub struct ProjectDocumentContent {
  /// Owning Project, independently checked against publication metadata.
  pub project_id: octacity_server_domain::ProjectId,
  /// Immutable content identity; a revision is a separate ordinary publication.
  pub artifact: octacity_server_factory::FactoryArtifactReference,
  /// Original bounded UTF-8 bytes; never included in diagnostics.
  pub content: String,
}
impl std::fmt::Debug for ProjectDocumentContent {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ProjectDocumentContent")
      .field("project_id", &self.project_id)
      .field("artifact", &self.artifact)
      .finish_non_exhaustive()
  }
}
/// Selects one named Project document, without requiring an Issue, Run or Build.
#[derive(Clone, Debug)]
pub struct ReadProjectInputDocumentQuery {
  /// Exact owning Project.
  pub project_id: octacity_server_domain::ProjectId,
  /// Exact immutable retained document.
  pub artifact_id: ArtifactId,
}
impl Query for ReadProjectInputDocumentQuery {
  type Outcome = ProjectDocumentContent;
}
impl ManagementAuthorizationTarget for ReadProjectInputDocumentQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Download, ManagementResourceKind::Artifact);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Artifact, self.artifact_id)
  }
}

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

#[async_trait::async_trait]
impl<
  S: ProjectTextInputStore + ProjectDocumentInputStore + 'static,
  B: ArtifactBytePublisher + ArtifactByteReader + 'static,
> ManagementQueryUseCase<ReadProjectInputDocumentQuery> for ProjectTextIntake<S, B>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ReadProjectInputDocumentQuery,
  ) -> Result<ProjectDocumentContent, ApplicationError> {
    self.authorize_document_read(context, &query).await?;
    let now = self.clock.now()?;
    let not_found = || {
      ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Artifact,
      })
    };
    let input = self
      .store
      .project_input_artifact(query.project_id, query.artifact_id)
      .await?
      .filter(|input| {
        input.mapping.as_str() == octacity_server_store::PROJECT_CONTEXT_DOCUMENT_MAPPING
          && input.object.role == octacity_server_store::FactoryInputRole::Document
          && now < input.expires_at
      })
      .ok_or_else(not_found)?;
    let artifact = input.object.artifact.clone();
    let expires_at = input.expires_at;
    let retained = crate::factory_artifact_reader::read_project_input_object(
      self.bytes.as_ref(),
      &ReadFactoryArtifact {
        project_id: query.project_id,
        artifact: artifact.clone(),
        maximum_bytes: u64::from(MAX_PROJECT_DOCUMENT_BYTES),
        observed_at: now,
        required_until: expires_at,
      },
      input,
    )
    .await?;
    self.authorize_document_read(context, &query).await?;
    if self.clock.now()? >= expires_at {
      return Err(not_found());
    }
    let content = String::from_utf8(retained.bytes().to_vec()).map_err(|_| ApplicationError::unavailable())?;
    Ok(ProjectDocumentContent {
      project_id: query.project_id,
      artifact,
      content,
    })
  }
}
impl<S, B> ProjectTextIntake<S, B> {
  async fn authorize_document_read(
    &self,
    context: &ManagementRequestContext,
    query: &ReadProjectInputDocumentQuery,
  ) -> Result<(), ApplicationError> {
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.project_id,
    )
    .await?;
    crate::factory_configuration_choices::require_authorized_reference(
      self.policy.as_ref(),
      context,
      ManagementAction::Download,
      ManagementResourceKind::Artifact,
      query.artifact_id,
    )
    .await
  }
}
