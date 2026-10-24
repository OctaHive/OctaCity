//! Project-scoped safe role catalogs and explicit release-template content reads.
use crate::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::{FactoryConfigurationStore, FactoryRoleProfileStore};
pub use octacity_server_store::{FactoryRoleCatalogPage, FactoryRoleProfileSummary, FactoryRoleSummary};

/// Safe current AI-agent catalog in stable role identity order.
#[derive(Clone, Debug)]
pub struct ListFactoryRolesQuery {
  /// Existing Project-scoped bounded page input.
  pub page: CurrentDefinitionPageInput<FactoryRoleId>,
}
impl Query for ListFactoryRolesQuery {
  type Outcome = FactoryRoleCatalogPage<FactoryRoleId, FactoryRoleSummary>;
}
impl ManagementAuthorizationTarget for ListFactoryRolesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.page.project_id())
  }
}
/// Safe current profiles owned by one role.
#[derive(Clone, Debug)]
pub struct ListFactoryRoleProfilesQuery {
  /// Existing owning Project and bounded continuation.
  pub page: CurrentDefinitionPageInput<FactoryRoleProfileId>,
  /// Owned role identity, independent of its current publication version.
  pub role_id: FactoryRoleId,
}
impl Query for ListFactoryRoleProfilesQuery {
  type Outcome = FactoryRoleCatalogPage<FactoryRoleProfileId, FactoryRoleProfileSummary>;
}
impl ManagementAuthorizationTarget for ListFactoryRoleProfilesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.page.project_id())
  }
}
/// Release starter catalog, available before Factory execution is enabled.
#[derive(Clone, Debug)]
pub struct ListFactoryRoleTemplatesQuery {
  /// Project ownership and bounded alias continuation.
  pub page: CurrentDefinitionPageInput<FactoryKey>,
}
impl Query for ListFactoryRoleTemplatesQuery {
  type Outcome = FactoryRoleCatalogPage<FactoryKey, FactoryRoleTemplateSummary>;
}
impl ManagementAuthorizationTarget for ListFactoryRoleTemplatesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.page.project_id())
  }
}
/// Safe seed metadata; full instructions require the dedicated exact detail query.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryRoleTemplateSummary {
  /// Exact release identity/version/digest.
  pub reference: ImmutableReference,
  /// Starter display name.
  pub name: FactoryText,
  /// Profile aliases requiring explicit installed selections.
  pub profiles: Vec<FactoryKey>,
}
/// Dedicated authorized content read for an exact release template.
#[derive(Clone, Debug)]
pub struct GetFactoryRoleTemplateQuery {
  /// Visible owning Project.
  pub project_id: ProjectId,
  /// Exact selected release identity, version and digest.
  pub reference: ImmutableReference,
}
impl Query for GetFactoryRoleTemplateQuery {
  type Outcome = FactoryRoleTemplate;
}
impl ManagementAuthorizationTarget for GetFactoryRoleTemplateQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait::async_trait]
impl<S: FactoryRoleProfileStore> ManagementQueryUseCase<ListFactoryRolesQuery> for FactoryRoleProfileHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListFactoryRolesQuery,
  ) -> Result<<ListFactoryRolesQuery as Query>::Outcome, ApplicationError> {
    super::factory_role_profiles::require_project_visibility(grant, query.page.project_id())?;
    Ok(
      self
        .store
        .list_factory_roles(query.page.project_id(), query.page.after().copied(), query.page.limit())
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: FactoryRoleProfileStore> ManagementQueryUseCase<ListFactoryRoleProfilesQuery>
  for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListFactoryRoleProfilesQuery,
  ) -> Result<<ListFactoryRoleProfilesQuery as Query>::Outcome, ApplicationError> {
    super::factory_role_profiles::require_project_visibility(grant, query.page.project_id())?;
    Ok(
      self
        .store
        .list_factory_role_profiles(
          query.page.project_id(),
          query.role_id,
          query.page.after().copied(),
          query.page.limit(),
        )
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: FactoryConfigurationStore> ManagementQueryUseCase<ListFactoryRoleTemplatesQuery>
  for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListFactoryRoleTemplatesQuery,
  ) -> Result<<ListFactoryRoleTemplatesQuery as Query>::Outcome, ApplicationError> {
    super::factory_role_profiles::require_project_visibility(grant, query.page.project_id())?;
    self
      .store
      .factory_configuration_availability(query.page.project_id())
      .await?;
    let mut items = factory_role_templates()
      .map_err(|_| ApplicationError::invalid())?
      .into_iter()
      .map(|template| {
        Ok(FactoryRoleTemplateSummary {
          reference: template.reference().map_err(|_| ApplicationError::invalid())?,
          name: template.role.name,
          profiles: template.profiles.into_keys().collect(),
        })
      })
      .collect::<Result<Vec<_>, ApplicationError>>()?;
    items.sort_by(|a, b| a.reference.identity().cmp(b.reference.identity()));
    let total = items.len() as u64;
    items.retain(|item| query.page.after().is_none_or(|after| item.reference.identity() > after));
    let more = items.len() > usize::from(query.page.limit());
    items.truncate(usize::from(query.page.limit()));
    let next_cursor = more.then(|| items.last().unwrap().reference.identity().clone());
    Ok(FactoryRoleCatalogPage {
      total,
      items,
      next_cursor,
    })
  }
}
#[async_trait::async_trait]
impl<S: FactoryConfigurationStore> ManagementQueryUseCase<GetFactoryRoleTemplateQuery>
  for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetFactoryRoleTemplateQuery,
  ) -> Result<FactoryRoleTemplate, ApplicationError> {
    super::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    self.store.factory_configuration_availability(query.project_id).await?;
    factory_role_templates()
      .map_err(|_| ApplicationError::invalid())?
      .into_iter()
      .find(|template| template.reference().is_ok_and(|reference| reference == query.reference))
      .ok_or_else(ApplicationError::capability_unavailable)
  }
}
