//! Issue-scoped publication and reads over the existing immutable input lifecycle.
use super::*;
use octacity_server_domain::{ArtifactId, IssueId, ProjectId};
use octacity_server_store::{IssueDocumentInputStore, ListIssueDocuments, ListProjectDocuments};
pub use octacity_server_store::{IssueDocumentPage, IssueDocumentResource, IssueDocumentSubmission};

/// A reviewed task document; owner and replay identity are assigned by the caller's Issue route.
#[derive(Clone, Debug)]
pub struct PublishIssueDocumentCommand {
  /// Bounded private text and exact parent, with no supplied actor or execution selection.
  pub submission: IssueDocumentSubmission,
}
impl Command for PublishIssueDocumentCommand {
  type Outcome = IssueDocumentResource;
}
impl ManagementAuthorizationTarget for PublishIssueDocumentCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Update, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.submission.issue_id)
  }
}
/// Bounded owner-specific document discovery, including retained immutable versions.
#[derive(Clone, Debug)]
pub struct ListIssueDocumentsQuery {
  page: CurrentDefinitionPageInput<ArtifactId>,
  issue_id: IssueId,
}
impl ListIssueDocumentsQuery {
  /// Applies the common discovery bounds before authorization or storage access.
  pub fn try_new(
    project: ProjectId,
    issue: IssueId,
    after: Option<ArtifactId>,
    limit: u16,
  ) -> Result<Self, CurrentDefinitionPageError> {
    Ok(Self {
      page: CurrentDefinitionPageInput::try_new(project, after, limit)?,
      issue_id: issue,
    })
  }
}
impl Query for ListIssueDocumentsQuery {
  type Outcome = IssueDocumentPage;
}
impl ManagementAuthorizationTarget for ListIssueDocumentsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::View,
    ManagementResourceKind::Artifact,
    ManagementResourceKind::Issue,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::owned_collection_resource(
      ManagementResourceKind::Artifact,
      ManagementResourceKind::Issue,
      self.issue_id,
    )
  }
}
/// Exact text read requiring parent Issue visibility and independent Artifact download permission.
#[derive(Clone, Debug)]
pub struct ReadIssueDocumentQuery {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact parent Issue.
  pub issue_id: IssueId,
  /// Exact publication, never a newest-version selector.
  pub artifact_id: ArtifactId,
}
impl Query for ReadIssueDocumentQuery {
  type Outcome = IssueDocumentContent;
}
impl ManagementAuthorizationTarget for ReadIssueDocumentQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Download, ManagementResourceKind::Artifact);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Artifact, self.artifact_id)
  }
}
/// Independently authorized exact text; private bytes are omitted from diagnostics.
#[derive(Clone, serde::Serialize)]
pub struct IssueDocumentContent {
  /// Actual document owner and immutable publication metadata.
  pub document: IssueDocumentResource,
  /// Exact bounded UTF-8 content, for dedicated no-store transport only.
  pub content: String,
}
impl std::fmt::Debug for IssueDocumentContent {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IssueDocumentContent")
      .field("document", &self.document)
      .finish_non_exhaustive()
  }
}

impl<S, B> ProjectTextIntake<S, B> {
  pub(super) async fn authorize_issue_document(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    issue: IssueId,
    action: ManagementAction,
  ) -> Result<(), ApplicationError> {
    use crate::factory::authoring::factory_configuration_choices::{
      require_authorized_reference, require_visible_reference,
    };
    require_visible_reference(self.policy.as_ref(), context, ManagementResourceKind::Project, project).await?;
    if action != ManagementAction::View {
      require_visible_reference(self.policy.as_ref(), context, ManagementResourceKind::Issue, issue).await?;
    }
    require_authorized_reference(
      self.policy.as_ref(),
      context,
      action,
      ManagementResourceKind::Issue,
      issue,
    )
    .await?;
    self
      .issues
      .as_ref()
      .ok_or_else(ApplicationError::unavailable)?
      .issue(project, issue, None)
      .await?;
    Ok(())
  }
}
#[async_trait::async_trait]
impl<
  S: ProjectTextInputStore + ProjectDocumentInputStore + 'static,
  B: ArtifactBytePublisher + ArtifactByteReader + 'static,
> ManagementCommandUseCase<PublishIssueDocumentCommand> for ProjectTextIntake<S, B>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: PublishIssueDocumentCommand,
  ) -> Result<IssueDocumentResource, ApplicationError> {
    let submission = command.submission;
    self
      .authorize_issue_document(
        context,
        submission.project_id,
        submission.issue_id,
        ManagementAction::Update,
      )
      .await?;
    let record = self
      .store
      .reserve_project_document(crate::management_security::audited_mutation(
        context,
        submission.into_reservation(self.clock.now()?)?,
      )?)
      .await?;
    if record.published_at.is_some() {
      return Ok(record.issue_resource()?);
    }
    let result = self.publish_context_document(context, &record, None).await;
    if result.is_err() {
      let _ = self
        .store
        .release_project_document(record.guard(self.clock.now()?))
        .await;
    }
    Ok(result?.issue_resource()?)
  }
}
#[async_trait::async_trait]
impl<S: IssueDocumentInputStore + 'static, B: Send + Sync + 'static> ManagementQueryUseCase<ListIssueDocumentsQuery>
  for ProjectTextIntake<S, B>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueDocumentsQuery,
  ) -> Result<IssueDocumentPage, ApplicationError> {
    let project = query.page.project_id();
    self
      .authorize_issue_document(context, project, query.issue_id, ManagementAction::View)
      .await?;
    let visibility = grant
      .visibility_for::<ListIssueDocumentsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let result = self
      .store
      .list_issue_documents(ListIssueDocuments {
        issue_id: query.issue_id,
        page: ListProjectDocuments {
          project_id: project,
          after: query.page.after().copied(),
          limit: query.page.limit(),
          visibility,
          observed_at: self.clock.now()?,
        },
      })
      .await?;
    self
      .authorize_issue_document(context, project, query.issue_id, ManagementAction::View)
      .await?;
    for document in &result.items {
      crate::factory::authoring::factory_configuration_choices::require_visible_reference(
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
impl<S: IssueDocumentInputStore + 'static, B: ArtifactByteReader + 'static>
  ManagementQueryUseCase<ReadIssueDocumentQuery> for ProjectTextIntake<S, B>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ReadIssueDocumentQuery,
  ) -> Result<IssueDocumentContent, ApplicationError> {
    self
      .authorize_issue_document(context, query.project_id, query.issue_id, ManagementAction::View)
      .await?;
    self
      .authorize_document_read(
        context,
        &ReadProjectInputDocumentQuery {
          project_id: query.project_id,
          artifact_id: query.artifact_id,
        },
      )
      .await?;
    let missing = || {
      ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Artifact,
      })
    };
    let now = self.clock.now()?;
    let document = self
      .store
      .issue_document_metadata(query.project_id, query.issue_id, query.artifact_id)
      .await?
      .filter(|doc| doc.expires_at > now)
      .ok_or_else(missing)?;
    let input = self
      .store
      .issue_document_input(query.project_id, query.issue_id, query.artifact_id)
      .await?
      .filter(|input| input.mapping.as_str() == octacity_server_store::ISSUE_DOCUMENT_MAPPING)
      .ok_or_else(missing)?;
    let retained = crate::factory::artifacts::factory_artifact_reader::read_project_input_object(
      self.bytes.as_ref(),
      &ReadFactoryArtifact {
        project_id: query.project_id,
        artifact: document.artifact.clone(),
        maximum_bytes: u64::from(MAX_PROJECT_DOCUMENT_BYTES),
        observed_at: now,
        required_until: document.expires_at,
      },
      input,
    )
    .await?;
    self
      .authorize_issue_document(context, query.project_id, query.issue_id, ManagementAction::View)
      .await?;
    self
      .authorize_document_read(
        context,
        &ReadProjectInputDocumentQuery {
          project_id: query.project_id,
          artifact_id: query.artifact_id,
        },
      )
      .await?;
    if self.clock.now()? >= document.expires_at {
      return Err(missing());
    }
    let content = String::from_utf8(retained.bytes().to_vec()).map_err(|_| ApplicationError::unavailable())?;
    Ok(IssueDocumentContent { document, content })
  }
}
