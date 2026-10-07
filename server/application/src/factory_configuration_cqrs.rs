use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{
  FactoryConfiguration, FactoryConfigurationDraft, FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest,
};
use octacity_server_store::{
  CreateFactoryConfiguration, FactoryConfigurationAvailability, FactoryConfigurationMutationIntent,
  FactoryConfigurationStore, IdempotencyKey, ReplaceFactoryConfiguration, ReplayFactoryConfigurationMutation,
};

use crate::{
  ApplicationError, Command, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementResourceKind, ManagementResourceResult, MutationDisposition, Query,
  management_security::{audited_mutation, instance_resource, owned_collection_resource},
};

/// Creates one immutable Factory Configuration identity and version one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateFactoryConfigurationCommand {
  /// Stable Factory Configuration identity.
  pub id: FactoryConfigurationId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Digest of the canonical submitted definition.
  pub definition_digest: FactoryDigest,
  /// Complete unresolved initial definition.
  pub draft: FactoryConfigurationDraft,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for CreateFactoryConfigurationCommand {
  type Outcome = FactoryConfigurationCommandOutcome;
}

impl ManagementAuthorizationTarget for CreateFactoryConfigurationCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Create,
    ManagementResourceKind::FactoryConfiguration,
    ManagementResourceKind::Project,
  );

  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FactoryConfiguration,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Publishes the immediately following immutable Factory Configuration version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceFactoryConfigurationCommand {
  /// Existing Factory Configuration identity.
  pub id: FactoryConfigurationId,
  /// Version that must still be current.
  pub expected_current_version: FactoryConfigurationVersion,
  /// Digest of the canonical submitted definition.
  pub definition_digest: FactoryDigest,
  /// Complete unresolved replacement definition.
  pub draft: FactoryConfigurationDraft,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for ReplaceFactoryConfigurationCommand {
  type Outcome = FactoryConfigurationCommandOutcome;
}

impl ManagementAuthorizationTarget for ReplaceFactoryConfigurationCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Publish, ManagementResourceKind::FactoryConfiguration);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryConfiguration, self.id)
  }
}

/// Validates and resolves one candidate without allocating or publishing a version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidateFactoryConfigurationQuery {
  /// Prospective stable Factory Configuration identity.
  pub id: FactoryConfigurationId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Digest of the canonical candidate definition.
  pub definition_digest: FactoryDigest,
  /// Complete unresolved candidate.
  pub draft: FactoryConfigurationDraft,
}

impl Query for ValidateFactoryConfigurationQuery {
  type Outcome = FactoryConfigurationValidation;
}

impl ManagementAuthorizationTarget for ValidateFactoryConfigurationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Create,
    ManagementResourceKind::FactoryConfiguration,
    ManagementResourceKind::Project,
  );

  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FactoryConfiguration,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Reads one exact immutable Factory Configuration version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetFactoryConfigurationQuery {
  /// Stable Factory Configuration identity.
  pub id: FactoryConfigurationId,
  /// Exact immutable version.
  pub version: FactoryConfigurationVersion,
}

impl Query for GetFactoryConfigurationQuery {
  type Outcome = FactoryConfigurationProjection;
}

impl ManagementAuthorizationTarget for GetFactoryConfigurationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::FactoryConfiguration);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryConfiguration, self.id)
  }
}

/// Discovers the current immutable version of one Factory Configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetCurrentFactoryConfigurationQuery {
  /// Stable Factory Configuration identity.
  pub id: FactoryConfigurationId,
}

impl Query for GetCurrentFactoryConfigurationQuery {
  type Outcome = FactoryConfigurationProjection;
}

impl ManagementAuthorizationTarget for GetCurrentFactoryConfigurationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::FactoryConfiguration);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryConfiguration, self.id)
  }
}

/// Safe application projection of one immutable Factory Configuration version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryConfigurationProjection {
  /// Fully resolved immutable configuration.
  pub configuration: FactoryConfiguration,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Successful non-mutating validation and exact alias-resolution preview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryConfigurationValidation {
  /// Prospective resolved configuration; it has not been persisted.
  pub configuration: FactoryConfiguration,
}

/// Result of creating or replacing one Factory Configuration version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryConfigurationCommandOutcome {
  /// Whether the mutation was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Immutable configuration committed by the original command.
  pub configuration: FactoryConfigurationProjection,
}

/// Typed Factory Configuration command and query handlers backed by one narrow port.
pub struct FactoryConfigurationHandlers<S> {
  store: Arc<S>,
}

impl<S> FactoryConfigurationHandlers<S> {
  /// Creates handlers from a backend-neutral Factory Configuration port.
  #[must_use]
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

impl<S> FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore,
{
  async fn resolve(
    &self,
    id: FactoryConfigurationId,
    version: FactoryConfigurationVersion,
    project_id: ProjectId,
    definition_digest: FactoryDigest,
    draft: FactoryConfigurationDraft,
  ) -> Result<FactoryConfiguration, ApplicationError> {
    let availability = self.store.factory_configuration_availability(project_id).await?;
    let choices = match availability {
      FactoryConfigurationAvailability::Disabled => return Err(ApplicationError::capability_unavailable()),
      FactoryConfigurationAvailability::Available(choices) => choices,
    };
    FactoryConfiguration::publish(id, version, project_id, definition_digest, draft, &choices)
      .map_err(|_| ApplicationError::invalid())
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<CreateFactoryConfigurationCommand> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: CreateFactoryConfigurationCommand,
  ) -> Result<FactoryConfigurationCommandOutcome, Self::Error> {
    let intent = FactoryConfigurationMutationIntent::Create {
      id: command.id,
      project_id: command.project_id,
      definition_digest: command.definition_digest,
      draft: command.draft.clone(),
    };
    if let Some(outcome) = self
      .store
      .replay_factory_configuration_mutation(audited_mutation(
        context,
        ReplayFactoryConfigurationMutation {
          idempotency_key: command.idempotency_key.clone(),
          intent: intent.clone(),
        },
      )?)
      .await?
    {
      return Ok(outcome.into());
    }
    let configuration = self
      .resolve(
        command.id,
        FactoryConfigurationVersion::INITIAL,
        command.project_id,
        command.definition_digest,
        command.draft,
      )
      .await?;
    let outcome = self
      .store
      .create_factory_configuration(audited_mutation(
        context,
        CreateFactoryConfiguration {
          configuration,
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
          intent,
        },
      )?)
      .await?;
    Ok(outcome.into())
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<ReplaceFactoryConfigurationCommand> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: ReplaceFactoryConfigurationCommand,
  ) -> Result<FactoryConfigurationCommandOutcome, Self::Error> {
    let intent = FactoryConfigurationMutationIntent::Replace {
      id: command.id,
      expected_current_version: command.expected_current_version,
      definition_digest: command.definition_digest,
      draft: command.draft.clone(),
    };
    if let Some(outcome) = self
      .store
      .replay_factory_configuration_mutation(audited_mutation(
        context,
        ReplayFactoryConfigurationMutation {
          idempotency_key: command.idempotency_key.clone(),
          intent: intent.clone(),
        },
      )?)
      .await?
    {
      return Ok(outcome.into());
    }
    let current = self.store.current_factory_configuration(command.id).await?;
    let next_version = FactoryConfigurationVersion::new(
      command
        .expected_current_version
        .get()
        .checked_add(1)
        .ok_or_else(ApplicationError::unavailable)?,
    )
    .map_err(|_| ApplicationError::unavailable())?;
    let configuration = self
      .resolve(
        command.id,
        next_version,
        current.configuration.reference().project_id(),
        command.definition_digest,
        command.draft,
      )
      .await?;
    let outcome = self
      .store
      .replace_factory_configuration(audited_mutation(
        context,
        ReplaceFactoryConfiguration {
          expected_current_version: command.expected_current_version,
          configuration,
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
          intent,
        },
      )?)
      .await?;
    Ok(outcome.into())
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<ValidateFactoryConfigurationQuery> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: ValidateFactoryConfigurationQuery,
  ) -> Result<FactoryConfigurationValidation, Self::Error> {
    Ok(FactoryConfigurationValidation {
      configuration: self
        .resolve(
          query.id,
          FactoryConfigurationVersion::INITIAL,
          query.project_id,
          query.definition_digest,
          query.draft,
        )
        .await?,
    })
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<GetFactoryConfigurationQuery> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: GetFactoryConfigurationQuery,
  ) -> Result<FactoryConfigurationProjection, Self::Error> {
    self
      .store
      .factory_configuration_version(query.id, query.version)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<GetCurrentFactoryConfigurationQuery> for FactoryConfigurationHandlers<S>
where
  S: FactoryConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: GetCurrentFactoryConfigurationQuery,
  ) -> Result<FactoryConfigurationProjection, Self::Error> {
    self
      .store
      .current_factory_configuration(query.id)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }
}

impl From<octacity_server_store::PublishedFactoryConfiguration> for FactoryConfigurationProjection {
  fn from(value: octacity_server_store::PublishedFactoryConfiguration) -> Self {
    Self {
      configuration: value.configuration,
      published_at: value.published_at,
    }
  }
}

impl From<octacity_server_store::FactoryConfigurationMutationOutcome> for FactoryConfigurationCommandOutcome {
  fn from(value: octacity_server_store::FactoryConfigurationMutationOutcome) -> Self {
    Self {
      disposition: value.disposition.into(),
      configuration: value.configuration.into(),
    }
  }
}
