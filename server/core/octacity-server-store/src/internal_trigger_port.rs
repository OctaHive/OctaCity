use async_trait::async_trait;

use crate::{
  ClaimInternalTriggerEvents, CompleteInternalTriggerEvent, CreateInternalTriggerDefinition,
  InternalTriggerDefinitionPage, InternalTriggerDefinitionRecord, InternalTriggerEventClaim,
  ListInternalTriggerDefinitions, MutationDisposition, PublishInternalTriggerVersion, StoreError,
  TriggerDefinitionMutationOutcome,
};
use octacity_server_domain::{TriggerId, TriggerVersion};

/// Durable management operations for versioned internal Trigger definitions.
#[async_trait]
pub trait InternalTriggerDefinitionStore: Send + Sync {
  /// Creates version one of an internal Trigger.
  async fn create_internal_trigger_definition(
    &self,
    request: CreateInternalTriggerDefinition,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError>;

  /// Publishes exactly the next immutable version.
  async fn publish_internal_trigger_version(
    &self,
    request: PublishInternalTriggerVersion,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError>;

  /// Reads one exact immutable internal Trigger version.
  async fn internal_trigger_definition(
    &self,
    trigger_id: TriggerId,
    version: TriggerVersion,
  ) -> Result<InternalTriggerDefinitionRecord, StoreError>;

  /// Lists current versions in deterministic stable-identity order.
  async fn list_internal_trigger_definitions(
    &self,
    request: ListInternalTriggerDefinitions,
  ) -> Result<InternalTriggerDefinitionPage, StoreError>;
}

/// Replica-safe delivery of terminal Build events from the transactional outbox.
#[async_trait]
pub trait InternalTriggerEventStore: Send + Sync {
  /// Claims a bounded batch and loads immutable matching Trigger definitions.
  async fn claim_internal_trigger_events(
    &self,
    request: ClaimInternalTriggerEvents,
  ) -> Result<Vec<InternalTriggerEventClaim>, StoreError>;

  /// Marks one source event delivered only while the caller owns its claim.
  async fn complete_internal_trigger_event(
    &self,
    request: CompleteInternalTriggerEvent,
  ) -> Result<MutationDisposition, StoreError>;
}
