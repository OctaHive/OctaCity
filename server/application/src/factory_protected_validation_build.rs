//! Ordinary command execution with exact protected-input result verification.
use crate::{
  ApplicationError, FactoryNodeBuildAdapter, FactoryNodeBuildOutputSource, FactoryNodeBuildOutputs,
  FactoryNodeCancellationStep, FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor,
  FactoryProtectedValidationMaterialization, FactoryProtectedValidationReport, OrdinaryBuildCancellationApplication,
};
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{EvidenceOutputKind, EvidenceRequirement, FactoryDigest, FlowBuildIntent, FlowNodeKind};
use octacity_server_store::BuildQueryStore;
use std::sync::Arc;

/// Trusted compiler port that restores verified candidates and retained tests
/// from the input's explicit Context selections. It must never infer a logical
/// node name, select a mutable latest candidate or acquire undeclared Artifacts.
#[async_trait]
pub trait FactoryProtectedValidationInputSource: Send + Sync {
  /// Resolves the same exact materialization used to sign the ordinary command Job.
  async fn materialization(
    &self,
    intent: &FlowBuildIntent,
  ) -> Result<FactoryProtectedValidationMaterialization, ApplicationError>;
}
/// Installed command capability. Results are verified before common completion;
/// configured deterministic gates remain responsible for routing the outcome.
pub struct FactoryProtectedValidationAdapter<B, O, V> {
  builds: Arc<B>,
  outputs: Arc<O>,
  inputs: Arc<V>,
  report: EvidenceRequirement,
  facts: EvidenceRequirement,
}
impl<B, O, V> FactoryProtectedValidationAdapter<B, O, V> {
  /// Binds existing ordinary execution, exact compiler inputs and declared outputs.
  pub fn new(
    builds: Arc<B>,
    outputs: Arc<O>,
    inputs: Arc<V>,
    report: EvidenceRequirement,
    facts: EvidenceRequirement,
  ) -> Self {
    Self {
      builds,
      outputs,
      inputs,
      report,
      facts,
    }
  }
}
#[async_trait]
impl<B, O, V> FactoryNodeExecutor for FactoryProtectedValidationAdapter<B, O, V>
where
  B: OrdinaryBuildCancellationApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
  V: FactoryProtectedValidationInputSource,
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
    let primary = binding.result_output == *self.report.kind()
      && binding.result_output_kind == self.report.output_kind()
      && binding.result_tool.as_ref().unwrap_or(&binding.tool) == self.report.tool()
      && binding.plugin == *self.report.plugin()
      && node
        .outcome(intent.result_outcome())
        .is_some_and(|outcome| outcome.schema() == self.report.schema());
    if node.kind() != FlowNodeKind::BuildCommand
      || self.report.output_kind() != EvidenceOutputKind::Report
      || self.facts.output_kind() != EvidenceOutputKind::Report
      || !binding.evidence.contains(&self.facts)
      || (!primary && !binding.evidence.contains(&self.report))
    {
      return Err(ApplicationError::invalid());
    }
    let materialized = self.inputs.materialization(intent).await?;
    if materialized.input() != intent.input() {
      return Err(ApplicationError::invalid());
    }
    let at = request.observed_at;
    let outputs = Arc::new(crate::factory_verified_build_outputs::VerifiedNodeBuildOutputs {
      builds: self.builds.as_ref(),
      source: self.outputs.as_ref(),
      intent,
      verify: |_: &octacity_server_factory::FlowBuildExecution, outputs: &FactoryNodeBuildOutputs| {
        let selected = |requirement: &EvidenceRequirement| {
          crate::factory_node_build::selected_document(
            &outputs.documents,
            requirement.kind(),
            requirement.output_kind(),
            requirement.schema(),
            requirement.tool(),
            requirement.plugin(),
          )
        };
        let document = selected(&self.report)?;
        let report: FactoryProtectedValidationReport =
          serde_json::from_slice(&document.bytes).map_err(|_| ApplicationError::invalid())?;
        materialized.verify_report(&report)?;
        let facts = serde_json::from_slice(&selected(&self.facts)?.bytes).map_err(|_| ApplicationError::invalid())?;
        materialized.verify_facts(
          &report,
          FactoryDigest::content_sha256(&document.bytes),
          &facts,
          outputs.fresh_until,
          at,
        )?;
        Ok(())
      },
    });
    FactoryNodeExecutor::observe(&FactoryNodeBuildAdapter::new(self.builds.clone(), outputs), request).await
  }
  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    at: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    FactoryNodeExecutor::cancel(
      &FactoryNodeBuildAdapter::new(self.builds.clone(), self.outputs.clone()),
      request,
      at,
    )
    .await
  }
}
