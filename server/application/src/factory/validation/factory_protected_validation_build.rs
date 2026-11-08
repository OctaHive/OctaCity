//! Ordinary command execution with exact protected-input result verification.
use crate::factory::execution::factory_node_build::{build_error, factory_build_request};
use crate::{
  ApplicationError, FactoryNodeBuildAdapter, FactoryNodeBuildOutputSource, FactoryNodeBuildOutputs,
  FactoryNodeCancellationStep, FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor,
  FactoryProtectedValidationClassResult, FactoryProtectedValidationMaterialization, FactoryProtectedValidationReport,
  FactoryScopedValidationCall, OrdinaryBuildCancellationApplication,
};
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  BudgetUsage, DeterministicGateOutcome, EvidenceOutputKind, EvidenceRequirement, FactoryDigest, FlowBuildIntent,
  FlowNodeKind,
};
use octacity_server_store::{BuildQueryStore, FactoryScopedBuildCaller};
use serde::Deserialize;
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

/// Bounded verification status for the authorized Tester invocation.
/// Artifact references, test contents and runner logs stay with the Build.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FactoryScopedValidationVerdict {
  /// The runner has not produced a verified terminal result yet.
  Waiting,
  /// Execution stopped without a verified verdict.
  Stopped,
  /// Outcome derived from the exact independently verified report.
  Verified {
    /// Runner verdict across the complete declared classes.
    outcome: DeterministicGateOutcome,
    /// Class names and results without diagnostics or private references.
    classes: Vec<FactoryProtectedValidationClassResult>,
  },
}

impl From<FactoryScopedValidationVerdict> for octacity_protocol::ScopedValidationVerdict {
  fn from(verdict: FactoryScopedValidationVerdict) -> Self {
    use octacity_protocol::{ScopedValidationClass, ScopedValidationOutcome};
    let convert = |outcome| match outcome {
      DeterministicGateOutcome::Passed => ScopedValidationOutcome::Passed,
      DeterministicGateOutcome::Failed => ScopedValidationOutcome::Failed,
      DeterministicGateOutcome::Indeterminate => ScopedValidationOutcome::Indeterminate,
    };
    match verdict {
      FactoryScopedValidationVerdict::Waiting => Self::Waiting,
      FactoryScopedValidationVerdict::Stopped => Self::Stopped,
      FactoryScopedValidationVerdict::Verified { outcome, classes } => Self::Verified {
        outcome: convert(outcome),
        classes: classes
          .into_iter()
          .map(|result| ScopedValidationClass {
            class: result.class.as_str().to_owned(),
            outcome: convert(result.outcome),
          })
          .collect(),
      },
    }
  }
}
/// Installed command capability. Results are verified before common completion;
/// configured deterministic gates remain responsible for routing the outcome.
pub struct FactoryProtectedValidationAdapter<B, O, V> {
  builds: Arc<B>,
  outputs: Arc<O>,
  inputs: Arc<V>,
  report: EvidenceRequirement,
  facts: EvidenceRequirement,
  #[cfg(test)]
  scoped_launch: bool,
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
      #[cfg(test)]
      scoped_launch: false,
    }
  }

  // Compare both observers over the same stored test intent. Production mode
  // always comes from the exact admitted definition.
  #[cfg(test)]
  pub(crate) fn with_scoped_launch(mut self) -> Self {
    self.scoped_launch = true;
    self
  }

  fn requires_scoped_launch(&self, request: &FactoryNodeExecutionRequest<'_>) -> bool {
    #[cfg(test)]
    if self.scoped_launch {
      return true;
    }
    request
      .admitted
      .closure()
      .definition(request.input.definition())
      .is_some_and(|definition| definition.scoped_validation_caller(request.input.node()).is_some())
  }

  fn validation_intent<'a>(
    &self,
    request: &FactoryNodeExecutionRequest<'a>,
  ) -> Result<&'a FlowBuildIntent, ApplicationError> {
    let intent = request.build.ok_or_else(ApplicationError::invalid)?;
    if intent.node() != request.attempt || intent.input() != request.input {
      return Err(ApplicationError::invalid());
    }
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
    Ok(intent)
  }
}
impl<B, O, V> FactoryProtectedValidationAdapter<B, O, V>
where
  B: OrdinaryBuildCancellationApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
  V: FactoryProtectedValidationInputSource,
{
  /// Starts the exact retained runner operation after the caller has resolved
  /// the authorized Tester lease. Replay uses ordinary Build acceptance.
  pub async fn launch_scoped(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    caller: FactoryScopedBuildCaller,
  ) -> Result<(), ApplicationError> {
    if !self.requires_scoped_launch(&request) {
      return Err(ApplicationError::invalid());
    }
    let intent = self.validation_intent(&request)?;
    intent
      .verify_admission(request.admitted)
      .map_err(|_| ApplicationError::invalid())?;
    intent
      .node()
      .verify_observer(&request.ownership, request.observed_at)
      .map_err(|_| ApplicationError::invalid())?;
    let build = factory_build_request(intent, request.ownership)?;
    if self
      .builds
      .factory_build_for_operation(build.operation_id)
      .await
      .map_err(build_error)?
      .is_none()
    {
      if request.observed_at >= intent.node().deadline() {
        return Err(ApplicationError::invalid());
      }
      self
        .builds
        .create_scoped_factory_build(build, caller)
        .await
        .map_err(build_error)?;
    }
    Ok(())
  }

  /// Observes an externally launched runner without dispatching it. The caller
  /// must resolve the request from an authorized Tester lease and frozen Flow.
  pub async fn scoped_verdict(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryScopedValidationVerdict, ApplicationError> {
    if !self.requires_scoped_launch(&request) {
      return Err(ApplicationError::invalid());
    }
    use FactoryScopedValidationVerdict::*;
    Ok(match self.observe(request).await? {
      FactoryNodeExecutionStep::Waiting { .. } | FactoryNodeExecutionStep::AwaitingTerminalUsage { .. } => Waiting,
      FactoryNodeExecutionStep::ExecutionStopped { .. } => Stopped,
      FactoryNodeExecutionStep::Completed { record, .. } => {
        let value = if record
          .primary_build_output()
          .is_some_and(|output| &output.name == self.report.kind())
        {
          record.payload().value()
        } else {
          record
            .build_provenance()
            .and_then(|build| build.outputs().iter().find(|output| &output.name == self.report.kind()))
            .and_then(|output| output.data.as_ref())
            .ok_or_else(ApplicationError::invalid)?
            .payload()
            .value()
        };
        let report = FactoryProtectedValidationReport::deserialize(value).map_err(|_| ApplicationError::invalid())?;
        Verified {
          outcome: report.verdict(),
          classes: report.classes,
        }
      }
    })
  }
}
#[async_trait]
impl<B, O, V> FactoryNodeExecutor for FactoryProtectedValidationAdapter<B, O, V>
where
  B: OrdinaryBuildCancellationApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
  V: FactoryProtectedValidationInputSource,
{
  async fn scoped_validation(
    &self,
    action: FactoryScopedValidationCall,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryScopedValidationVerdict, ApplicationError> {
    match action {
      FactoryScopedValidationCall::Start(caller) => {
        self.launch_scoped(request, caller).await?;
        Ok(FactoryScopedValidationVerdict::Waiting)
      }
      FactoryScopedValidationCall::Status => self.scoped_verdict(request).await,
    }
  }

  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let intent = self.validation_intent(&request)?;
    let accepted = if self.requires_scoped_launch(&request) {
      intent
        .verify_admission(request.admitted)
        .map_err(|_| ApplicationError::invalid())?;
      intent
        .node()
        .verify_observer(&request.ownership, request.observed_at)
        .map_err(|_| ApplicationError::invalid())?;
      let Some(accepted) = self
        .builds
        .factory_build_for_operation(intent.operation_id().map_err(|_| ApplicationError::invalid())?)
        .await
        .map_err(build_error)?
      else {
        return Ok(if request.observed_at < intent.node().deadline() {
          FactoryNodeExecutionStep::Waiting { execution: None }
        } else {
          FactoryNodeExecutionStep::ExecutionStopped {
            execution: None,
            usage: BudgetUsage::default(),
          }
        });
      };
      Some(accepted)
    } else {
      None
    };
    let materialized = self.inputs.materialization(intent).await?;
    if materialized.input() != intent.input() {
      return Err(ApplicationError::invalid());
    }
    let at = request.observed_at;
    let outputs = Arc::new(
      crate::factory::validation::factory_verified_build_outputs::VerifiedNodeBuildOutputs {
        builds: self.builds.as_ref(),
        source: self.outputs.as_ref(),
        intent,
        verify: |_: &octacity_server_factory::FlowBuildExecution, outputs: &FactoryNodeBuildOutputs| {
          let selected = |requirement: &EvidenceRequirement| {
            crate::factory::execution::factory_node_build::selected_document(
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
      },
    );
    let adapter = FactoryNodeBuildAdapter::new(self.builds.clone(), outputs);
    let adapter = if let Some(accepted) = accepted {
      adapter.with_accepted_build(accepted)
    } else {
      adapter
    };
    FactoryNodeExecutor::observe(&adapter, request).await
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
