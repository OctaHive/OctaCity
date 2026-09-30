use async_trait::async_trait;

use crate::{
  CreateTriggerDefinition, ManagementMutation, ProjectPolicyMutationOutcome, PublishProjectPolicy, StoreError,
  TriggerDefinitionMutationOutcome,
};

/// Backend-neutral atomic mutations for policy and Trigger definitions.
#[async_trait]
pub trait DefinitionStore: Send + Sync {
  /// Publishes exactly the next immutable Project policy version.
  async fn publish_project_policy(
    &self,
    request: ManagementMutation<PublishProjectPolicy>,
  ) -> Result<ProjectPolicyMutationOutcome, StoreError>;

  /// Creates one immutable Trigger definition.
  async fn create_trigger_definition(
    &self,
    request: ManagementMutation<CreateTriggerDefinition>,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError>;
}
