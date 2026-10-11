//! Ordinary Build capture capability over the common node execution interface.
//! Names below identify reserved capture files, never logical Flow nodes or edges.

use crate::{
  ApplicationError, FactoryNodeBuildAdapter, FactoryNodeBuildOutputSource, FactoryNodeBuildOutputs,
  FactoryNodeCancellationStep, FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor,
  OrdinaryBuildCancellationApplication, verify_factory_node_change_set,
};
use async_trait::async_trait;
use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{EvidenceOutputKind, FlowBuildExecution, ImmutableReference};
use octacity_server_store::BuildQueryStore;
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
    let primary_manifest = intent.profile().result_tool() == &self.capture_tool
      && binding.result_output.as_str() == CHANGE_SET_MANIFEST_OUTPUT
      && binding.result_output_kind == EvidenceOutputKind::Artifact;
    let retained_manifest = binding.evidence.iter().any(|output| {
      output.kind().as_str() == CHANGE_SET_MANIFEST_OUTPUT
        && output.output_kind() == EvidenceOutputKind::Artifact
        && output.tool() == &self.capture_tool
        && output.plugin() == &binding.plugin
    });
    if (!primary_manifest && !retained_manifest)
      || !binding.evidence.iter().any(|output| {
        output.kind().as_str() == CHANGE_SET_BUNDLE_OUTPUT
          && output.output_kind() == EvidenceOutputKind::Artifact
          && output.tool() == &self.capture_tool
          && output.plugin() == &binding.plugin
      })
    {
      return Err(ApplicationError::invalid());
    }
    let observed_at = request.observed_at;
    let outputs = Arc::new(crate::factory_verified_build_outputs::VerifiedNodeBuildOutputs {
      builds: self.builds.as_ref(),
      source: self.outputs.as_ref(),
      intent,
      verify: |execution: &FlowBuildExecution, outputs: &FactoryNodeBuildOutputs| {
        verify_factory_node_change_set(intent, execution, outputs, &self.capture_tool, observed_at)
          .map(|_| ())
          .map_err(|_| ApplicationError::invalid())
      },
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
