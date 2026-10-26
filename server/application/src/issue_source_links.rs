//! The task owner supplies read-only native references to existing Source preview and diagnostics.
use super::*;
use crate::{FactoryWorkSourceIssueLinks, WorkSourceIssueLink};
use octacity_server_store::IssueSourceStore;
#[async_trait::async_trait]
impl<S: IssueSourceStore> FactoryWorkSourceIssueLinks for IssueHandlers<S> {
  async fn visible_issue(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    native: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<WorkSourceIssueLink>, ApplicationError> {
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      project,
    )
    .await?;
    let Some(link) = self
      .store
      .native_issue_source_link(project, &context.security_scope().to_store(), native)
      .await?
    else {
      return Ok(None);
    };
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Issue,
      link.issue_id,
    )
    .await?;
    self.store.issue(project, link.issue_id, None).await?;
    Ok(Some(WorkSourceIssueLink {
      project_id: project,
      issue_id: ManagementResourceIdentity::new(link.issue_id.to_string()).map_err(|_| ApplicationError::invalid())?,
    }))
  }
}
