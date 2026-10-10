//! Plugin extensions of configured trusted-action nodes over the common node journal.

use crate::ApplicationError;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use std::sync::Arc;

/// Scoped plugin input with no parent transcript, credentials or routing authority.
pub struct FactoryFlowActionRequest<'a> {
  /// Stable identity; plugins must deduplicate effects across retries and restart.
  pub operation_id: FactoryDigest,
  /// Internal Work identity.
  pub work_id: WorkEnvelopeId,
  /// Provider-neutral source identity used by the selected connector.
  pub external_identity: &'a ExternalWorkIdentity,
  /// Exact admitted source subject.
  pub subject: &'a ExactSubject,
  /// Exact action and operator-published parameters.
  pub binding: &'a FlowActionBinding,
  /// Exact explicitly projected input; plugins receive no other node data.
  pub input: &'a FlowNodeInput,
  /// Published contracts for the finite declared outputs.
  pub schemas: &'a [FlowDataSchema],
  /// Maximum authority; the plugin must enforce it before any effect.
  pub permissions: &'a FactoryPermissionSet,
  /// Hard resource ceiling; the plugin must enforce it during execution.
  pub budget: BudgetLimit,
  /// Authoritative stop deadline, never extended by a retry.
  pub deadline: Timestamp,
}

/// Trusted plugin execution facts; the Flow validates the declared outcome again.
pub struct FactoryFlowActionObservation {
  /// One outcome declared by the immutable node.
  pub outcome: FactoryKey,
  /// Bounded result including the plugin's idempotent receipt, under a published contract.
  pub payload: FlowPayload,
  /// Measured resources; attempt creation is charged by the Flow owner.
  pub usage: BudgetUsage,
}

/// Server-installed extension of `trusted_action`, never a new primitive kind.
#[async_trait]
pub trait FactoryFlowActionPlugin: Send + Sync {
  /// Returns the exact installed plugin version and content identity.
  fn identity(&self) -> ImmutableReference;
  /// Validates action parameters and every possible result before external effects.
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError>;
  /// Executes idempotently within the supplied permissions, budget and deadline.
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError>;
}

/// Validates a generic attempt and invokes its exact installed action plugin.
pub struct FactoryFlowActionAdapter {
  plugin: Arc<dyn FactoryFlowActionPlugin>,
}
impl FactoryFlowActionAdapter {
  /// Selects an installed plugin; definition binding and attempt identity are checked on every call.
  #[must_use]
  pub fn new(plugin: Arc<dyn FactoryFlowActionPlugin>) -> Self {
    Self { plugin }
  }
}

#[async_trait]
impl crate::FactoryNodeExecutor for FactoryFlowActionAdapter {
  async fn observe(
    &self,
    request: crate::FactoryNodeExecutionRequest<'_>,
  ) -> Result<crate::FactoryNodeExecutionStep, ApplicationError> {
    let definition = request
      .snapshot
      .admitted_flow
      .closure()
      .definition(request.input.definition())
      .ok_or_else(ApplicationError::invalid)?;
    let node = definition
      .node(request.attempt.node_key())
      .ok_or_else(ApplicationError::invalid)?;
    let binding = node.action().ok_or_else(ApplicationError::invalid)?;
    if request.input.factory_run_id() != request.snapshot.run.id()
      || request.input.node() != request.attempt.node_key()
      || request.input.digest().map_err(|_| ApplicationError::invalid())? != request.attempt.input_digest()
      || node.kind() != FlowNodeKind::TrustedAction
      || request.attempt.node_kind() != node.kind()
      || request.attempt.budget() != node.budget()
      || request.observed_at >= request.attempt.deadline()
      || binding.plugin() != &self.plugin.identity()
      || !node.permissions().plugins().any(|allowed| allowed == binding.plugin())
      || request.attempt.execution() != &NodeExecutionIdentity::External(binding.plugin().clone())
    {
      return Err(ApplicationError::invalid());
    }
    request
      .attempt
      .verify_observer(&request.ownership, request.observed_at)
      .map_err(|_| ApplicationError::invalid())?;
    self.plugin.validate(binding, node.outcomes())?;
    let bytes = serde_json::to_vec(&(
      request.snapshot.work.id(),
      request.snapshot.work.external_identity(),
      request.snapshot.work.subject(),
      binding,
      definition.reference(),
      request.attempt.input_digest(),
    ))
    .map_err(|_| ApplicationError::invalid())?;
    let operation_id = FactoryDigest::sha256(
      "octacity.factory.flow-action.v1",
      &[request.attempt.id().as_uuid().as_bytes(), &bytes],
    );
    let result = self
      .plugin
      .execute(FactoryFlowActionRequest {
        operation_id,
        work_id: request.snapshot.work.id(),
        external_identity: request.snapshot.work.external_identity(),
        subject: request.snapshot.work.subject(),
        binding,
        input: request.input,
        schemas: request.snapshot.admitted_flow.data_schemas(),
        permissions: node.permissions(),
        budget: node.budget(),
        deadline: request.attempt.deadline(),
      })
      .await?;
    if result.usage.attempts != 0 {
      return Err(ApplicationError::invalid());
    }
    let schema = request
      .snapshot
      .admitted_flow
      .data_schema(result.payload.schema())
      .ok_or_else(ApplicationError::invalid)?;
    FlowPayload::restore(
      &serde_json::to_vec(&result.payload).map_err(|_| ApplicationError::invalid())?,
      schema,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let record = FlowNodeRecord::new(
      request.input,
      request.attempt,
      definition,
      FlowRecordObservation {
        outcome: result.outcome,
        payload: result.payload,
        producer: request.attempt.execution().clone(),
        observed_at: request.observed_at,
      },
    )
    .map_err(|_| ApplicationError::invalid())?;
    Ok(crate::FactoryNodeExecutionStep::Completed {
      execution: None,
      record: Box::new(record),
      usage: result.usage,
    })
  }
}

/// Configurable, non-authoritative projection of a Work status into its source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryWorkStatusRequest {
  /// Idempotency key for one exact Flow action.
  pub operation_id: FactoryDigest,
  /// Internal Work identity.
  pub work_id: WorkEnvelopeId,
  /// Source identity understood only by the selected connector.
  pub external_identity: ExternalWorkIdentity,
  /// Exact admitted Project, Repository and revision.
  pub subject: ExactSubject,
  /// Operator-chosen status; no research-outcome vocabulary is imposed.
  pub status: FactorySafeText,
  /// Exact frozen authority; the connector checks hosts and credential profiles before effects.
  pub permissions: FactoryPermissionSet,
  /// Hard resource ceiling inherited from the action node.
  pub budget: BudgetLimit,
  /// Authoritative execution deadline.
  pub deadline: Timestamp,
}

/// Connector boundary: GitHub projects a status to a label; Jira to a native status.
///
/// Implementations deduplicate by operation identity, retain a receipt, enforce
/// the supplied permissions and execution bounds before protected operations,
/// and return its digest plus measured resource usage. Unknown or unenforceable
/// authority fails closed; credentials come only from allowed profiles.
/// Projection failure cannot undo an accepted node result or grant authority.
#[async_trait]
pub trait FactoryWorkStatusProjector: Send + Sync {
  /// Projects the arbitrary configured value using trusted connector credentials.
  async fn set_status(
    &self,
    request: FactoryWorkStatusRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError>;
}

/// One reusable action plugin; additional actions implement the same generic port.
pub struct FactoryWorkStatusAction<P> {
  identity: ImmutableReference,
  projector: Arc<P>,
  outcome: FlowOutcomeDefinition,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusParameters {
  status: FactorySafeText,
}
impl<P> FactoryWorkStatusAction<P> {
  /// Binds an exact installed version, source connector and declared successful result.
  #[must_use]
  pub fn new(identity: ImmutableReference, projector: Arc<P>, outcome: FlowOutcomeDefinition) -> Self {
    Self {
      identity,
      projector,
      outcome,
    }
  }
}
#[async_trait]
impl<P: FactoryWorkStatusProjector> FactoryFlowActionPlugin for FactoryWorkStatusAction<P> {
  fn identity(&self) -> ImmutableReference {
    self.identity.clone()
  }
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    if binding.action().as_str() != "set_status" || !outcomes.contains(&self.outcome) {
      return Err(ApplicationError::invalid());
    }
    serde_json::from_value::<StatusParameters>(binding.parameters().clone())
      .map_err(|_| ApplicationError::invalid())?;
    Ok(())
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let parameters: StatusParameters =
      serde_json::from_value(request.binding.parameters().clone()).map_err(|_| ApplicationError::invalid())?;
    let (digest, usage) = self
      .projector
      .set_status(FactoryWorkStatusRequest {
        operation_id: request.operation_id,
        work_id: request.work_id,
        external_identity: request.external_identity.clone(),
        subject: request.subject.clone(),
        status: parameters.status,
        permissions: request.permissions.clone(),
        budget: request.budget,
        deadline: request.deadline,
      })
      .await?;
    Ok(FactoryFlowActionObservation {
      outcome: self.outcome.key().clone(),
      payload: FlowPayload::new(
        request
          .schemas
          .iter()
          .find(|schema| schema.reference() == self.outcome.schema())
          .ok_or_else(ApplicationError::invalid)?,
        serde_json::json!({"receipt":digest}),
      )
      .map_err(|_| ApplicationError::invalid())?,
      usage,
    })
  }
}
