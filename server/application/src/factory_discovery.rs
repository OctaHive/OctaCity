use crate::{
  ApplicationError, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementResourceKind, ManagementResourceResult, Query, management_security::owned_collection_resource,
};
use async_trait::async_trait;
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
  store: Arc<S>,
}
impl<S> FactoryDiscoveryHandlers<S> {
  /// Creates transport-independent discovery handlers.
  #[must_use]
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
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
