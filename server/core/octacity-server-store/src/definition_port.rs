use async_trait::async_trait;

use crate::{
  CreateTriggerDefinition, ManagementMutation, ManualTriggerDefinitionRecord, ProjectPolicyMutationOutcome,
  PublishProjectPolicy, StoreError, TriggerDefinitionMutationOutcome,
};

use octacity_server_domain::{TriggerId, TriggerVersion};

/// Backend-neutral persistence for policy and manual Trigger definitions.
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

  /// Reads one exact immutable manual Trigger definition version.
  async fn manual_trigger_definition(
    &self,
    trigger_id: TriggerId,
    version: TriggerVersion,
  ) -> Result<ManualTriggerDefinitionRecord, StoreError>;
}
