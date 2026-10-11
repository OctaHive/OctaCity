//! Ordinary Build capture capability over the common node execution interface.
//! Names below identify reserved capture files, never logical Flow nodes or edges.

use crate::{
  ApplicationError, FactoryNodeBuildAdapter, FactoryNodeBuildOutputSource, FactoryNodeBuildOutputs,
  FactoryNodeCancellationStep, FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor,
  OrdinaryBuildApplicationError, OrdinaryBuildCancellationApplication, verify_factory_node_change_set,
};
use async_trait::async_trait;
use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT};
use octacity_server_domain::{AttemptId, BuildId, Timestamp};
use octacity_server_factory::{
  BudgetUsage, EvidenceOutputKind, FlowBuildExecution, FlowBuildIntent, ImmutableReference,
};
use octacity_server_orchestrator::AttemptState;
use octacity_server_store::{BuildQueryStore, StoreError};
use std::sync::Arc;

/// Installed capture executor: ordinary execution, verified capture, generic result.
/// Its immutable tool binding is supplied by composition. Cancellation reuses
/// ordinary stop observation and never parses capture results or selects an outcome.
pub struct FactoryNodeCaptureBuildAdapter<B, O> {
  builds: Arc<B>,
  outputs: Arc<O>,
  capture_tool: ImmutableReference,
}
impl<B, O> FactoryNodeCaptureBuildAdapter<B, O> {
  /// Connects ordinary Build ports and the exact installed trusted capture tool.
  #[must_use]
  pub fn new(builds: Arc<B>, outputs: Arc<O>, capture_tool: ImmutableReference) -> Self {
    Self {
      builds,
      outputs,
      capture_tool,
    }
  }
}
#[async_trait]
impl<B: OrdinaryBuildCancellationApplication + BuildQueryStore, O: FactoryNodeBuildOutputSource> FactoryNodeExecutor
  for FactoryNodeCaptureBuildAdapter<B, O>
{
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let intent = request.build.ok_or_else(ApplicationError::invalid)?;
    let binding = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .and_then(|definition| definition.node(request.input.node()))
      .and_then(|node| node.build())
      .ok_or_else(ApplicationError::invalid)?;
    // Reject a substituted capability selection before creating an ordinary operation.
    if intent.profile().result_tool() != &self.capture_tool
      || binding.result_output.as_str() != CHANGE_SET_MANIFEST_OUTPUT
      || binding.result_output_kind != EvidenceOutputKind::Artifact
      || !binding.evidence.iter().any(|output| {
        output.kind().as_str() == CHANGE_SET_BUNDLE_OUTPUT
          && output.output_kind() == EvidenceOutputKind::Artifact
          && output.tool() == &self.capture_tool
          && output.plugin() == &binding.plugin
      })
    {
      return Err(ApplicationError::invalid());
    }
    let outputs = Arc::new(VerifiedCaptureOutputs {
      builds: self.builds.as_ref(),
      source: self.outputs.as_ref(),
      intent,
      capture_tool: &self.capture_tool,
      observed_at: request.observed_at,
    });
    FactoryNodeExecutor::observe(&FactoryNodeBuildAdapter::new(self.builds.clone(), outputs), request).await
  }
  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    FactoryNodeExecutor::cancel(
      &FactoryNodeBuildAdapter::new(self.builds.clone(), self.outputs.clone()),
      request,
      requested_at,
    )
    .await
  }
}

/// Verification is performed before the common adapter can produce a completion.
/// The same frozen intent and complete ordinary Job identities scope both verifiers.
struct VerifiedCaptureOutputs<'a, B, O> {
  builds: &'a B,
  source: &'a O,
  intent: &'a FlowBuildIntent,
  capture_tool: &'a ImmutableReference,
  observed_at: Timestamp,
}
#[async_trait]
impl<B: BuildQueryStore, O: FactoryNodeBuildOutputSource> FactoryNodeBuildOutputSource
  for VerifiedCaptureOutputs<'_, B, O>
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
    verify_factory_node_change_set(self.intent, &execution, &outputs, self.capture_tool, self.observed_at)
      .map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
    Ok(outputs)
  }
}
