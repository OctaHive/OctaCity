use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, ProjectId, ProjectPolicyVersion, Timestamp, TriggerId,
  TriggerVersion,
};
use octacity_server_store::{CreateTriggerDefinition, DefinitionStore, IdempotencyKey, PublishProjectPolicy};
use octacity_server_trigger::TriggerKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
  ApplicationError, Command, CommandHandler, CommandTransaction, MutationDisposition, ProjectPolicyDefinition,
};

/// Publishes exactly the next immutable policy version for one Project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishProjectPolicyCommand {
  /// Project that owns the policy.
  pub project_id: ProjectId,
  /// Current version expected by the caller, or `None` for initial publication.
  pub expected_current_version: Option<ProjectPolicyVersion>,
  /// Strict typed policy-directive document.
  pub policy: ProjectPolicyDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Creates one immutable Trigger definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateTriggerDefinitionCommand {
  /// Stable Trigger identity.
  pub id: TriggerId,
  /// Initial immutable Trigger version.
  pub version: TriggerVersion,
  /// Target Build Configuration identity.
  pub configuration_id: BuildConfigurationId,
  /// Exact target Build Configuration version.
  pub configuration_version: BuildConfigurationVersion,
  /// Normalized Trigger origin.
  pub kind: TriggerKind,
  /// Whether new occurrences may be accepted.
  pub enabled: bool,
  /// Strict kind-specific definition document.
  pub definition: Value,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative creation time.
  pub created_at: Timestamp,
}

/// Result of publishing one Project policy version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectPolicyCommandOutcome {
  /// Whether the mutation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Project that owns the policy.
  pub project_id: ProjectId,
  /// Exact published version.
  pub version: ProjectPolicyVersion,
}

/// Result of creating one Trigger definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TriggerDefinitionCommandOutcome {
  /// Whether the mutation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Stable Trigger identity.
  pub trigger_id: TriggerId,
  /// Exact immutable version.
  pub version: TriggerVersion,
}

impl Command for PublishProjectPolicyCommand {
  type Outcome = ProjectPolicyCommandOutcome;
}

impl Command for CreateTriggerDefinitionCommand {
  type Outcome = TriggerDefinitionCommandOutcome;
}

/// Typed policy and Trigger-definition command handlers backed by one narrow port.
pub struct DefinitionHandlers<S> {
  store: Arc<S>,
}

impl<S> DefinitionHandlers<S> {
  /// Creates handlers from a backend-neutral definition store.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> CommandTransaction<PublishProjectPolicyCommand> for S
where
  S: DefinitionStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    command: PublishProjectPolicyCommand,
  ) -> Result<ProjectPolicyCommandOutcome, Self::Error> {
    let policy = serde_json::to_value(command.policy).map_err(|_| ApplicationError::unavailable())?;
    let outcome = self
      .publish_project_policy(PublishProjectPolicy {
        project_id: command.project_id,
        expected_current_version: command.expected_current_version,
        policy,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    Ok(ProjectPolicyCommandOutcome {
      disposition: outcome.disposition.into(),
      project_id: outcome.project_id,
      version: outcome.version,
    })
  }
}

#[async_trait]
impl<S> CommandHandler<PublishProjectPolicyCommand> for DefinitionHandlers<S>
where
  S: DefinitionStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: PublishProjectPolicyCommand,
  ) -> Result<ProjectPolicyCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<CreateTriggerDefinitionCommand> for S
where
  S: DefinitionStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    command: CreateTriggerDefinitionCommand,
  ) -> Result<TriggerDefinitionCommandOutcome, Self::Error> {
    let outcome = self
      .create_trigger_definition(CreateTriggerDefinition {
        id: command.id,
        version: command.version,
        configuration_id: command.configuration_id,
        configuration_version: command.configuration_version,
        kind: command.kind,
        enabled: command.enabled,
        definition: command.definition,
        idempotency_key: command.idempotency_key,
        created_at: command.created_at,
      })
      .await?;
    Ok(TriggerDefinitionCommandOutcome {
      disposition: outcome.disposition.into(),
      trigger_id: outcome.trigger_id,
      version: outcome.version,
    })
  }
}

#[async_trait]
impl<S> CommandHandler<CreateTriggerDefinitionCommand> for DefinitionHandlers<S>
where
  S: DefinitionStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: CreateTriggerDefinitionCommand,
  ) -> Result<TriggerDefinitionCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}
