use async_trait::async_trait;

use crate::{CriticalSystemConditionChange, ListOperatorAttention, OperatorAttentionPage, StoreError};

/// Backend-neutral write port for server-classified critical conditions.
#[async_trait]
pub trait CriticalSystemConditionStore: Send + Sync {
  /// Records one validated condition opening or resolution.
  async fn record_critical_system_condition(&self, change: CriticalSystemConditionChange) -> Result<(), StoreError>;
}

/// Backend-neutral operator-attention read port.
#[async_trait]
pub trait OperatorAttentionStore: Send + Sync {
  /// Lists safe visible attention items in deterministic newest-first order.
  async fn list_operator_attention(&self, request: ListOperatorAttention) -> Result<OperatorAttentionPage, StoreError>;
}
