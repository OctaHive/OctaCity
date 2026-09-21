use async_trait::async_trait;

use crate::{
  CreateTriggerDefinition, ProjectPolicyMutationOutcome, PublishProjectPolicy, StoreError,
  TriggerDefinitionMutationOutcome,
};

/// Backend-neutral atomic mutations for policy and Trigger definitions.
#[async_trait]
pub trait DefinitionStore: Send + Sync {
  /// Publishes exactly the next immutable Project policy version.
  async fn publish_project_policy(
    &self,
    request: PublishProjectPolicy,
  ) -> Result<ProjectPolicyMutationOutcome, StoreError>;

  /// Creates one immutable Trigger definition.
  async fn create_trigger_definition(
    &self,
    request: CreateTriggerDefinition,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError>;
}
