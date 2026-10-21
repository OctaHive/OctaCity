//! Shared operator discovery; original Run controls remain the only mutation authority.
use crate::*;
use octacity_server_domain::{IssueId, ProjectId, Timestamp};
use octacity_server_factory::FactoryDigest;
pub use octacity_server_store::{
  FactoryActionProjectionCoverage, FactoryActionQueueItem, FactoryActionQueuePage, FactoryActionQueuePosition,
  FactoryActionQueueStore, ReadFactoryActionQueue,
};
use octacity_server_store::{FactoryRunListVisibility, IssueListVisibility};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
const CONTRACT: &str = "factory-actions-v1";
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActionCursor {
  scope: FactoryDigest,
  position: FactoryActionQueuePosition,
}
/// Visible shared collection, optionally confined to a Project.
#[derive(Clone, Debug)]
pub struct ListFactoryActionsQuery {
  project: Option<ProjectId>,
  after: Option<ActionCursor>,
  limit: u16,
}
impl ListFactoryActionsQuery {
  /// Validates common page/canonical cursor bounds without reading or reserving execution.
  pub fn try_new(project: Option<ProjectId>, after: Option<String>, limit: u16) -> Result<Self, ApplicationError> {
    octacity_server_store::validate_action_projection_limit(limit)?;
    let after = after
      .map(|v| {
        crate::opaque_cursor::decode_canonical(&v, MAX_ISSUE_OBSERVATION_CURSOR_BYTES)
          .map_err(|_| ApplicationError::invalid())
      })
      .transpose()?;
    Ok(Self { project, after, limit })
  }
}
impl Query for ListFactoryActionsQuery {
  type Outcome = FactoryActionsPage;
}
impl ManagementAuthorizationTarget for ListFactoryActionsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ListFactoryRunsQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    match self.project {
      Some(project) => crate::management_security::owned_collection_resource(
        ManagementResourceKind::FactoryRun,
        ManagementResourceKind::Project,
        project,
      ),
      None => ManagementResource::collection(ManagementResourceKind::FactoryRun),
    }
  }
}
impl ManagementVisibilityTarget for ListFactoryActionsQuery {
  type Visibility = FactoryRunListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::FactoryRun];
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    grant.visibility_for::<ListFactoryRunsQuery>()
  }
}
/// One Issue's associated execution actions, independent of editable Issue version.
#[derive(Clone, Debug)]
pub struct ListIssueFactoryActionsQuery {
  issue: IssueId,
  page: ListFactoryActionsQuery,
}
impl ListIssueFactoryActionsQuery {
  /// Retains the exact local task selection; a cursor cannot retarget another card.
  pub fn try_new(
    project: ProjectId,
    issue: IssueId,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    Ok(Self {
      issue,
      page: ListFactoryActionsQuery::try_new(Some(project), after, limit)?,
    })
  }
}
impl Query for ListIssueFactoryActionsQuery {
  type Outcome = FactoryActionsPage;
}
impl ManagementAuthorizationTarget for ListIssueFactoryActionsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue)
  }
}
impl ManagementVisibilityTarget for ListIssueFactoryActionsQuery {
  type Visibility = IssueListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Issue];
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    grant.visibility_for::<ListProjectIssuesQuery>()
  }
}
/// Current safe collection observation; count completeness is qualified explicitly.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryActionsPage {
  /// Current indexed matching visible count; incomplete coverage may omit actions.
  pub total: u64,
  /// Whether all matching visible source versions have been materialized.
  pub coverage: FactoryActionProjectionCoverage,
  /// Exact common response/retry contracts; never private command bodies.
  pub items: Vec<FactoryActionQueueItem>,
  /// Continuation bound to Project/Issue, security partition and all current read scopes.
  pub next_cursor: Option<String>,
  /// Trusted read time used for expiry; no client-supplied observation time is accepted.
  pub observed_at: Timestamp,
}
/// Composes the read index with existing independent authorization and trusted time.
pub struct FactoryActionHandlers<S> {
  pub(crate) associations: Option<Arc<dyn octacity_server_store::IssueExecutionStore>>,
  pub(crate) store: Arc<S>,
  pub(crate) policy: Arc<dyn ManagementAuthorizationPolicy>,
  pub(crate) clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> FactoryActionHandlers<S> {
  /// Reuses the original Work association reader for optional Issue-scoped target observation.
  pub fn with_issue_associations(mut self, store: Arc<dyn octacity_server_store::IssueExecutionStore>) -> Self {
    self.associations = Some(store);
    self
  }

  /// Installs read-only actions independently of providers and execution configuration.
  pub fn new(
    store: Arc<S>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      store,
      policy,
      clock,
      associations: None,
    }
  }
  async fn visibility(
    &self,
    context: &ManagementRequestContext,
    resource: ManagementResourceResult,
  ) -> Result<ManagementAuthorizationGrant, ApplicationError> {
    let resource = resource.map_err(|_| ApplicationError::invalid())?;
    Ok(
      self
        .policy
        .authorize(context, ManagementAction::View, &resource)
        .await
        .unwrap_or_else(|_| ManagementAuthorizationGrant::new(ManagementVisibility::none())),
    )
  }
}
impl<S: FactoryActionQueueStore> FactoryActionHandlers<S> {
  async fn list(
    &self,
    context: &ManagementRequestContext,
    run_grant: &ManagementAuthorizationGrant,
    issue_grant: &ManagementAuthorizationGrant,
    issue: Option<IssueId>,
    page: ListFactoryActionsQuery,
  ) -> Result<FactoryActionsPage, ApplicationError> {
    let runs = run_grant
      .visibility_for::<ListFactoryRunsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let issues = issue_grant
      .visibility_for::<ListProjectIssuesQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let resource = match page.project {
      Some(project) => crate::management_security::instance_resource(ManagementResourceKind::Project, project),
      None => ManagementResource::collection(ManagementResourceKind::Project),
    };
    let projects = self
      .visibility(context, resource)
      .await?
      .visibility_for::<ListProjectsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let selection = serde_json::json!({"project":page.project,"issue":issue});
    let issue_scope = crate::issue_activity::observation_scope_for(context, issues.view(), selection.clone(), CONTRACT);
    let project_scope =
      crate::issue_activity::observation_scope_for(context, projects.view(), selection.clone(), CONTRACT);
    let scope = crate::issue_activity::observation_scope_for(
      context,
      runs.view(),
      serde_json::json!({"selection":selection,"issues":issue_scope,"projects":project_scope}),
      CONTRACT,
    );
    if page.after.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let observed_at = self.clock.now()?;
    let result = self
      .store
      .read_factory_action_queue(ReadFactoryActionQueue {
        project_id: page.project,
        issue_id: issue,
        after: page.after.map(|c| c.position),
        limit: page.limit,
        observed_at,
        runs,
        issues,
        projects,
      })
      .await?;
    Ok(FactoryActionsPage {
      total: result.total,
      coverage: result.coverage,
      items: result.items,
      next_cursor: result
        .next
        .map(|position| crate::opaque_cursor::encode(&ActionCursor { scope, position })),
      observed_at,
    })
  }
}
#[async_trait::async_trait]
impl<S: FactoryActionQueueStore + 'static> ManagementQueryUseCase<ListFactoryActionsQuery>
  for FactoryActionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListFactoryActionsQuery,
  ) -> Result<FactoryActionsPage, ApplicationError> {
    let resource = match query.project {
      Some(project) => crate::management_security::owned_collection_resource(
        ManagementResourceKind::Issue,
        ManagementResourceKind::Project,
        project,
      ),
      None => ManagementResource::collection(ManagementResourceKind::Issue),
    };
    let issues = self.visibility(context, resource).await?;
    self.list(context, grant, &issues, None, query).await
  }
}
#[async_trait::async_trait]
impl<S: FactoryActionQueueStore + 'static> ManagementQueryUseCase<ListIssueFactoryActionsQuery>
  for FactoryActionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueFactoryActionsQuery,
  ) -> Result<FactoryActionsPage, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue)?;
    let runs = self.visibility(context, query.page.management_resource()).await?;
    self.list(context, &runs, grant, Some(query.issue), query.page).await
  }
}
