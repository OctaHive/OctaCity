//! Bounded cleanup of durable private compiler inputs.
use crate::ApplicationError;
use octacity_artifact_store::{ArtifactObject, ArtifactStore};
use octacity_server_domain::Timestamp;
use octacity_server_store::{CompleteFactoryExecutionInputDeletion, FactoryExecutionInputRetentionStore};
use std::sync::Arc;

/// Deletes expired immutable objects before acknowledging their cleanup.
/// Failures leave the exact identity available for another bounded pass.
pub struct FactoryExecutionInputRetentionWorker<S, B: ?Sized> {
  store: Arc<S>,
  bytes: Arc<B>,
}
impl<S: FactoryExecutionInputRetentionStore, B: ArtifactStore + ?Sized> FactoryExecutionInputRetentionWorker<S, B> {
  /// Installs authoritative expiration and idempotent physical deletion.
  pub fn new(store: Arc<S>, bytes: Arc<B>) -> Self {
    Self { store, bytes }
  }
  /// Removes at most `limit` objects using a trusted observation time.
  /// Replica overlap is safe: expired identities cannot be republished, and
  /// both physical deletion and durable acknowledgements are idempotent.
  pub async fn run_once(&self, observed_at: Timestamp, limit: u16) -> Result<usize, ApplicationError> {
    let inputs = self.store.expired_factory_execution_inputs(observed_at, limit).await?;
    let mut completed = 0;
    for input in inputs {
      let object = ArtifactObject::new(
        input.artifact_id,
        input.upload_id,
        input.size_bytes,
        input.digest.to_string(),
        input.media_type.as_str(),
      )
      .map_err(|_| ApplicationError::invalid())?;
      self
        .bytes
        .delete(&object)
        .await
        .map_err(|_| ApplicationError::unavailable())?;
      self
        .store
        .complete_factory_execution_input_deletion(CompleteFactoryExecutionInputDeletion {
          expected: input,
          deleted_at: observed_at,
        })
        .await?;
      completed += 1;
    }
    Ok(completed)
  }
}
