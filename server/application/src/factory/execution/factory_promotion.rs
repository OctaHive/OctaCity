//! Promotion qualifications around the existing ordinary CI/CD Build adapter.
use crate::{
  ApplicationError, FactoryNodeBuildAdapter, FactoryNodeBuildOutputSource, FactoryNodeCancellationStep,
  FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor, OrdinaryBuildCancellationApplication,
};
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::BuildQueryStore;

/// Installed trusted promotion capability. It owns qualification, while ordinary
/// CI/CD owns deployment identity, execution, observation and rollback mechanics.
/// Models receive neither this executor nor the deployment system's credentials.
pub struct FactoryPromotionBuildAdapter<B, O> {
  identity: ImmutableReference,
  build: FactoryNodeBuildAdapter<B, O>,
}
impl<B, O> FactoryPromotionBuildAdapter<B, O> {
  /// Wraps the existing Build adapter under one immutable installed identity.
  #[must_use]
  pub fn new(identity: ImmutableReference, build: FactoryNodeBuildAdapter<B, O>) -> Self {
    Self { identity, build }
  }
  fn plan(&self, request: &FactoryNodeExecutionRequest<'_>) -> Result<PromotionPlan, ApplicationError> {
    request
      .input
      .verify_attempt(request.attempt)
      .map_err(|_| ApplicationError::invalid())?;
    let intent = request.build.ok_or_else(ApplicationError::invalid)?;
    let action = intent.action().ok_or_else(ApplicationError::invalid)?;
    if request.run_id != request.input.factory_run_id()
      || intent.input() != request.input
      || intent.node() != request.attempt
      || request.attempt.node_kind() != FlowNodeKind::TrustedAction
      || action.plugin() != &self.identity
      || intent.profile().tool != self.identity
      || !intent.permissions().plugins().any(|allowed| allowed == &self.identity)
    {
      return Err(ApplicationError::invalid());
    }
    let plan = PromotionPlan::freeze(request.admitted, request.input).map_err(|_| ApplicationError::invalid())?;
    plan
      .authorize(request.work, request.admitted, request.input)
      .map_err(|_| ApplicationError::invalid())?;
    Ok(plan)
  }
}
#[async_trait]
impl<B, O> FactoryNodeExecutor for FactoryPromotionBuildAdapter<B, O>
where
  B: OrdinaryBuildCancellationApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
{
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let plan = self.plan(&request)?;
    let input = request.input;
    let started_at = request.attempt.started_at();
    let deadline = request.attempt.deadline();
    let at = request.observed_at;
    let action_input =
      serde_json::json!({"plan":plan,"plan_digest":plan.digest().map_err(|_|ApplicationError::invalid())?});
    let step = self.build.with_action_input(action_input)?.observe(request).await?;
    match step {
      FactoryNodeExecutionStep::Completed {
        execution,
        record,
        usage,
        outbox,
      } => {
        if record
          .build_provenance()
          .is_none_or(|proof| proof.outputs().iter().any(|output| output.fresh_until <= at))
        {
          return Err(ApplicationError::invalid());
        }
        let observation: PromotionObservation =
          serde_json::from_value(record.payload().value().clone()).map_err(|_| ApplicationError::invalid())?;
        // The common Build adapter binds record.observed_at to trusted primary
        // receipt publication. Its bytes cannot describe later measurements;
        // re-verifying earlier bytes cannot repair this.
        if observation.finished_at > record.observed_at() {
          return Err(ApplicationError::invalid());
        }
        observation
          .verify(&plan, input, started_at, deadline, at)
          .map_err(|_| ApplicationError::invalid())?;
        // Emit only qualified owner facts. The common owner computes its completion
        // from this complete record, so discovery shares the retained result digest.
        let record = (*record)
          .with_discovery(FlowDiscoveryFacts::promotion(&observation))
          .map_err(|_| ApplicationError::invalid())?;
        Ok(FactoryNodeExecutionStep::Completed {
          execution,
          record: Box::new(record),
          usage,
          outbox,
        })
      }
      pending => Ok(pending),
    }
  }
  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    self.plan(&request)?;
    FactoryNodeExecutor::cancel(&self.build, request, requested_at).await
  }
}
