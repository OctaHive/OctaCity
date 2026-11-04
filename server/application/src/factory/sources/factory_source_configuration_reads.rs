//! Bounded safe configuration reads reuse existing dependency visibility scopes.
use super::*;
use octacity_server_domain::ProjectId;
/// Inspection of one immutable input or group, including disabled and historical definitions.
#[derive(Clone, Debug)]
pub struct GetWorkSourceConfigurationQuery {
  /// Visible owner.
  pub project_id: ProjectId,
  /// Exact input or group version.
  pub target: WorkSourceScanTarget,
}
/// Current configured inputs/groups, independently of execution/polling progress.
#[derive(Clone, Debug)]
pub struct ListWorkSourceConfigurationsQuery {
  /// Visible owner.
  pub project_id: ProjectId,
  /// Last visible kind/identity position, if any.
  pub after: Option<WorkSourceScanTarget>,
  /// Bounded page size.
  pub limit: u16,
}
impl Query for GetWorkSourceConfigurationQuery {
  type Outcome = WorkSourceConfigurationRecord;
}
impl Query for ListWorkSourceConfigurationsQuery {
  type Outcome = WorkSourceConfigurationPage;
}
impl ManagementAuthorizationTarget for GetWorkSourceConfigurationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
impl ManagementAuthorizationTarget for ListWorkSourceConfigurationsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait]
impl<
  S: FactorySourceConfigurationStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore,
> ManagementQueryUseCase<ListWorkSourceConfigurationsQuery> for FactorySourceConfigurationHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListWorkSourceConfigurationsQuery,
  ) -> Result<WorkSourceConfigurationPage, ApplicationError> {
    if !grant
      .visibility_for::<ListWorkSourceConfigurationsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?
      .allows(&query.project_id)
    {
      return Err(ApplicationError::invalid());
    }
    self
      .require(
        context,
        ManagementResourceKind::Project,
        query.project_id,
        ManagementAction::View,
      )
      .await?;
    let visibility = self.source_visibility(context, query.project_id).await?;
    Ok(
      self
        .store
        .list_work_source_configurations(ListWorkSourceConfigurations {
          connection_dependency: None,
          project_id: query.project_id,
          after: query.after,
          limit: query.limit,
          visibility,
        })
        .await?,
    )
  }
}
#[async_trait]
impl<
  S: FactorySourceStore + IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore,
> ManagementQueryUseCase<GetWorkSourceConfigurationQuery> for FactorySourceConfigurationHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetWorkSourceConfigurationQuery,
  ) -> Result<WorkSourceConfigurationRecord, ApplicationError> {
    self
      .require(
        context,
        ManagementResourceKind::Project,
        query.project_id,
        ManagementAction::View,
      )
      .await?;
    let result = match query.target {
      WorkSourceScanTarget::Input { id, version } => self
        .store
        .source_input(query.project_id, id, version)
        .await
        .map(|input| WorkSourceConfigurationRecord::Input(Box::new(input))),
      WorkSourceScanTarget::Group { id, version } => self
        .store
        .source_group(query.project_id, id, version)
        .await
        .map(WorkSourceConfigurationRecord::Group),
    };
    let missing = || {
      ApplicationError::Store(StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::FactoryConfiguration,
      })
    };
    let result = result.map_err(|e| {
      if matches!(e, StoreError::Conflict { .. } | StoreError::NotFound { .. }) {
        missing()
      } else {
        e.into()
      }
    })?;
    for input in result.inputs() {
      self
        .require_input_visibility(context, input.definition())
        .await
        .map_err(|_| missing())?;
    }
    Ok(result)
  }
}

impl ManagementVisibilityTarget for ListWorkSourceConfigurationsQuery {
  type Visibility = ProjectListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Project];
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    <ListProjectsQuery as ManagementVisibilityTarget>::visibility_from(grant)
  }
}

impl<
  S: FactorySourceStore + IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore,
> FactorySourceConfigurationHandlers<S>
{
  pub(super) async fn source_visibility(
    &self,
    context: &ManagementRequestContext,
    project_id: ProjectId,
  ) -> Result<WorkSourceConfigurationVisibility, ApplicationError> {
    let project = ManagementResourceIdentity::new(project_id.to_string()).map_err(|_| ApplicationError::invalid())?;
    let configurations = self
      .policy
      .authorize(
        context,
        ManagementAction::View,
        &ManagementResource::owned_collection(
          ManagementResourceKind::FactoryConfiguration,
          ManagementResourceKind::Project,
          project.clone(),
        )
        .map_err(|_| ApplicationError::invalid())?,
      )
      .await
      .map_err(|_| ApplicationError::invalid())?
      .visibility_for::<ListProjectFactoryConfigurationsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let repositories = self
      .policy
      .authorize(
        context,
        ManagementAction::View,
        &ManagementResource::owned_collection(
          ManagementResourceKind::Repository,
          ManagementResourceKind::Project,
          project,
        )
        .map_err(|_| ApplicationError::invalid())?,
      )
      .await
      .map_err(|_| ApplicationError::invalid())?
      .visibility_for::<ListProjectRepositoriesQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let grant = self
      .policy
      .authorize(
        context,
        ManagementAction::View,
        &ManagementResource::collection(ManagementResourceKind::IntegrationConnection)
          .map_err(|_| ApplicationError::invalid())?,
      )
      .await
      .map_err(|_| ApplicationError::invalid())?;
    let connections = match grant.visibility().view() {
      ManagementVisibilityView::All => IntegrationConnectionListVisibility::all(),
      ManagementVisibilityView::None => IntegrationConnectionListVisibility::none(),
      ManagementVisibilityView::Restricted(resources) => {
        let ids = resources
          .iter()
          .map(|r| {
            if r.kind() != ManagementResourceKind::IntegrationConnection {
              return Err(ApplicationError::InvalidAuthorizationVisibility);
            }
            r.identity()
              .ok_or(ApplicationError::InvalidAuthorizationVisibility)?
              .as_str()
              .parse()
              .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)
          })
          .collect::<Result<Vec<octacity_server_domain::IntegrationId>, _>>()?;
        IntegrationConnectionListVisibility::restricted(ids)
          .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?
      }
    };
    Ok(WorkSourceConfigurationVisibility {
      configurations,
      repositories,
      connections,
    })
  }
}

/// Visible retained pipeline input versions using a connection in one Project.
/// Read-only history remains available after revocation; this grants no use authority.
#[derive(Clone, Debug)]
pub struct ListIntegrationDependenciesQuery {
  /// Connection whose native scope must remain visible.
  pub connection_id: octacity_server_domain::IntegrationId,
  /// Explicit visible owner; no global scan or hidden Project enumeration.
  pub project_id: ProjectId,
  /// Exact input identity/version position.
  pub after: Option<WorkSourceScanTarget>,
  /// Positive bounded page size.
  pub limit: u16,
}
impl Query for ListIntegrationDependenciesQuery {
  type Outcome = WorkSourceConfigurationPage;
}
impl ManagementAuthorizationTarget for ListIntegrationDependenciesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ListWorkSourceConfigurationsQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
impl ManagementVisibilityTarget for ListIntegrationDependenciesQuery {
  type Visibility = ProjectListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Project];
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    <ListProjectsQuery as ManagementVisibilityTarget>::visibility_from(grant)
  }
}
#[async_trait]
impl<
  S: FactorySourceConfigurationStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore,
> ManagementQueryUseCase<ListIntegrationDependenciesQuery> for FactorySourceConfigurationHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIntegrationDependenciesQuery,
  ) -> Result<WorkSourceConfigurationPage, ApplicationError> {
    if !grant
      .visibility_for::<ListIntegrationDependenciesQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?
      .allows(&query.project_id)
    {
      return Err(ApplicationError::invalid());
    }
    self
      .require(
        context,
        ManagementResourceKind::Project,
        query.project_id,
        ManagementAction::View,
      )
      .await?;
    self
      .require(
        context,
        ManagementResourceKind::IntegrationConnection,
        query.connection_id,
        ManagementAction::View,
      )
      .await?;
    Ok(
      self
        .store
        .list_work_source_configurations(ListWorkSourceConfigurations {
          connection_dependency: Some(query.connection_id),
          project_id: query.project_id,
          after: query.after,
          limit: query.limit,
          visibility: self.source_visibility(context, query.project_id).await?,
        })
        .await?,
    )
  }
}
