//! Exact installed executor selection; logical node names carry no dispatch meaning.

use crate::{
  ApplicationError, FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor,
  FactoryScopedValidationCall,
};
use async_trait::async_trait;
use octacity_server_factory::{FlowNodeKind, NodeExecutionIdentity};
use std::sync::Arc;

pub use octacity_server_factory::MAX_FACTORY_EXECUTOR_BINDINGS;

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

  fn select(&self, request: &FactoryNodeExecutionRequest<'_>) -> Result<&dyn FactoryNodeExecutor, ApplicationError> {
    self
      .bindings
      .iter()
      .find(|(kind, identity, _)| *kind == request.attempt.node_kind() && identity == request.attempt.execution())
      .map(|(_, _, executor)| executor.as_ref())
      .ok_or_else(ApplicationError::invalid)
  }
}

#[async_trait]
impl FactoryNodeExecutor for FactoryNodeExecutorRegistry {
  async fn scoped_validation(
    &self,
    action: FactoryScopedValidationCall,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<crate::FactoryScopedValidationVerdict, ApplicationError> {
    self.select(&request)?.scoped_validation(action, request).await
  }

  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    requested_at: octacity_server_domain::Timestamp,
  ) -> Result<crate::FactoryNodeCancellationStep, ApplicationError> {
    self.select(&request)?.cancel(request, requested_at).await
  }
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    self.select(&request)?.observe(request).await
  }
}
