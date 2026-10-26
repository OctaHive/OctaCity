//! Project read choices require neither an existing task nor Factory preparation.
use crate::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, ProjectId};
use octacity_server_factory::{FactoryDigest, ImmutableReference, WorkSourceBinding};
use octacity_server_store::*;
use std::sync::Arc;

/// Current installed task readers for one reviewed Project/connection selection.
#[derive(Clone, Debug)]
pub struct ListProjectIssueSourceChoicesQuery {
  /// Existing connection discovery contract, without a Factory target or Repository.
  pub selection: ListWorkSourceInputChoicesQuery,
}
impl Query for ListProjectIssueSourceChoicesQuery {
  type Outcome = WorkSourceInputChoices;
}
impl ManagementAuthorizationTarget for ListProjectIssueSourceChoicesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ListWorkSourceInputChoicesQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.selection.management_resource()
  }
}
impl ManagementVisibilityTarget for ListProjectIssueSourceChoicesQuery {
  type Visibility = ProjectListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Project];
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    <ListWorkSourceInputChoicesQuery as ManagementVisibilityTarget>::visibility_from(grant)
  }
}

/// Freezes the original Project and connection metadata into a task read profile.
/// A current grant is still required at dispatch; its mutable revision is not reader identity.
pub fn project_issue_source_binding(
  mut source: WorkSourceBinding,
  project: ProjectId,
  connection: IntegrationId,
  version: IntegrationVersion,
) -> Result<WorkSourceBinding, ApplicationError> {
  let bytes = serde_json::to_vec(&(project, connection, version, &source)).map_err(|_| ApplicationError::invalid())?;
  source.configuration = ImmutableReference::new(
    source.configuration.identity().clone(),
    source.configuration.version().clone(),
    FactoryDigest::sha256("octacity.issue.project-source-profile.v1", &[&bytes]),
  );
  Ok(source)
}

/// Read-only discovery over the shared trusted installation and ordinary current-grant checks.
/// The provider may include observation-only readers; no publication command is exposed here.
pub struct ProjectIssueSourceChoiceHandlers<S> {
  provider: Arc<dyn FactorySourceConfigurationProvider<S>>,
}
impl<S> ProjectIssueSourceChoiceHandlers<S> {
  /// Installs trusted discovery without provider reads, imports or execution.
  pub fn new(provider: Arc<dyn FactorySourceConfigurationProvider<S>>) -> Self {
    Self { provider }
  }
}
#[async_trait::async_trait]
impl<S: IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore + 'static>
  ManagementQueryUseCase<ListProjectIssueSourceChoicesQuery> for ProjectIssueSourceChoiceHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListProjectIssueSourceChoicesQuery,
  ) -> Result<WorkSourceInputChoices, ApplicationError> {
    let project = query.selection.project_id;
    let mut choices = self
      .provider
      .for_project(project, context)?
      .execute_management_query(context, grant, query.selection)
      .await?;
    choices.items.retain(|c| c.purpose == IntegrationPurpose::TaskRead);
    for choice in &mut choices.items {
      choice.binding = project_issue_source_binding(
        choice.binding.clone(),
        project,
        choices.connection_id,
        choices.connection_version,
      )?;
    }
    Ok(choices)
  }
}
