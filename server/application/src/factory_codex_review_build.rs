//! Independent reviewer result discovery over qualified ordinary Build observations.
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{Assessment, FlowDiscoveryFacts, ImmutableReference};
use octacity_server_store::BuildQueryStore;
use std::sync::Arc;

/// Adds typed Assessment discovery after the installed output source has verified
/// the readonly Job, its protected invocation and evaluator provenance. The Node
/// owner retains discovery with the same immutable completion and usage.
pub struct FactoryCodexReviewBuildAdapter<B, O> {
  builds: Arc<B>,
  outputs: Arc<O>,
  reviews: Arc<dyn FactoryReviewInvocationSource>,
  evaluator: ImmutableReference,
}
impl<B, O> FactoryCodexReviewBuildAdapter<B, O> {
  /// Installs the same journal qualifier used at dispatch and retained observation.
  #[must_use]
  pub fn new(
    builds: Arc<B>,
    outputs: Arc<O>,
    reviews: Arc<dyn FactoryReviewInvocationSource>,
    evaluator: ImmutableReference,
  ) -> Self {
    Self {
      builds,
      outputs,
      reviews,
      evaluator,
    }
  }
}
#[async_trait]
impl<B: OrdinaryBuildCancellationApplication + BuildQueryStore, O: FactoryNodeBuildOutputSource> FactoryNodeExecutor
  for FactoryCodexReviewBuildAdapter<B, O>
{
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let intent = request.build.ok_or_else(invalid)?;
    let invocation = self
      .reviews
      .invocation(request.input, &self.evaluator, intent.node().started_at())
      .await?;
    let step = FactoryNodeExecutor::observe(
      &FactoryNodeBuildAdapter::new(self.builds.clone(), self.outputs.clone()),
      request,
    )
    .await?;
    match step {
      FactoryNodeExecutionStep::Completed {
        execution,
        record,
        usage,
        outbox,
      } => {
        let assessment: Assessment = serde_json::from_value(record.payload().value().clone()).map_err(|_| invalid())?;
        assessment
          .validate_bindings(invocation.plan(), invocation.evidence())
          .map_err(|_| invalid())?;
        let primary = record
          .build_provenance()
          .ok_or_else(invalid)?
          .outputs()
          .iter()
          .find(|output| output.name == intent.profile().result_output)
          .ok_or_else(invalid)?;
        if assessment.evaluator_reference() != &self.evaluator
          || assessment.provenance().model() != &intent.profile().model_or_tool
          || assessment.provenance().result() != &primary.artifact
        {
          return Err(invalid());
        }
        let record = (*record)
          .with_discovery(FlowDiscoveryFacts::assessment(&assessment).map_err(|_| invalid())?)
          .map_err(|_| invalid())?;
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
    FactoryNodeExecutor::cancel(
      &FactoryNodeBuildAdapter::new(self.builds.clone(), self.outputs.clone()),
      request,
      requested_at,
    )
    .await
  }
}
