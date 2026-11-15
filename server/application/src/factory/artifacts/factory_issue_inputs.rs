//! Read-only Issue provenance for ordinary inputs, independent of field-writing effects.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::sync::Arc;

/// Freezes the associated Issue revision before Context acquisition or model execution.
/// The captured origin grants no write capability and follows ordinary input replay.
pub struct FactoryIssueInputs {
  issues: Arc<dyn IssueStore>,
  associations: Arc<dyn IssueExecutionStore>,
  inputs: Arc<dyn FactoryFlowInputPreparer>,
}
impl FactoryIssueInputs {
  /// Composes existing metadata readers with the installed Context/input owner; no writer is required.
  pub fn new(
    issues: Arc<dyn IssueStore>,
    associations: Arc<dyn IssueExecutionStore>,
    inputs: Arc<dyn FactoryFlowInputPreparer>,
  ) -> Self {
    Self {
      issues,
      associations,
      inputs,
    }
  }
}
#[async_trait::async_trait]
impl FactoryFlowInputPreparer for FactoryIssueInputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let flow = snapshot
      .flow
      .runs
      .iter()
      .find(|flow| flow.id() == target.flow_run_id)
      .ok_or_else(invalid)?;
    let node = snapshot
      .admitted_flow
      .closure()
      .definition(flow.definition())
      .and_then(|definition| definition.node(&target.node))
      .ok_or_else(invalid)?;
    let fills_fields = node.outcomes().iter().any(|outcome| outcome.issue_fields().is_some());
    // Pure control steps have no Context to attribute; Incoming provenance already has its exact origin.
    if !fills_fields && node.context_binding().is_none() && node.build_bindings().next().is_none() {
      return self.inputs.prepare(snapshot, target).await;
    }
    let association = self
      .associations
      .issue_work_association(snapshot.work.subject().project_id(), snapshot.work.id())
      .await?;
    let Some(association) = association else {
      return if fills_fields {
        Err(ApplicationError::capability_unavailable())
      } else {
        self.inputs.prepare(snapshot, target).await
      };
    };
    if association.run_id != snapshot.run.id() || association.requirements_version.is_none() {
      return Err(invalid());
    }
    // Read the origin before the underlying owner awaits protected Context bytes. A concurrent
    // material change cannot become an apparently new, dependency-free source revision.
    let issue = self
      .issues
      .issue(association.project_id, association.issue_id, None)
      .await?;
    let captured = FlowIssueFieldBaseline {
      issue_id: issue.id,
      version: issue.version,
    };
    let input = self.inputs.prepare(snapshot, target).await?;
    let retained = snapshot.flow.data.inputs.iter().find(|row| {
      row.definition() == input.definition()
        && row.flow_run_id() == input.flow_run_id()
        && row.cycle_id() == input.cycle_id()
        && row.node() == input.node()
        && row.generation() == input.generation()
    });
    let origin = retained
      .and_then(FlowNodeInput::issue_field_baseline)
      .or_else(|| input.issue_field_baseline())
      .cloned()
      .unwrap_or(captured);
    if origin.issue_id != association.issue_id {
      return Err(invalid());
    }
    input.with_issue_field_baseline(origin).map_err(|_| invalid())
  }
}
