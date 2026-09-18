use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, ProjectId, RepositoryId, RepositoryName,
  RepositoryVersion, Timestamp,
};
use octacity_server_store::{
  BuildConfigurationDefinition, ConfigurationStore, CreateBuildConfiguration, CreateRepository, IdempotencyKey,
  PublishBuildConfigurationVersion, PublishRepositoryVersion, RepositoryDefinition,
};
use serde::{Deserialize, Serialize};

use crate::{
  ApplicationError, BuildConfigurationProjection, Command, CommandHandler, CommandTransaction, MutationDisposition,
  Query, QueryHandler, RepositoryProjection, projections::validate_configuration_projection,
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

  async fn commit_command(&self, command: CreateRepositoryCommand) -> Result<RepositoryCommandOutcome, Self::Error> {
    let outcome = self
      .create_repository(CreateRepository {
        id: command.id,
        project_id: command.project_id,
        name: command.name,
        definition: command.definition,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    Ok(RepositoryCommandOutcome {
      disposition: outcome.disposition.into(),
      repository: outcome.repository.into(),
    })
  }
}

#[async_trait]
impl<S> CommandHandler<CreateRepositoryCommand> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: CreateRepositoryCommand) -> Result<RepositoryCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
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
    command: PublishRepositoryVersionCommand,
  ) -> Result<RepositoryCommandOutcome, Self::Error> {
    let outcome = self
      .publish_repository_version(PublishRepositoryVersion {
        id: command.id,
        expected_current_version: command.expected_current_version,
        definition: command.definition,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    Ok(RepositoryCommandOutcome {
      disposition: outcome.disposition.into(),
      repository: outcome.repository.into(),
    })
  }
}

#[async_trait]
impl<S> CommandHandler<PublishRepositoryVersionCommand> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: PublishRepositoryVersionCommand,
  ) -> Result<RepositoryCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
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
    command: CreateBuildConfigurationCommand,
  ) -> Result<BuildConfigurationCommandOutcome, Self::Error> {
    validate_configuration_projection(&command.definition)?;
    let outcome = self
      .create_build_configuration(CreateBuildConfiguration {
        id: command.id,
        project_id: command.project_id,
        name: command.name,
        definition: command.definition,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    Ok(BuildConfigurationCommandOutcome {
      disposition: outcome.disposition.into(),
      configuration: outcome.configuration.try_into()?,
    })
  }
}

#[async_trait]
impl<S> QueryHandler<GetRepositoryQuery> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetRepositoryQuery) -> Result<RepositoryProjection, Self::Error> {
    self
      .store
      .repository_version(query.repository_id, query.version)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }
}

#[async_trait]
impl<S> CommandHandler<CreateBuildConfigurationCommand> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: CreateBuildConfigurationCommand,
  ) -> Result<BuildConfigurationCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
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
    command: PublishBuildConfigurationVersionCommand,
  ) -> Result<BuildConfigurationCommandOutcome, Self::Error> {
    validate_configuration_projection(&command.definition)?;
    let outcome = self
      .publish_build_configuration_version(PublishBuildConfigurationVersion {
        id: command.id,
        expected_current_version: command.expected_current_version,
        definition: command.definition,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    Ok(BuildConfigurationCommandOutcome {
      disposition: outcome.disposition.into(),
      configuration: outcome.configuration.try_into()?,
    })
  }
}

#[async_trait]
impl<S> CommandHandler<PublishBuildConfigurationVersionCommand> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: PublishBuildConfigurationVersionCommand,
  ) -> Result<BuildConfigurationCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> QueryHandler<GetBuildConfigurationQuery> for BuildConfigurationHandlers<S>
where
  S: ConfigurationStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetBuildConfigurationQuery) -> Result<BuildConfigurationProjection, Self::Error> {
    self
      .store
      .build_configuration_version(query.configuration_id, query.version)
      .await?
      .try_into()
      .map_err(Into::into)
  }
}
