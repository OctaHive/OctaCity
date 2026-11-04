//! Bounded resumable materialization; the source Run version is its durable checkpoint.
use crate::ApplicationError;
use octacity_server_domain::Timestamp;
use octacity_server_store::{FactoryActionProjectionStore, StoreError};
use std::sync::Arc;

/// One maintenance pass, independent of execution ownership and admission.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryActionProjectionProgress {
  /// Fully published source versions, including empty projections.
  pub completed: usize,
  /// Source versions changed after selection; a later pass selects the new version.
  pub conflicted: usize,
  /// Failed publications left pending for restart/retry.
  pub failed: usize,
  /// More missing source versions existed beyond this bounded pass.
  pub has_more: bool,
}
/// Uses the common descriptor/store port without provider or Issue dependencies.
pub struct FactoryActionProjectionWorker<S> {
  store: Arc<S>,
}
impl<S: FactoryActionProjectionStore> FactoryActionProjectionWorker<S> {
  /// Composes over the same store as ordinary Factory operations.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
  /// Publishes at most `limit` source versions. No acknowledgement precedes commit.
  pub async fn run_once(&self, at: Timestamp, limit: u16) -> Result<FactoryActionProjectionProgress, ApplicationError> {
    let batch = self.store.factory_action_projection_work(limit).await?;
    let mut progress = FactoryActionProjectionProgress {
      has_more: batch.has_more,
      ..Default::default()
    };
    for work in batch.items {
      match self.store.refresh_factory_action_projection(work, at).await {
        Ok(()) => progress.completed += 1,
        Err(StoreError::Conflict { .. }) => progress.conflicted += 1,
        Err(_) => progress.failed += 1,
      }
    }
    Ok(progress)
  }
}
