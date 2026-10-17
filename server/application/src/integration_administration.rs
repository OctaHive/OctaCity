//! Authorized, recoverable orchestration of safe database state and private material.
use crate::management_security::{audited_mutation, instance_resource};
use crate::*;
use octacity_server_domain::IntegrationId;
pub use octacity_server_secrets::{
  CredentialPublicationKind, LogicalSecretReference, MAX_PROVIDER_CREDENTIAL_BYTES, ProviderCredentialValue,
};
use octacity_server_secrets::{ProviderCredentialError, ProviderCredentialPublisher};
pub use octacity_server_store::{
  BeginIntegrationAdministration, IntegrationAdministrationAction, IntegrationAdministrationIntent,
  IntegrationAdministrationReceipt, IntegrationAdministrationState, IntegrationAdministrationStatus,
  IntegrationConnectionIntent, IntegrationConnectionMutationOutcome, PublishedIntegrationConnection,
};
use octacity_server_store::{IntegrationAdministrationStore, StoreError};
use std::sync::Arc;

/// Write-only input. It has no serialization/clone path into ordinary persistence.
#[derive(Debug)]
pub struct ManageIntegrationCredentialCommand {
  /// Original credential-free operation and replay identity.
  pub request: BeginIntegrationAdministration,
  /// Transient secret; recovery and adoption may omit it.
  pub value: Option<ProviderCredentialValue>,
}
impl Command for ManageIntegrationCredentialCommand {
  type Outcome = IntegrationAdministrationReceipt;
}
impl ManagementAuthorizationTarget for ManageIntegrationCredentialCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::instance(
    ManagementAction::Administer,
    ManagementResourceKind::IntegrationCredential,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::IntegrationCredential, self.request.intent.id)
  }
}
/// Explicit lifecycle change; it cannot submit credentials or grant Project use.
#[derive(Clone, Debug)]
pub struct SetIntegrationConnectionStateCommand {
  /// Exact metadata/administrative preconditions and safe replay key.
  pub request: BeginIntegrationAdministration,
}
impl Command for SetIntegrationConnectionStateCommand {
  type Outcome = IntegrationAdministrationReceipt;
}
impl ManagementAuthorizationTarget for SetIntegrationConnectionStateCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::instance(
    ManagementAction::Administer,
    ManagementResourceKind::IntegrationConnection,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::IntegrationConnection, self.request.intent.id)
  }
}
/// Safe lifecycle inspection; connection authority does not expose secret material.
#[derive(Clone, Debug)]
pub struct GetIntegrationAdministrationQuery {
  /// Stable connection identity.
  pub id: IntegrationId,
}
impl Query for GetIntegrationAdministrationQuery {
  type Outcome = IntegrationAdministrationState;
}
impl ManagementAuthorizationTarget for GetIntegrationAdministrationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::IntegrationConnection);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::IntegrationConnection, self.id)
  }
}
/// One bounded private-preparation cleanup pass under current credential administration.
#[derive(Clone, Debug)]
pub struct CleanupIntegrationPreparationsCommand {
  /// Authorized connection, with all referenced immutable versions/material retained.
  pub id: IntegrationId,
  /// Trusted request time for attributed expiration evidence.
  pub requested_at: Timestamp,
  /// Trusted expiry cutoff; the HTTP adapter uses a fixed retention window.
  pub older_than: Timestamp,
  /// At most 100 preparations.
  pub limit: u16,
}
impl Command for CleanupIntegrationPreparationsCommand {
  type Outcome = usize;
}
impl ManagementAuthorizationTarget for CleanupIntegrationPreparationsCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::instance(
    ManagementAction::Administer,
    ManagementResourceKind::IntegrationCredential,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::IntegrationCredential, self.id)
  }
}
/// Administrative orchestration; no provider call, Project grant or Factory input activation.
pub struct IntegrationAdministrationHandlers<S, P: ?Sized> {
  store: Arc<S>,
  publisher: Arc<P>,
}
impl<S, P: ?Sized> IntegrationAdministrationHandlers<S, P> {
  /// Composes the durable safe journal with a purpose-confined private publisher.
  pub fn new(store: Arc<S>, publisher: Arc<P>) -> Self {
    Self { store, publisher }
  }
}
fn private_failure(error: ProviderCredentialError) -> ApplicationError {
  match error {
    ProviderCredentialError::InvalidValue => ApplicationError::invalid(),
    ProviderCredentialError::NotConfigured => ApplicationError::capability_unavailable(),
    ProviderCredentialError::Conflict => ApplicationError::Store(StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::Integration,
    }),
    ProviderCredentialError::Denied => ApplicationError::Store(StoreError::CredentialRejected),
    ProviderCredentialError::Unavailable => ApplicationError::unavailable(),
  }
}
#[async_trait::async_trait]
impl<S: IntegrationAdministrationStore, P: ProviderCredentialPublisher + ?Sized>
  ManagementCommandUseCase<ManageIntegrationCredentialCommand> for IntegrationAdministrationHandlers<S, P>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: ManageIntegrationCredentialCommand,
  ) -> Result<IntegrationAdministrationReceipt, ApplicationError> {
    // A network-position grant is insufficient for write-only secret administration.
    if context.actor().kind() != ManagementActorKind::AuthenticatedManagement
      || context.attributes().ingress() != ManagementIngress::VerifiedIdentity
    {
      return Err(ApplicationError::invalid());
    }
    let IntegrationAdministrationAction::Credential { kind, .. } = command.request.intent.action else {
      return Err(ApplicationError::invalid());
    };
    if matches!(
      kind,
      CredentialPublicationKind::Adopt | CredentialPublicationKind::Revoke
    ) && command.value.is_some()
    {
      return Err(ApplicationError::invalid());
    }
    let original_intent = command.request.intent.clone();
    let receipt = self
      .store
      .begin_integration_administration(audited_mutation(context, command.request)?)
      .await?;
    let publication = receipt.publication.as_ref().ok_or_else(ApplicationError::unavailable)?;
    let IntegrationAdministrationAction::Credential { reference, .. } = &original_intent.action else {
      return Err(ApplicationError::invalid());
    };
    if receipt.intent != original_intent
      || publication.reference != *reference
      || publication.operation != receipt.operation
      || publication.generation != receipt.revision
      || original_intent.expected_revision.checked_add(1) != Some(receipt.revision)
    {
      return Err(ApplicationError::unavailable());
    }
    if receipt.status != IntegrationAdministrationStatus::Prepared {
      return Ok(receipt);
    }
    self
      .publisher
      .prepare(publication, kind, command.value)
      .await
      .map_err(private_failure)?;
    self.publisher.activate(publication).await.map_err(private_failure)?;
    let receipt = self
      .store
      .complete_integration_administration(audited_mutation(context, receipt.clone())?)
      .await?;
    // Completion is authoritative. Failed private cleanup remains eligible for a bounded pass.
    if self.publisher.discard(publication).await.is_ok() {
      let _ = self
        .store
        .acknowledge_integration_preparation_cleanup(audited_mutation(context, publication.clone())?)
        .await;
    }
    Ok(receipt)
  }
}

#[async_trait::async_trait]
impl<S: IntegrationAdministrationStore, P: ProviderCredentialPublisher + ?Sized>
  ManagementCommandUseCase<SetIntegrationConnectionStateCommand> for IntegrationAdministrationHandlers<S, P>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: SetIntegrationConnectionStateCommand,
  ) -> Result<IntegrationAdministrationReceipt, ApplicationError> {
    if matches!(
      command.request.intent.action,
      IntegrationAdministrationAction::Credential { .. }
    ) {
      return Err(ApplicationError::invalid());
    }
    Ok(
      self
        .store
        .begin_integration_administration(audited_mutation(context, command.request)?)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IntegrationAdministrationStore, P: ProviderCredentialPublisher + ?Sized>
  ManagementQueryUseCase<GetIntegrationAdministrationQuery> for IntegrationAdministrationHandlers<S, P>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetIntegrationAdministrationQuery,
  ) -> Result<IntegrationAdministrationState, ApplicationError> {
    Ok(self.store.integration_administration(query.id).await?)
  }
}
#[async_trait::async_trait]
impl<S: IntegrationAdministrationStore, P: ProviderCredentialPublisher + ?Sized>
  ManagementCommandUseCase<CleanupIntegrationPreparationsCommand> for IntegrationAdministrationHandlers<S, P>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: CleanupIntegrationPreparationsCommand,
  ) -> Result<usize, ApplicationError> {
    if context.actor().kind() != ManagementActorKind::AuthenticatedManagement {
      return Err(ApplicationError::invalid());
    }
    let batch = self
      .store
      .integration_preparation_cleanup(audited_mutation(
        context,
        octacity_server_store::IntegrationPreparationCleanup {
          id: command.id,
          requested_at: command.requested_at,
          older_than: command.older_than,
          limit: command.limit,
        },
      )?)
      .await?;
    for publication in &batch {
      self.publisher.discard(publication).await.map_err(private_failure)?;
      self
        .store
        .acknowledge_integration_preparation_cleanup(audited_mutation(context, publication.clone())?)
        .await?;
    }
    Ok(batch.len())
  }
}
