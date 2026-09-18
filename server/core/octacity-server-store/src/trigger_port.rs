use async_trait::async_trait;

use crate::{StoreError, TriggerDefinitionRef, TriggerKind, TriggerTarget};

/// Backend-neutral lookup for an enabled Trigger definition.
#[async_trait]
pub trait TriggerDefinitionStore: Send + Sync {
  /// Requires the exact Trigger version, kind, and target to be enabled.
  async fn require_enabled_trigger(
    &self,
    trigger: TriggerDefinitionRef,
    kind: TriggerKind,
    target: TriggerTarget,
  ) -> Result<(), StoreError>;
}
