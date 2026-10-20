//! Safe business metadata; requirements, execution and synchronization have separate reads.
use super::*;

/// Reads current business metadata and its retained workflow without Factory dependencies.
#[derive(Clone, Debug)]
pub struct GetIssueCardQuery {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Visible Issue identity.
  pub id: IssueId,
}
impl Query for GetIssueCardQuery {
  type Outcome = IssueCardProjection;
}
impl ManagementAuthorizationTarget for GetIssueCardQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.id)
  }
}
/// One business observation, with no acceptance verdict or embedded execution aggregate.
#[derive(Clone, Debug, serde::Serialize)]
pub struct IssueCardProjection {
  /// Safe current task metadata; dedicated reads own requirement bodies.
  pub issue: octacity_server_store::IssueSummary,
  /// Organizational priority; never a change to admitted Build scheduling.
  pub priority: Option<u8>,
  /// Current optional organizational properties; independent of worker/profile selection.
  pub planning: octacity_server_store::IssuePlanning,
  /// Original pinned policy, never the Project's later default.
  pub workflow: octacity_server_store::PublishedIssueWorkflow,
}
#[async_trait::async_trait]
impl<S: IssueStore + IssueWorkflowStore> ManagementQueryUseCase<GetIssueCardQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueCardQuery,
  ) -> Result<IssueCardProjection, ApplicationError> {
    visible_issue(grant, query.id)?;
    let issue = self.store.issue(query.project_id, query.id, None).await?;
    let workflow = self
      .store
      .issue_workflow(query.project_id, issue.workflow_id, Some(issue.workflow_version))
      .await?;
    Ok(IssueCardProjection {
      issue: (&issue).into(),
      priority: issue.content.priority,
      planning: issue.planning,
      workflow,
    })
  }
}
