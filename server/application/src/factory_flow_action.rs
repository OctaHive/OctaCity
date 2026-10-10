//! Plugin extensions of ordinary trusted-action nodes, independent of Research.

use crate::ApplicationError;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use std::sync::Arc;

/// Already-admitted, durably recorded execution selected by the owning Flow.
///
/// The caller validates retained Flow history, readiness and the current stored
/// claim, persists the Node Attempt before dispatch, and atomically commits the
/// returned completion. This adapter grants none of those authorities.
pub struct FactoryFlowActionContext<'a> {
  /// Owning Factory Run.
  pub run: &'a FactoryRun,
  /// Immutable admitted Work.
  pub work: &'a WorkEnvelope,
  /// Exact definition pinned by the Flow Run.
  pub definition: &'a FlowDefinition,
  /// Durable generic Flow Run.
  pub flow: &'a FlowRun,
  /// Durable generic attempt selected for execution.
  pub node: &'a NodeAttempt,
  /// Current fenced owner from the authoritative store.
  pub ownership: FactoryClaimOwnership,
  /// Server observation time.
  pub at: Timestamp,
}

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
  /// Digest of the explicitly projected predecessor input.
  pub input_digest: FactoryDigest,
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
  /// Exact declared result schema.
  pub output_schema: ImmutableReference,
  /// Digest of the plugin's retained, idempotent execution receipt.
  pub output_digest: FactoryDigest,
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

  /// Produces a fenced completion without selecting a successor or changing Work state.
  pub async fn execute(
    &self,
    context: FactoryFlowActionContext<'_>,
  ) -> Result<NodeAttemptCompletion, ApplicationError> {
    let node = context
      .definition
      .node(context.node.node_key())
      .ok_or_else(ApplicationError::invalid)?;
    let binding = node.action().ok_or_else(ApplicationError::invalid)?;
    if context.run.id() != context.flow.factory_run_id()
      || context.run.id() != context.node.factory_run_id()
      || context.run.work_id() != context.work.id()
      || context.run.subject() != context.work.subject()
      || context.node.flow_run_id() != context.flow.id()
      || context.flow.definition() != context.definition.reference()
      || node.kind() != FlowNodeKind::TrustedAction
      || context.node.node_kind() != node.kind()
      || context.node.budget() != node.budget()
      || context.at >= context.node.deadline()
      || binding.plugin() != &self.plugin.identity()
      || !node.permissions().plugins().any(|allowed| allowed == binding.plugin())
      || context.node.execution() != &NodeExecutionIdentity::External(binding.plugin().clone())
    {
      return Err(ApplicationError::invalid());
    }
    context
      .node
      .verify_observer(&context.ownership, context.at)
      .map_err(|_| ApplicationError::invalid())?;
    self.plugin.validate(binding, node.outcomes())?;
    let bytes = serde_json::to_vec(&(
      context.work.id(),
      context.work.external_identity(),
      context.work.subject(),
      binding,
      context.flow.definition(),
      context.node.input_digest(),
    ))
    .map_err(|_| ApplicationError::invalid())?;
    let operation_id = FactoryDigest::sha256(
      "octacity.factory.flow-action.v1",
      &[context.node.id().as_uuid().as_bytes(), &bytes],
    );
    let result = self
      .plugin
      .execute(FactoryFlowActionRequest {
        operation_id,
        work_id: context.work.id(),
        external_identity: context.work.external_identity(),
        subject: context.work.subject(),
        binding,
        input_digest: context.node.input_digest(),
        permissions: node.permissions(),
        budget: node.budget(),
        deadline: context.node.deadline(),
      })
      .await?;
    if result.usage.attempts != 0 {
      return Err(ApplicationError::invalid());
    }
    NodeAttemptCompletion::new(
      context.node,
      context.definition,
      NodeAttemptCompletionInput {
        outcome: result.outcome,
        output_schema: result.output_schema,
        output_digest: result.output_digest,
        ownership: context.ownership,
        usage: result.usage,
        observed_at: context.at,
      },
    )
    .map_err(|_| ApplicationError::invalid())
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
/// Projection failure cannot undo an accepted research result or grant authority.
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
      output_schema: self.outcome.schema().clone(),
      output_digest: digest,
      usage,
    })
  }
}
