pub use octacity_server_store::{
  InstalledIntegration, IntegrationConnectionPage, MAX_INTEGRATION_AUTHENTICATION_METHODS,
  MAX_INTEGRATION_DISCOVERY_PAGE_SIZE, MAX_INTEGRATION_ENDPOINT_BYTES, MAX_INTEGRATION_ENDPOINTS,
};
use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use octacity_server_domain::Timestamp;
pub use octacity_server_domain::{IntegrationId, IntegrationVersion};
use octacity_server_factory::FactoryDigest;
use octacity_server_factory::ImmutableReference;
use octacity_server_store::{
  IdempotencyKey, IntegrationConnectionIntent, IntegrationConnectionMutationOutcome, IntegrationConnectionStore,
  PublishIntegrationConnection, PublishedIntegrationConnection, ReplayIntegrationConnection,
};
use serde::Serialize;

use crate::{
  ApplicationError, Command, ManagementAction, ManagementAuthorizationGrant, ManagementAuthorizationMapping,
  ManagementAuthorizationTarget, ManagementCommandUseCase, ManagementQueryUseCase, ManagementRequestContext,
  ManagementResourceKind, ManagementResourceResult, Query,
  management_security::{audited_mutation, instance_resource},
};

/// Maximum UTF-8 bytes in a reusable connection's display name.
pub const MAX_INTEGRATION_NAME_BYTES: usize = octacity_server_domain::MAX_RESOURCE_NAME_BYTES;

/// Publishes safe metadata from an explicitly selected installed declaration.
#[derive(Clone, Debug)]
pub struct PublishIntegrationConnectionCommand {
  /// Credential-free original caller intent and current-version precondition.
  pub intent: IntegrationConnectionIntent,
  /// Replay key partitioned by the accepted management security scope.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time; replay retains the original time.
  pub published_at: Timestamp,
}
impl Command for PublishIntegrationConnectionCommand {
  type Outcome = IntegrationConnectionMutationOutcome;
}
impl ManagementAuthorizationTarget for PublishIntegrationConnectionCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::instance(
    ManagementAction::Administer,
    ManagementResourceKind::IntegrationConnection,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::IntegrationConnection, self.intent.id)
  }
}

/// Reads safe metadata for one exact connection, optionally at a retained version.
#[derive(Clone, Debug)]
pub struct GetIntegrationConnectionQuery {
  /// Stable reusable connection identity.
  pub id: IntegrationId,
  /// Retained immutable version; absence selects current metadata.
  pub version: Option<IntegrationVersion>,
}
impl Query for GetIntegrationConnectionQuery {
  type Outcome = PublishedIntegrationConnection;
}
impl ManagementAuthorizationTarget for GetIntegrationConnectionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::IntegrationConnection);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::IntegrationConnection, self.id)
  }
}

/// Metadata-only use cases. Declarations come from trusted verified installation
/// composition, never a transport request. Neither publication nor lookup grants
/// Project use, changes Factory input, accesses credentials or invokes a provider.
pub struct IntegrationConnectionHandlers<S> {
  store: Arc<S>,
  installations: BTreeMap<ImmutableReference, InstalledIntegration>,
}
impl<S> IntegrationConnectionHandlers<S> {
  /// Accepts exact verified installation pins, rejecting ambiguous declarations.
  pub fn new(store: Arc<S>, declarations: Vec<InstalledIntegration>) -> Result<Self, ApplicationError> {
    let mut installations = BTreeMap::new();
    for declaration in declarations {
      declaration.validate()?;
      if installations
        .insert(declaration.installation.clone(), declaration)
        .is_some()
      {
        return Err(ApplicationError::invalid());
      }
    }
    Ok(Self { store, installations })
  }
}
#[async_trait]
impl<S: IntegrationConnectionStore> ManagementCommandUseCase<PublishIntegrationConnectionCommand>
  for IntegrationConnectionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: PublishIntegrationConnectionCommand,
  ) -> Result<IntegrationConnectionMutationOutcome, ApplicationError> {
    if let Some(outcome) = self
      .store
      .replay_integration_connection(audited_mutation(
        context,
        ReplayIntegrationConnection {
          intent: command.intent.clone(),
          idempotency_key: command.idempotency_key.clone(),
        },
      )?)
      .await?
    {
      return Ok(outcome);
    }
    let installed = self
      .installations
      .get(&command.intent.draft.installation)
      .ok_or_else(ApplicationError::capability_unavailable)?
      .clone();
    Ok(
      self
        .store
        .publish_integration_connection(audited_mutation(
          context,
          PublishIntegrationConnection {
            intent: command.intent,
            installed,
            idempotency_key: command.idempotency_key,
            published_at: command.published_at,
          },
        )?)
        .await?,
    )
  }
}
#[async_trait]
impl<S: IntegrationConnectionStore> ManagementQueryUseCase<GetIntegrationConnectionQuery>
  for IntegrationConnectionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetIntegrationConnectionQuery,
  ) -> Result<PublishedIntegrationConnection, ApplicationError> {
    Ok(self.store.integration_connection(query.id, query.version).await?)
  }
}

/// Lists current visible connections without provider or credential I/O.
#[derive(Clone, Debug)]
pub struct ListIntegrationConnectionsQuery {
  /// Exclusive visible connection position.
  pub after: Option<IntegrationId>,
  /// Positive page ceiling, at most 100.
  pub limit: u16,
}
impl Query for ListIntegrationConnectionsQuery {
  type Outcome = octacity_server_store::IntegrationConnectionPage;
}
impl ManagementAuthorizationTarget for ListIntegrationConnectionsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::collection(ManagementAction::View, ManagementResourceKind::IntegrationConnection);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::ManagementResource::collection(ManagementResourceKind::IntegrationConnection)
  }
}
#[async_trait]
impl<S: IntegrationConnectionStore> ManagementQueryUseCase<ListIntegrationConnectionsQuery>
  for IntegrationConnectionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIntegrationConnectionsQuery,
  ) -> Result<octacity_server_store::IntegrationConnectionPage, ApplicationError> {
    let visibility = grant
      .visibility_for::<ListIntegrationConnectionsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    Ok(
      self
        .store
        .list_integration_connections(octacity_server_store::ReadIntegrationConnections {
          visibility,
          after: query.after,
          limit: query.limit,
        })
        .await?,
    )
  }
}

/// Safe installed choices for integration administrators; not remote discovery or a grant.
#[derive(Clone, Debug, Serialize)]
pub struct IntegrationInstallationPage {
  /// Verified declarations with logical references only, never credential values or paths.
  pub items: Vec<InstalledIntegration>,
  /// Stable installation digest position, present only when more choices exist.
  pub next_cursor: Option<FactoryDigest>,
}
/// Bounded catalog of operator-installed provider declarations.
#[derive(Clone, Debug)]
pub struct ListIntegrationInstallationsQuery {
  /// Exclusive installed declaration position.
  pub after: Option<FactoryDigest>,
  /// Positive page ceiling, at most 100.
  pub limit: u16,
}
impl Query for ListIntegrationInstallationsQuery {
  type Outcome = IntegrationInstallationPage;
}
impl crate::ManagementVisibilityTarget for ListIntegrationInstallationsQuery {
  type Visibility = octacity_server_store::IntegrationConnectionListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::IntegrationConnection];
  fn visibility_from(
    grant: &ManagementAuthorizationGrant,
  ) -> Result<Self::Visibility, crate::ManagementVisibilityError> {
    <ListIntegrationConnectionsQuery as crate::ManagementVisibilityTarget>::visibility_from(grant)
  }
}
impl ManagementAuthorizationTarget for ListIntegrationInstallationsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::collection(
    ManagementAction::Administer,
    ManagementResourceKind::IntegrationConnection,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    crate::ManagementResource::collection(ManagementResourceKind::IntegrationConnection)
  }
}
#[async_trait]
impl<S: Send + Sync> ManagementQueryUseCase<ListIntegrationInstallationsQuery> for IntegrationConnectionHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIntegrationInstallationsQuery,
  ) -> Result<IntegrationInstallationPage, ApplicationError> {
    // Installed declarations are global administration metadata, not Project-use choices.
    // A restricted connection grant must never enumerate the global installation catalog.
    if !matches!(grant.visibility().view(), crate::ManagementVisibilityView::All)
      || !(1..=MAX_INTEGRATION_DISCOVERY_PAGE_SIZE).contains(&query.limit)
    {
      return Err(ApplicationError::invalid());
    }
    let mut choices = BTreeMap::new();
    for installed in self.installations.values() {
      let bytes = serde_json::to_vec(&installed.installation).map_err(|_| ApplicationError::invalid())?;
      let position = FactoryDigest::sha256("octacity.integration-installation-position.v1", &[&bytes]);
      if query.after.is_none_or(|after| position > after) {
        choices.insert(position, installed.clone());
      }
    }
    let mut items: Vec<_> = choices.into_iter().take(usize::from(query.limit) + 1).collect();
    let next_cursor = (items.len() > usize::from(query.limit)).then(|| items[usize::from(query.limit) - 1].0);
    items.truncate(usize::from(query.limit));
    Ok(IntegrationInstallationPage {
      items: items.into_iter().map(|(_, choice)| choice).collect(),
      next_cursor,
    })
  }
}

/// Resolves a draft against current verified installation choices without publication.
#[derive(Clone, Debug)]
pub struct ValidateIntegrationConnectionQuery {
  /// Exact metadata intent to review; no credential value is accepted.
  pub intent: IntegrationConnectionIntent,
}
impl Query for ValidateIntegrationConnectionQuery {
  type Outcome = octacity_server_store::IntegrationConnectionDefinition;
}
impl ManagementAuthorizationTarget for ValidateIntegrationConnectionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = PublishIntegrationConnectionCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::IntegrationConnection, self.intent.id)
  }
}
#[async_trait]
impl<S: Send + Sync> ManagementQueryUseCase<ValidateIntegrationConnectionQuery> for IntegrationConnectionHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ValidateIntegrationConnectionQuery,
  ) -> Result<octacity_server_store::IntegrationConnectionDefinition, ApplicationError> {
    let installed = self
      .installations
      .get(&query.intent.draft.installation)
      .ok_or_else(ApplicationError::capability_unavailable)?;
    Ok(octacity_server_store::IntegrationConnectionDefinition::resolve(
      &query.intent.draft,
      installed,
    )?)
  }
}
