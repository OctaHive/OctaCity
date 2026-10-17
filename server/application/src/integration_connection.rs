use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use octacity_server_domain::{IntegrationId, IntegrationVersion, Timestamp};
use octacity_server_factory::ImmutableReference;
use octacity_server_store::{
  IdempotencyKey, InstalledIntegration, IntegrationConnectionIntent, IntegrationConnectionMutationOutcome,
  IntegrationConnectionStore, PublishIntegrationConnection, PublishedIntegrationConnection,
  ReplayIntegrationConnection,
};

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
