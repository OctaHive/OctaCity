use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, ProjectId, RepositoryId, RepositoryName,
  RepositoryVersion, Timestamp,
};
use octacity_server_store::{
  BuildConfigurationDefinition, ConfigurationDiscoveryStore, ConfigurationStore, CreateBuildConfiguration,
  CreateRepository, IdempotencyKey, ListProjectBuildConfigurations, ListProjectRepositories,
  PublishBuildConfigurationVersion, PublishRepositoryVersion, RepositoryDefinition,
};
use serde::{Deserialize, Serialize};

use crate::{
  ApplicationError, BuildConfigurationPageProjection, BuildConfigurationProjection, Command, CommandTransaction,
  ListProjectBuildConfigurationsQuery, ListProjectRepositoriesQuery, ManagementAction, ManagementAuthorizationMapping,
  ManagementAuthorizationTarget, ManagementResourceKind, ManagementResourceResult, MutationDisposition, Query,
  RepositoryPageProjection, RepositoryProjection,
  management_security::{audited_mutation, instance_resource, owned_collection_resource},
  projections::validate_configuration_projection,
};

/// Creates one Repository identity and immutable initial version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateRepositoryCommand {
  /// Stable Repository identity.
  pub id: RepositoryId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Project-local Repository name.
  pub name: RepositoryName,
  /// Provider-neutral immutable Repository definition.
  pub definition: RepositoryDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for CreateRepositoryCommand {
  type Outcome = RepositoryCommandOutcome;
}

impl ManagementAuthorizationTarget for CreateRepositoryCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Create,
    ManagementResourceKind::Repository,
    ManagementResourceKind::Project,
  );

  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::Repository,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Appends the next immutable version of one Repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishRepositoryVersionCommand {
  /// Existing Repository identity.
  pub id: RepositoryId,
  /// Version that must still be current.
  pub expected_current_version: RepositoryVersion,
  /// Complete next provider-neutral definition.
  pub definition: RepositoryDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for PublishRepositoryVersionCommand {
  type Outcome = RepositoryCommandOutcome;
}

impl ManagementAuthorizationTarget for PublishRepositoryVersionCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Publish, ManagementResourceKind::Repository);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Repository, self.id)
  }
}

/// Result of creating or publishing one Repository version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositoryCommandOutcome {
  /// Whether the mutation was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Safe application projection committed by the original command.
  pub repository: RepositoryProjection,
}

/// Reads one exact immutable Repository version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetRepositoryQuery {
  /// Stable Repository identity.
  pub repository_id: RepositoryId,
  /// Exact immutable version.
  pub version: RepositoryVersion,
}

impl Query for GetRepositoryQuery {
  type Outcome = RepositoryProjection;
}

impl ManagementAuthorizationTarget for GetRepositoryQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Repository);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Repository, self.repository_id)
  }
}

/// Creates one Build Configuration identity and immutable initial version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateBuildConfigurationCommand {
  /// Stable Build Configuration identity.
  pub id: BuildConfigurationId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Project-local Build Configuration name.
  pub name: BuildConfigurationName,
  /// Complete initial immutable definition.
  pub definition: BuildConfigurationDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for CreateBuildConfigurationCommand {
  type Outcome = BuildConfigurationCommandOutcome;
}

impl ManagementAuthorizationTarget for CreateBuildConfigurationCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Create,
    ManagementResourceKind::BuildConfiguration,
    ManagementResourceKind::Project,
  );

  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::BuildConfiguration,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Appends the next immutable version of one Build Configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishBuildConfigurationVersionCommand {
  /// Existing Build Configuration identity.
  pub id: BuildConfigurationId,
  /// Version that must still be current.
  pub expected_current_version: BuildConfigurationVersion,
  /// Complete next immutable definition.
  pub definition: BuildConfigurationDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for PublishBuildConfigurationVersionCommand {
  type Outcome = BuildConfigurationCommandOutcome;
}

impl ManagementAuthorizationTarget for PublishBuildConfigurationVersionCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Publish, ManagementResourceKind::BuildConfiguration);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::BuildConfiguration, self.id)
  }
}

/// Result of creating or publishing one Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildConfigurationCommandOutcome {
  /// Whether the mutation was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Safe application projection committed by the original command.
  pub configuration: BuildConfigurationProjection,
}

/// Reads one exact immutable Build Configuration version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetBuildConfigurationQuery {
  /// Stable Build Configuration identity.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable version.
  pub version: BuildConfigurationVersion,
}

impl Query for GetBuildConfigurationQuery {
  type Outcome = BuildConfigurationProjection;
}

impl ManagementAuthorizationTarget for GetBuildConfigurationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::BuildConfiguration);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::BuildConfiguration, self.configuration_id)
  }
}

/// Typed Build Configuration command and query handlers backed by one narrow port.
pub struct BuildConfigurationHandlers<S> {
  store: Arc<S>,
}

impl<S> BuildConfigurationHandlers<S> {
  /// Creates handlers from a backend-neutral Configuration port.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> CommandTransaction<CreateRepositoryCommand> for S
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    context: &crate::ManagementRequestContext,
    command: CreateRepositoryCommand,
  ) -> Result<RepositoryCommandOutcome, Self::Error> {
    let outcome = self
      .create_repository(audited_mutation(
        context,
        CreateRepository {
          id: command.id,
          project_id: command.project_id,
          name: command.name,
          definition: command.definition,
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
        },
      )?)
      .await?;
    Ok(RepositoryCommandOutcome {
      disposition: outcome.disposition.into(),
      repository: outcome.repository.into(),
    })
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<CreateRepositoryCommand> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: CreateRepositoryCommand,
  ) -> Result<RepositoryCommandOutcome, Self::Error> {
    self.store.commit_command(context, command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<PublishRepositoryVersionCommand> for S
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    context: &crate::ManagementRequestContext,
    command: PublishRepositoryVersionCommand,
  ) -> Result<RepositoryCommandOutcome, Self::Error> {
    let outcome = self
      .publish_repository_version(audited_mutation(
        context,
        PublishRepositoryVersion {
          id: command.id,
          expected_current_version: command.expected_current_version,
          definition: command.definition,
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
        },
      )?)
      .await?;
    Ok(RepositoryCommandOutcome {
      disposition: outcome.disposition.into(),
      repository: outcome.repository.into(),
    })
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<PublishRepositoryVersionCommand> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: PublishRepositoryVersionCommand,
  ) -> Result<RepositoryCommandOutcome, Self::Error> {
    self.store.commit_command(context, command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<CreateBuildConfigurationCommand> for S
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    context: &crate::ManagementRequestContext,
    command: CreateBuildConfigurationCommand,
  ) -> Result<BuildConfigurationCommandOutcome, Self::Error> {
    validate_configuration_input(&command.definition)?;
    let outcome = self
      .create_build_configuration(audited_mutation(
        context,
        CreateBuildConfiguration {
          id: command.id,
          project_id: command.project_id,
          name: command.name,
          definition: command.definition,
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
        },
      )?)
      .await?;
    Ok(BuildConfigurationCommandOutcome {
      disposition: outcome.disposition.into(),
      configuration: outcome.configuration.try_into()?,
    })
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<GetRepositoryQuery> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: GetRepositoryQuery,
  ) -> Result<RepositoryProjection, Self::Error> {
    self
      .store
      .repository_version(query.repository_id, query.version)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<ListProjectRepositoriesQuery> for BuildConfigurationHandlers<S>
where
  S: ConfigurationDiscoveryStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: ListProjectRepositoriesQuery,
  ) -> Result<RepositoryPageProjection, Self::Error> {
    let page = self
      .store
      .list_project_repositories(ListProjectRepositories::new(
        query.page().project_id(),
        query.page().after().copied(),
        query.page().limit(),
        grant
          .visibility_for::<ListProjectRepositoriesQuery>()
          .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?,
      )?)
      .await?;
    Ok(RepositoryPageProjection {
      items: page.items.into_iter().map(Into::into).collect(),
      next_cursor: page.next_cursor,
    })
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<CreateBuildConfigurationCommand> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: CreateBuildConfigurationCommand,
  ) -> Result<BuildConfigurationCommandOutcome, Self::Error> {
    self.store.commit_command(context, command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<PublishBuildConfigurationVersionCommand> for S
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    context: &crate::ManagementRequestContext,
    command: PublishBuildConfigurationVersionCommand,
  ) -> Result<BuildConfigurationCommandOutcome, Self::Error> {
    validate_configuration_input(&command.definition)?;
    let outcome = self
      .publish_build_configuration_version(audited_mutation(
        context,
        PublishBuildConfigurationVersion {
          id: command.id,
          expected_current_version: command.expected_current_version,
          definition: command.definition,
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
        },
      )?)
      .await?;
    Ok(BuildConfigurationCommandOutcome {
      disposition: outcome.disposition.into(),
      configuration: outcome.configuration.try_into()?,
    })
  }
}

fn validate_configuration_input(definition: &BuildConfigurationDefinition) -> Result<(), ApplicationError> {
  validate_configuration_projection(definition).map_err(|_| ApplicationError::invalid())
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<PublishBuildConfigurationVersionCommand> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: PublishBuildConfigurationVersionCommand,
  ) -> Result<BuildConfigurationCommandOutcome, Self::Error> {
    self.store.commit_command(context, command).await
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<GetBuildConfigurationQuery> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: GetBuildConfigurationQuery,
  ) -> Result<BuildConfigurationProjection, Self::Error> {
    self
      .store
      .build_configuration_version(query.configuration_id, query.version)
      .await?
      .try_into()
      .map_err(Into::into)
  }
}

#[async_trait]
impl<S> crate::ManagementQueryUseCase<ListProjectBuildConfigurationsQuery> for BuildConfigurationHandlers<S>
where
  S: ConfigurationDiscoveryStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: ListProjectBuildConfigurationsQuery,
  ) -> Result<BuildConfigurationPageProjection, Self::Error> {
    let page = self
      .store
      .list_project_build_configurations(ListProjectBuildConfigurations::new(
        query.page().project_id(),
        query.page().after().copied(),
        query.page().limit(),
        grant
          .visibility_for::<ListProjectBuildConfigurationsQuery>()
          .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?,
      )?)
      .await?;
    Ok(BuildConfigurationPageProjection {
      items: page.items.into_iter().map(Into::into).collect(),
      next_cursor: page.next_cursor,
    })
  }
}
