//! Installed test-authoring capability, without logical Flow stage dispatch.
use crate::{
  ApplicationError, FactoryNodeBuildAdapter, FactoryNodeBuildOutputSource, FactoryNodeBuildOutputs,
  FactoryNodeCancellationStep, FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor,
  OrdinaryBuildCancellationApplication, verify_factory_protected_test_bundle,
};
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{EvidenceOutputKind, EvidenceRequirement, FlowBuildIntent, ProtectedTestPlan};
use octacity_server_store::BuildQueryStore;
use std::sync::Arc;

/// Trusted compiler port for one deterministic, immutable authoring plan.
/// Implementations resolve the exact pinned profile and the explicitly selected
/// accepted specification or Accepted Work Contract from authoritative records.
/// Every call after restart must reproduce the same contract, classes and deadline;
/// model outputs and mutable current-stage pointers cannot supply plan authority.
#[async_trait]
pub trait FactoryProtectedTestPlanSource: Send + Sync {
  /// Returns the plan bound to this frozen input and immutable execution profile.
  async fn frozen_plan(&self, intent: &FlowBuildIntent) -> Result<ProtectedTestPlan, ApplicationError>;
}
/// Ordinary Build authoring with private file capture and a closed public result.
pub struct FactoryProtectedTestAuthoringAdapter<B, O, P> {
  builds: Arc<B>,
  outputs: Arc<O>,
  plans: Arc<P>,
  manifest: EvidenceRequirement,
}
impl<B, O, P> FactoryProtectedTestAuthoringAdapter<B, O, P> {
  /// Installs existing Build ports, trusted plan resolution and exact private output selection.
  pub fn new(builds: Arc<B>, outputs: Arc<O>, plans: Arc<P>, manifest: EvidenceRequirement) -> Self {
    Self {
      builds,
      outputs,
      plans,
      manifest,
    }
  }
}
#[async_trait]
impl<B, O, P> FactoryNodeExecutor for FactoryProtectedTestAuthoringAdapter<B, O, P>
where
  B: OrdinaryBuildCancellationApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
  P: FactoryProtectedTestPlanSource,
{
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let intent = request.build.ok_or_else(ApplicationError::invalid)?;
    let node = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .and_then(|definition| definition.node(request.input.node()))
      .ok_or_else(ApplicationError::invalid)?;
    let binding = node.build().ok_or_else(ApplicationError::invalid)?;
    let schema = node
      .outcome(intent.result_outcome())
      .ok_or_else(ApplicationError::invalid)?
      .schema();
    if intent.profile().result_tool() != self.manifest.tool()
      || self.manifest.output_kind() != EvidenceOutputKind::Artifact
      || binding.evidence != [self.manifest.clone()]
    {
      return Err(ApplicationError::invalid());
    }
    let plan = self.plans.frozen_plan(intent).await?;
    if intent
      .input()
      .context()
      .is_none_or(|context| context.subject() != plan.subject())
    {
      return Err(ApplicationError::invalid());
    }
    let observed_at = request.observed_at;
    let outputs = Arc::new(crate::factory_verified_build_outputs::VerifiedNodeBuildOutputs {
      builds: self.builds.as_ref(),
      source: self.outputs.as_ref(),
      intent,
      verify: |execution: &octacity_server_factory::FlowBuildExecution, outputs: &FactoryNodeBuildOutputs| {
        let bundle =
          verify_factory_protected_test_bundle(intent, execution, outputs, &plan, &self.manifest, observed_at)?;
        let public = crate::factory_node_build::selected_document(
          &outputs.documents,
          &intent.profile().result_output,
          intent.profile().result_output_kind,
          schema,
          intent.profile().result_tool(),
          &intent.profile().plugin,
        )?;
        let handoff: octacity_server_factory::ProtectedTestHandoff =
          serde_json::from_slice(&public.bytes).map_err(|_| ApplicationError::invalid())?;
        if handoff != bundle.public_handoff() {
          return Err(ApplicationError::invalid());
        }
        Ok(())
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
