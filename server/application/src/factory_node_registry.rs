//! Exact installed executor selection; logical node names carry no dispatch meaning.

use crate::{ApplicationError, FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor};
use async_trait::async_trait;
use octacity_server_factory::{FlowNodeKind, NodeExecutionIdentity};
use std::sync::Arc;

/// Maximum installed execution bindings in one bounded Factory owner.
pub const MAX_FACTORY_EXECUTOR_BINDINGS: usize = 256;

/// One installed executor selected by primitive and exact immutable execution identity.
pub type FactoryNodeExecutorBinding = (FlowNodeKind, NodeExecutionIdentity, Arc<dyn FactoryNodeExecutor>);

/// Dispatches only to explicitly installed bindings, with no stage-name fallback.
pub struct FactoryNodeExecutorRegistry {
  bindings: Vec<FactoryNodeExecutorBinding>,
}

impl FactoryNodeExecutorRegistry {
  /// Rejects ambiguous selections before any Work can reach an executor.
  pub fn new(bindings: Vec<FactoryNodeExecutorBinding>) -> Result<Self, ApplicationError> {
    if bindings.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || bindings.iter().enumerate().any(|(index, (kind, identity, _))| {
        bindings[..index]
          .iter()
          .any(|(previous_kind, previous_identity, _)| kind == previous_kind && identity == previous_identity)
      })
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self { bindings })
  }
}

#[async_trait]
impl FactoryNodeExecutor for FactoryNodeExecutorRegistry {
  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    requested_at: octacity_server_domain::Timestamp,
  ) -> Result<crate::FactoryNodeCancellationStep, ApplicationError> {
    let (_, _, executor) = self
      .bindings
      .iter()
      .find(|(kind, identity, _)| *kind == request.attempt.node_kind() && identity == request.attempt.execution())
      .ok_or_else(ApplicationError::invalid)?;
    executor.cancel(request, requested_at).await
  }
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let (_, _, executor) = self
      .bindings
      .iter()
      .find(|(kind, identity, _)| *kind == request.attempt.node_kind() && identity == request.attempt.execution())
      .ok_or_else(ApplicationError::invalid)?;
    executor.observe(request).await
  }
}
