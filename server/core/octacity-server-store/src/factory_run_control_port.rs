use async_trait::async_trait;

use crate::{ApplyFactoryRunControl, FactoryRunControlOutcome, ManagementMutation, StoreError};

/// Backend-neutral atomic port for operator control of Factory Runs.
#[async_trait]
pub trait FactoryRunControlStore: Send + Sync {
  /// Applies or exactly replays one closed, strongly preconditioned intent.
  async fn apply_factory_run_control(
    &self,
    request: ManagementMutation<ApplyFactoryRunControl>,
  ) -> Result<FactoryRunControlOutcome, StoreError>;
}
