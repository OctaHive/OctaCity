//! Current Project grants are checked at protected use, independently of remote credentials.
use octacity_server_store::*;
pub use octacity_server_store::{
  IntegrationProjectAccessIntent, IntegrationProjectAccessOutcome, IntegrationPurpose, IntegrationResource,
  IntegrationResourceSelection, MAX_INTEGRATION_RESOURCES, PublishIntegrationProjectAccess,
  PublishedIntegrationProjectAccess,
};

/// Trusted installed material accepted by a current resource/purpose grant.
#[derive(Clone, Debug)]
pub struct AuthorizedIntegrationResource {
  /// Exact installed namespace, retained across credential rotation.
  pub installation: octacity_server_factory::ImmutableReference,
  /// Original installed executable pin.
  pub adapter: octacity_server_factory::ImmutableReference,
  /// Exact original native resource identity.
  pub resource: IntegrationResource,
  /// Only the explicitly requested installed purpose.
  pub capability: IntegrationCapability,
}
/// Revalidates current grants for one pinned operation; never contacts providers or resolves secrets.
/// Call immediately before every protected dispatch, including retained-intent retries.
pub async fn authorize_integration_resource<
  S: IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore,
>(
  store: &S,
  request: &IntegrationResourceAccess,
) -> Result<AuthorizedIntegrationResource, ApplicationError> {
  let grant = store
    .integration_project_access(request.connection_id, request.project_id)
    .await?
    .ok_or_else(ApplicationError::capability_unavailable)?;
  if grant.connection_version != request.connection_version
    || !grant
      .resources
      .iter()
      .any(|r| r.reference == request.resource && r.purposes.contains(&request.purpose))
  {
    return Err(ApplicationError::capability_unavailable());
  }
  let metadata = store
    .integration_connection(request.connection_id, Some(request.connection_version))
    .await?;
  let resource = metadata
    .definition
    .resources
    .iter()
    .find(|r| r.reference == request.resource)
    .ok_or_else(ApplicationError::capability_unavailable)?
    .clone();
  let capability = resource
    .capabilities
    .iter()
    .find(|c| c.purpose == request.purpose)
    .ok_or_else(ApplicationError::capability_unavailable)?
    .clone();
  let administration = store.integration_administration(request.connection_id).await?;
  if !administration.enabled
    || administration.active_version != Some(request.connection_version)
    || !administration
      .credentials
      .iter()
      .any(|c| c.reference == capability.credential && c.configured)
  {
    return Err(ApplicationError::capability_unavailable());
  }
  Ok(AuthorizedIntegrationResource {
    installation: metadata.definition.installation,
    adapter: metadata.definition.adapter,
    resource,
    capability,
  })
}

use crate::management_security::{audited_mutation, instance_resource};
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{IntegrationId, ProjectId};
use std::sync::Arc;

/// Explicit Project/resource/purpose publication, independently authorized from use.
#[derive(Clone, Debug)]
pub struct PublishIntegrationProjectAccessCommand {
  /// Exact reviewed grant/revocation and replay preconditions.
  pub request: PublishIntegrationProjectAccess,
}
impl Command for PublishIntegrationProjectAccessCommand {
  type Outcome = IntegrationProjectAccessOutcome;
}
impl ManagementAuthorizationTarget for PublishIntegrationProjectAccessCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::instance(
    ManagementAction::Administer,
    ManagementResourceKind::IntegrationConnection,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(
      ManagementResourceKind::IntegrationConnection,
      self.request.intent.connection_id,
    )
  }
}
/// Enumerates only the Project's explicitly granted resource/purpose choices.
#[derive(Clone, Debug)]
pub struct ListIntegrationProjectResourcesQuery {
  /// Visible connection to use, not necessarily administer.
  pub connection_id: IntegrationId,
  /// Project whose visibility and use are required.
  pub project_id: ProjectId,
}
impl Query for ListIntegrationProjectResourcesQuery {
  type Outcome = Vec<IntegrationResource>;
}
impl ManagementAuthorizationTarget for ListIntegrationProjectResourcesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
/// Grant publication and use discovery share current authorization, not credential administration.
pub struct IntegrationProjectAccessHandlers<S> {
  store: Arc<S>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S> IntegrationProjectAccessHandlers<S> {
  /// Uses the same policy as the outer authorized dispatcher for dependent resources.
  pub fn new(store: Arc<S>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self { store, policy }
  }
  async fn require(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    kind: ManagementResourceKind,
    id: impl ToString,
  ) -> Result<(), ApplicationError> {
    let resource = instance_resource(kind, id.to_string()).map_err(|_| ApplicationError::invalid())?;
    let grant = self
      .policy
      .authorize(context, action, &resource)
      .await
      .map_err(|_| ApplicationError::invalid())?;
    match grant.visibility().view() {
      ManagementVisibilityView::All => Ok(()),
      ManagementVisibilityView::Restricted(resources) if resources.contains(&resource) => Ok(()),
      _ => Err(ApplicationError::invalid()),
    }
  }
}
#[async_trait]
impl<S: IntegrationProjectAccessStore> ManagementCommandUseCase<PublishIntegrationProjectAccessCommand>
  for IntegrationProjectAccessHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: PublishIntegrationProjectAccessCommand,
  ) -> Result<IntegrationProjectAccessOutcome, ApplicationError> {
    self
      .require(
        context,
        ManagementAction::Administer,
        ManagementResourceKind::IntegrationConnection,
        command.request.intent.connection_id,
      )
      .await?;
    self
      .require(
        context,
        ManagementAction::Administer,
        ManagementResourceKind::Project,
        command.request.intent.project_id,
      )
      .await?;
    self
      .require(
        context,
        ManagementAction::View,
        ManagementResourceKind::Project,
        command.request.intent.project_id,
      )
      .await?;
    Ok(
      self
        .store
        .publish_integration_project_access(audited_mutation(context, command.request)?)
        .await?,
    )
  }
}
#[async_trait]
impl<S: IntegrationProjectAccessStore + IntegrationConnectionStore>
  ManagementQueryUseCase<ListIntegrationProjectResourcesQuery> for IntegrationProjectAccessHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ListIntegrationProjectResourcesQuery,
  ) -> Result<Vec<IntegrationResource>, ApplicationError> {
    self
      .require(
        context,
        ManagementAction::View,
        ManagementResourceKind::Project,
        query.project_id,
      )
      .await?;
    self
      .require(
        context,
        ManagementAction::Execute,
        ManagementResourceKind::IntegrationConnection,
        query.connection_id,
      )
      .await?;
    self
      .require(
        context,
        ManagementAction::View,
        ManagementResourceKind::IntegrationConnection,
        query.connection_id,
      )
      .await?;
    let Some(access) = self
      .store
      .integration_project_access(query.connection_id, query.project_id)
      .await?
    else {
      return Ok(vec![]);
    };
    let metadata = self
      .store
      .integration_connection(query.connection_id, Some(access.connection_version))
      .await?;
    Ok(
      metadata
        .definition
        .resources
        .into_iter()
        .filter_map(|mut resource| {
          let selected = access.resources.iter().find(|r| r.reference == resource.reference)?;
          resource.capabilities.retain(|c| selected.purposes.contains(&c.purpose));
          if resource.capabilities.is_empty() {
            None
          } else {
            Some(resource)
          }
        })
        .collect(),
    )
  }
}

#[async_trait]
trait IntegrationResourceAuthorizer: Send + Sync {
  async fn authorize(
    &self,
    request: &IntegrationResourceAccess,
  ) -> Result<AuthorizedIntegrationResource, ApplicationError>;
}
struct StoredIntegrationAuthorizer<S>(Arc<S>);
#[async_trait]
impl<S: IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore>
  IntegrationResourceAuthorizer for StoredIntegrationAuthorizer<S>
{
  async fn authorize(
    &self,
    request: &IntegrationResourceAccess,
  ) -> Result<AuthorizedIntegrationResource, ApplicationError> {
    authorize_integration_resource(self.0.as_ref(), request).await
  }
}
/// Explicit managed-use attachment to a trusted installed runtime profile.
/// It is separate from immutable Work identity; a denied managed check never falls back to legacy credentials.
#[derive(Clone)]
pub struct ManagedIntegrationAccess {
  connection_id: IntegrationId,
  version: octacity_server_domain::IntegrationVersion,
  authorizer: Arc<dyn IntegrationResourceAuthorizer>,
  work_configuration: Option<(
    octacity_server_factory::ImmutableReference,
    octacity_server_factory::ImmutableReference,
  )>,
}
/// Shared current-authority resolver for explicitly managed inputs, without eager reads or provider IO.
#[derive(Clone)]
pub struct ManagedIntegrationConnections {
  authorizer: Arc<dyn IntegrationResourceAuthorizer>,
}
impl ManagedIntegrationConnections {
  /// Installs the existing connection/access read ports once for Source and Reporter composition.
  pub fn new<
    S: IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore + 'static,
  >(
    store: Arc<S>,
  ) -> Self {
    Self {
      authorizer: Arc::new(StoredIntegrationAuthorizer(store)),
    }
  }
  /// Reuses current-authority resolution for the original private Issue writer pin.
  pub fn for_issue_writer(
    &self,
    pin: &octacity_server_store::IssueSourceWriterAccess,
    binding: &octacity_server_factory::WorkSourceBinding,
  ) -> ManagedIntegrationAccess {
    ManagedIntegrationAccess {
      connection_id: pin.connection_id,
      version: pin.connection_version,
      authorizer: self.authorizer.clone(),
      work_configuration: pin
        .configuration
        .as_ref()
        .map(|c| (binding.configuration.clone(), c.clone())),
    }
  }
  /// Binds one published input profile to its exact connection profile; this grants no authority.
  pub fn for_work_binding(
    &self,
    connection: &octacity_server_factory::WorkSourceConnectionRef,
    binding: &octacity_server_factory::WorkSourceBinding,
    purpose: IntegrationPurpose,
  ) -> Result<ManagedIntegrationAccess, ApplicationError> {
    let configuration = match purpose {
      IntegrationPurpose::TaskRead => &connection.source_configuration,
      IntegrationPurpose::TaskUpdate => connection
        .reporter_configuration
        .as_ref()
        .ok_or_else(ApplicationError::capability_unavailable)?,
      _ => return Err(ApplicationError::capability_unavailable()),
    };
    Ok(ManagedIntegrationAccess {
      connection_id: connection.id,
      version: connection.version,
      authorizer: self.authorizer.clone(),
      work_configuration: Some((binding.configuration.clone(), configuration.clone())),
    })
  }
}
impl ManagedIntegrationAccess {
  /// Original connection identity for independent management visibility/use checks.
  pub fn connection_id(&self) -> IntegrationId {
    self.connection_id
  }
  /// Freezes only the original managed writer identity, never a credential or continuing grant.
  pub fn issue_writer_access(&self) -> octacity_server_store::IssueSourceWriterAccess {
    octacity_server_store::IssueSourceWriterAccess {
      connection_id: self.connection_id,
      connection_version: self.version,
      configuration: self
        .work_configuration
        .as_ref()
        .map(|(_, configuration)| configuration.clone()),
    }
  }
  /// Pins the reviewed connection version without reading credentials, granting access or contacting providers.
  pub fn new<
    S: IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore + 'static,
  >(
    store: Arc<S>,
    connection_id: IntegrationId,
    version: octacity_server_domain::IntegrationVersion,
  ) -> Self {
    Self {
      connection_id,
      version,
      authorizer: Arc::new(StoredIntegrationAuthorizer(store)),
      work_configuration: None,
    }
  }
  /// Rechecks current Project use and exact native/profile pins immediately before a provider dispatch.
  pub async fn authorize_work_binding(
    &self,
    project_id: ProjectId,
    binding: &octacity_server_factory::WorkSourceBinding,
    plugin: &octacity_server_factory::ImmutableReference,
    native_resource: &str,
    purpose: IntegrationPurpose,
  ) -> Result<(), ApplicationError> {
    let material = self
      .authorizer
      .authorize(&IntegrationResourceAccess {
        connection_id: self.connection_id,
        connection_version: self.version,
        project_id,
        resource: binding.resource.clone(),
        purpose,
      })
      .await?;
    let configuration = if let Some((input, connection)) = &self.work_configuration {
      if input != &binding.configuration {
        return Err(ApplicationError::capability_unavailable());
      }
      connection
    } else {
      &binding.configuration
    };
    if material.installation != binding.installation
      || material.adapter != *plugin
      || material.resource.native_id.as_str() != native_resource
      || material.capability.capability != binding.capability
      || material.capability.configuration != *configuration
    {
      return Err(ApplicationError::capability_unavailable());
    }
    Ok(())
  }
}

/// Reads current Project grant metadata without granting runtime use or requiring administration.
#[derive(Clone, Debug)]
pub struct GetIntegrationProjectAccessQuery {
  /// Visible connection identity.
  pub connection_id: IntegrationId,
  /// Project whose current grant is being inspected.
  pub project_id: ProjectId,
}
impl Query for GetIntegrationProjectAccessQuery {
  type Outcome = Option<PublishedIntegrationProjectAccess>;
}
impl ManagementAuthorizationTarget for GetIntegrationProjectAccessQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait]
impl<S: IntegrationProjectAccessStore> ManagementQueryUseCase<GetIntegrationProjectAccessQuery>
  for IntegrationProjectAccessHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetIntegrationProjectAccessQuery,
  ) -> Result<Option<PublishedIntegrationProjectAccess>, ApplicationError> {
    self
      .require(
        context,
        ManagementAction::View,
        ManagementResourceKind::Project,
        query.project_id,
      )
      .await?;
    self
      .require(
        context,
        ManagementAction::View,
        ManagementResourceKind::IntegrationConnection,
        query.connection_id,
      )
      .await?;
    Ok(
      self
        .store
        .integration_project_access(query.connection_id, query.project_id)
        .await?,
    )
  }
}
