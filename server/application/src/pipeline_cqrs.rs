use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{PipelineId, PipelineName, PipelineVersion, ProjectId, Timestamp};
use octacity_server_job::JobExecutionTemplate;
use octacity_server_pipeline::PublishablePipelineDag;
use octacity_server_store::{CreatePipeline, IdempotencyKey, PipelineStore, PublishPipelineVersion};
use serde::{Deserialize, Serialize};

use crate::{
  ApplicationError, Command, CommandHandler, CommandTransaction, ManagementAction, ManagementAuthorizationTarget,
  ManagementResource, ManagementResourceKind, MutationDisposition, PipelineProjection, ProjectionError, Query,
  QueryHandler,
  management_security::{instance_resource, owned_collection_resource},
};

/// Creates one Pipeline identity and immutable initial version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatePipelineCommand {
  /// Stable Pipeline identity.
  pub id: PipelineId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Project-local Pipeline name.
  pub name: PipelineName,
  /// Validated immutable initial DAG.
  pub dag: PublishablePipelineDag,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for CreatePipelineCommand {
  type Outcome = PipelineCommandOutcome;
}

impl ManagementAuthorizationTarget for CreatePipelineCommand {
  fn management_action(&self) -> ManagementAction {
    ManagementAction::Create
  }

  fn management_resource(&self) -> ManagementResource {
    owned_collection_resource(
      ManagementResourceKind::Pipeline,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Appends the next immutable version of one Pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishPipelineVersionCommand {
  /// Existing Pipeline identity.
  pub id: PipelineId,
  /// Version that must still be current.
  pub expected_current_version: PipelineVersion,
  /// Validated immutable next DAG.
  pub dag: PublishablePipelineDag,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl Command for PublishPipelineVersionCommand {
  type Outcome = PipelineCommandOutcome;
}

impl ManagementAuthorizationTarget for PublishPipelineVersionCommand {
  fn management_action(&self) -> ManagementAction {
    ManagementAction::Publish
  }

  fn management_resource(&self) -> ManagementResource {
    instance_resource(ManagementResourceKind::Pipeline, self.id)
  }
}

/// Result of creating or publishing one Pipeline version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PipelineCommandOutcome {
  /// Whether the mutation was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Safe application projection committed by the original command.
  pub pipeline: PipelineProjection,
}

/// Reads one exact immutable Pipeline version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetPipelineQuery {
  /// Stable Pipeline identity.
  pub pipeline_id: PipelineId,
  /// Exact immutable version.
  pub version: PipelineVersion,
}

impl Query for GetPipelineQuery {
  type Outcome = PipelineProjection;
}

impl ManagementAuthorizationTarget for GetPipelineQuery {
  fn management_action(&self) -> ManagementAction {
    ManagementAction::View
  }

  fn management_resource(&self) -> ManagementResource {
    instance_resource(ManagementResourceKind::Pipeline, self.pipeline_id)
  }
}

/// Typed Pipeline command and query handlers backed by one narrow port.
pub struct PipelineHandlers<S> {
  store: Arc<S>,
}

impl<S> PipelineHandlers<S> {
  /// Creates handlers from a backend-neutral Pipeline port.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> CommandTransaction<CreatePipelineCommand> for S
where
  S: PipelineStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(&self, command: CreatePipelineCommand) -> Result<PipelineCommandOutcome, Self::Error> {
    validate_execution_templates(&command.dag)?;
    let outcome = self
      .create_pipeline(CreatePipeline {
        id: command.id,
        project_id: command.project_id,
        name: command.name,
        dag: command.dag,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    Ok(PipelineCommandOutcome {
      disposition: outcome.disposition.into(),
      pipeline: outcome.pipeline.try_into()?,
    })
  }
}

#[async_trait]
impl<S> CommandHandler<CreatePipelineCommand> for PipelineHandlers<S>
where
  S: PipelineStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: CreatePipelineCommand) -> Result<PipelineCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<PublishPipelineVersionCommand> for S
where
  S: PipelineStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    command: PublishPipelineVersionCommand,
  ) -> Result<PipelineCommandOutcome, Self::Error> {
    validate_execution_templates(&command.dag)?;
    let outcome = self
      .publish_pipeline_version(PublishPipelineVersion {
        id: command.id,
        expected_current_version: command.expected_current_version,
        dag: command.dag,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    Ok(PipelineCommandOutcome {
      disposition: outcome.disposition.into(),
      pipeline: outcome.pipeline.try_into()?,
    })
  }
}

#[async_trait]
impl<S> CommandHandler<PublishPipelineVersionCommand> for PipelineHandlers<S>
where
  S: PipelineStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: PublishPipelineVersionCommand,
  ) -> Result<PipelineCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> QueryHandler<GetPipelineQuery> for PipelineHandlers<S>
where
  S: PipelineStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetPipelineQuery) -> Result<PipelineProjection, Self::Error> {
    self
      .store
      .pipeline_version(query.pipeline_id, query.version)
      .await?
      .try_into()
      .map_err(Into::into)
  }
}

fn validate_execution_templates(dag: &PublishablePipelineDag) -> Result<(), ProjectionError> {
  for node in dag.nodes() {
    serde_json::from_value::<JobExecutionTemplate>(node.template().clone())
      .map_err(|_| ProjectionError::InvalidPipelineTemplate)?;
  }
  Ok(())
}
