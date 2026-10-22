mod diagnostics;
use crate::{
  ApplicationError, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementResourceKind, ManagementResourceResult, Query, management_security::owned_collection_resource,
};
use async_trait::async_trait;
pub use diagnostics::{
  FactoryAuditProjection, FactoryCallProjection, FactoryDiagnosticCollection, FactoryDiagnosticPage,
  FactoryDiagnosticRecord, FactoryNodeAttemptProjection, FactoryNodeCompletionProjection, FactoryNodeStopProjection,
  FactoryWorkProjection, ListFactoryDiagnosticsQuery, MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,
};
use octacity_server_domain::ProjectId;
use octacity_server_store::{
  FactoryDiscoveryStore, FactoryRunFilter, FactoryRunListVisibility, FactoryRunPagePosition, ListFactoryRuns,
};
use std::sync::Arc;
/// Bounded Project-scoped or global Factory Run collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListFactoryRunsQuery {
  page: ListFactoryRuns,
}
impl ListFactoryRunsQuery {
  /// Validates filters and limits without observing any persisted resource.
  pub fn try_new(
    project: Option<ProjectId>,
    filter: FactoryRunFilter,
    after: Option<FactoryRunPagePosition>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    Ok(Self {
      page: ListFactoryRuns::new(project, filter, after, limit, FactoryRunListVisibility::none())?,
    })
  }
  /// Borrows the validated page intent; the handler supplies policy visibility.
  #[must_use]
  pub const fn page(&self) -> &ListFactoryRuns {
    &self.page
  }
}
impl Query for ListFactoryRunsQuery {
  type Outcome = octacity_server_store::FactoryRunPage;
}
impl ManagementAuthorizationTarget for ListFactoryRunsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::collection_or_owned_collection(
    ManagementAction::View,
    ManagementResourceKind::FactoryRun,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    match self.page.project_id() {
      Some(project) => owned_collection_resource(
        ManagementResourceKind::FactoryRun,
        ManagementResourceKind::Project,
        project,
      ),
      None => crate::ManagementResource::collection(ManagementResourceKind::FactoryRun),
    }
  }
}
/// Authorization-aware discovery over the same authoritative store used by admission.
pub struct FactoryDiscoveryHandlers<S> {
  issue_associations: Option<Arc<dyn octacity_server_store::IssueExecutionStore>>,
  store: Arc<S>,
  policy: Arc<dyn crate::ManagementAuthorizationPolicy>,
}
impl<S> FactoryDiscoveryHandlers<S> {
  /// Creates transport-independent discovery handlers.
  #[must_use]
  pub fn new(store: Arc<S>) -> Self {
    Self::with_policy(store, Arc::new(crate::TrustedNetworkManagementPolicy))
  }
  /// Uses the same policy for the immutable owning Project before returning diagnostics.
  pub fn with_policy(store: Arc<S>, policy: Arc<dyn crate::ManagementAuthorizationPolicy>) -> Self {
    Self {
      store,
      policy,
      issue_associations: None,
    }
  }
  /// Adds optional task navigation without making Run discovery depend on Issue availability.
  pub fn with_issue_associations(mut self, store: Arc<dyn octacity_server_store::IssueExecutionStore>) -> Self {
    self.issue_associations = Some(store);
    self
  }
}
#[async_trait]
impl<S: FactoryDiscoveryStore + 'static> crate::ManagementQueryUseCase<ListFactoryRunsQuery>
  for FactoryDiscoveryHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: ListFactoryRunsQuery,
  ) -> Result<octacity_server_store::FactoryRunPage, ApplicationError> {
    let page = query.page();
    let visibility = grant
      .visibility_for::<ListFactoryRunsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    Ok(
      self
        .store
        .list_factory_runs(ListFactoryRuns::new(
          page.project_id(),
          page.filter().clone(),
          page.after(),
          page.limit().get(),
          visibility,
        )?)
        .await?,
    )
  }
}

/// Bounded visible current entries of an exact immutable phase policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListFactoryPhaseReadyQuery {
  page: octacity_server_store::ListPhaseReadyEntries,
}
impl ListFactoryPhaseReadyQuery {
  /// Validates page bounds and a continuation bound to the same exact policy.
  pub fn try_new(
    project: Option<ProjectId>,
    policy: octacity_server_factory::FactoryDigest,
    after: Option<octacity_server_store::PhaseReadyPagePosition>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    Ok(Self {
      page: octacity_server_store::ListPhaseReadyEntries::new(
        project,
        policy,
        after,
        limit,
        FactoryRunListVisibility::none(),
      )?,
    })
  }
  /// Borrows the validated intent before handler-selected visibility.
  #[must_use]
  pub const fn page(&self) -> &octacity_server_store::ListPhaseReadyEntries {
    &self.page
  }
}
impl Query for ListFactoryPhaseReadyQuery {
  type Outcome = octacity_server_store::PhaseReadyPage;
}
impl ManagementAuthorizationTarget for ListFactoryPhaseReadyQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ListFactoryRunsQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    match self.page.project_id() {
      Some(project) => owned_collection_resource(
        ManagementResourceKind::FactoryRun,
        ManagementResourceKind::Project,
        project,
      ),
      None => crate::ManagementResource::collection(ManagementResourceKind::FactoryRun),
    }
  }
}
#[async_trait]
impl<S: FactoryDiscoveryStore + 'static> crate::ManagementQueryUseCase<ListFactoryPhaseReadyQuery>
  for FactoryDiscoveryHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: ListFactoryPhaseReadyQuery,
  ) -> Result<octacity_server_store::PhaseReadyPage, ApplicationError> {
    let page = query.page();
    let visibility = grant
      .visibility_for::<ListFactoryPhaseReadyQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    Ok(
      self
        .store
        .list_phase_ready_entries(octacity_server_store::ListPhaseReadyEntries::new(
          page.project_id(),
          page.policy(),
          page.after().cloned(),
          page.limit(),
          visibility,
        )?)
        .await?,
    )
  }
}

/// Reads safe current facts for one visible Factory Run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetFactoryRunQuery {
  /// Stable Run identity, independent of logical Flow names.
  pub run_id: octacity_server_factory::FactoryRunId,
}
impl Query for GetFactoryRunQuery {
  type Outcome = octacity_server_store::FactoryRunSummary;
}
impl ManagementAuthorizationTarget for GetFactoryRunQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::FactoryRun);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::FactoryRun, self.run_id)
  }
}
impl crate::ManagementVisibilityTarget for GetFactoryRunQuery {
  type Visibility = FactoryRunListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] =
    <ListFactoryRunsQuery as crate::ManagementVisibilityTarget>::RESTRICTED_RESOURCE_KINDS;
  fn visibility_from(
    grant: &crate::ManagementAuthorizationGrant,
  ) -> Result<Self::Visibility, crate::ManagementVisibilityError> {
    <ListFactoryRunsQuery as crate::ManagementVisibilityTarget>::visibility_from(grant)
  }
}
#[async_trait]
impl<S: FactoryDiscoveryStore + 'static> crate::ManagementQueryUseCase<GetFactoryRunQuery>
  for FactoryDiscoveryHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: GetFactoryRunQuery,
  ) -> Result<octacity_server_store::FactoryRunSummary, ApplicationError> {
    let visibility = grant
      .visibility_for::<GetFactoryRunQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    self.visible_run_summary(context, query.run_id, visibility).await
  }
}

impl<S: FactoryDiscoveryStore> FactoryDiscoveryHandlers<S> {
  pub(crate) async fn visible_run_summary(
    &self,
    context: &crate::ManagementRequestContext,
    run_id: octacity_server_factory::FactoryRunId,
    visibility: FactoryRunListVisibility,
  ) -> Result<octacity_server_store::FactoryRunSummary, ApplicationError> {
    let mut summary = self.store.factory_run_summary(run_id, visibility).await?;
    let not_found = || {
      ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::FactoryRun,
      })
    };
    let project = crate::management_security::instance_resource(ManagementResourceKind::Project, summary.project_id)
      .map_err(|_| ApplicationError::invalid())?;
    let grant = self
      .policy
      .authorize(context, ManagementAction::View, &project)
      .await
      .map_err(|_| not_found())?;
    match grant.visibility().view() {
      crate::ManagementVisibilityView::All => {}
      crate::ManagementVisibilityView::Restricted(resources) if resources.contains(&project) => {}
      _ => return Err(not_found()),
    }
    if let Some(reader) = &self.issue_associations
      && let Ok(Some(association)) = reader.issue_work_association(summary.project_id, summary.work_id).await
      && association.project_id == summary.project_id
      && association.work_id == summary.work_id
      && association.run_id == summary.id
    {
      let resource = crate::management_security::instance_resource(ManagementResourceKind::Issue, association.issue_id)
        .map_err(|_| ApplicationError::invalid())?;
      if let Ok(grant) = self.policy.authorize(context, ManagementAction::View, &resource).await
        && (matches!(grant.visibility().view(), crate::ManagementVisibilityView::All)
          || matches!(grant.visibility().view(),crate::ManagementVisibilityView::Restricted(resources) if resources.contains(&resource)))
      {
        summary.issue_association = Some(association);
      }
    }
    Ok(summary)
  }
}
