//! Shared terminal-Attempt fencing for installed ordinary output verifiers.
use crate::{ApplicationError, FactoryNodeBuildOutputSource, FactoryNodeBuildOutputs, OrdinaryBuildApplicationError};
use async_trait::async_trait;
use octacity_server_domain::{AttemptId, BuildId};
use octacity_server_factory::{BudgetUsage, FlowBuildExecution, FlowBuildIntent};
use octacity_server_orchestrator::AttemptState;
use octacity_server_store::{BuildQueryStore, StoreError};

pub(super) struct VerifiedNodeBuildOutputs<'a, B, O, F> {
  pub builds: &'a B,
  pub source: &'a O,
  pub intent: &'a FlowBuildIntent,
  pub verify: F,
}
#[async_trait]
impl<B, O, F> FactoryNodeBuildOutputSource for VerifiedNodeBuildOutputs<'_, B, O, F>
where
  B: BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
  F: Fn(&FlowBuildExecution, &FactoryNodeBuildOutputs) -> Result<(), ApplicationError> + Send + Sync,
{
  async fn terminal_usage(
    &self,
    build: BuildId,
    attempt: AttemptId,
  ) -> Result<BudgetUsage, OrdinaryBuildApplicationError> {
    self.source.terminal_usage(build, attempt).await
  }
  async fn published_outputs(
    &self,
    build: BuildId,
    attempt_id: AttemptId,
    max_bytes: u64,
  ) -> Result<FactoryNodeBuildOutputs, OrdinaryBuildApplicationError> {
    let outputs = self.source.published_outputs(build, attempt_id, max_bytes).await?;
    let attempt = self.builds.attempt(attempt_id).await.map_err(|error| match error {
      StoreError::Unavailable => OrdinaryBuildApplicationError::Unavailable,
      _ => OrdinaryBuildApplicationError::Invalid,
    })?;
    if attempt.id != attempt_id || attempt.build_id != build || attempt.state != AttemptState::Succeeded {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    let execution = FlowBuildExecution::new(
      self.intent,
      build,
      attempt_id,
      attempt.jobs.iter().map(|job| job.id()).collect(),
    )
    .map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
    (self.verify)(&execution, &outputs).map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
    Ok(outputs)
  }
}
