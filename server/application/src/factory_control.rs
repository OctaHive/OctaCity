use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{FactoryRunId, FactoryRunState, FactoryRunVersion};
use octacity_server_store::{
  ApplyFactoryRunControl, FactoryRunControlIntent, FactoryRunControlOutcome, FactoryRunControlStore, IdempotencyKey,
};

use crate::{
  ApplicationError, Command, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementResourceKind, ManagementResourceResult, MutationDisposition,
  management_security::{audited_mutation, instance_resource},
};

/// Requests durable cancellation of one non-terminal Factory Run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelFactoryRunCommand {
  /// Factory Run to cancel.
  pub run_id: FactoryRunId,
  /// Exact aggregate version shown to the operator.
  pub expected_version: FactoryRunVersion,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative request time.
  pub requested_at: Timestamp,
}

impl Command for CancelFactoryRunCommand {
  type Outcome = FactoryRunControlCommandOutcome;
}

impl ManagementAuthorizationTarget for CancelFactoryRunCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Cancel, ManagementResourceKind::FactoryRun);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryRun, self.run_id)
  }
}

/// Records an operator response to one exact configured HumanGate attempt.
/// It cannot select a Run state or manufacture execution facts for other primitives.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveFactoryNodeCommand {
  /// Run owning the pending node.
  pub run_id: FactoryRunId,
  /// Exact attempt shown to the operator.
  pub node_attempt_id: octacity_server_factory::NodeAttemptId,
  /// One outcome declared by this pinned HumanGate contract.
  pub outcome: octacity_server_factory::FactoryKey,
  /// Response validated against that outcome's exact schema.
  pub payload: octacity_server_factory::FlowPayload,
  /// Exact aggregate version shown to the operator.
  pub expected_version: FactoryRunVersion,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative request time, bounded by the original attempt deadline.
  pub requested_at: Timestamp,
}
impl Command for ResolveFactoryNodeCommand {
  type Outcome = FactoryRunControlCommandOutcome;
}
impl ManagementAuthorizationTarget for ResolveFactoryNodeCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Execute, ManagementResourceKind::FactoryRun);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryRun, self.run_id)
  }
}
#[async_trait]
impl<S: FactoryRunControlStore + 'static> crate::ManagementCommandUseCase<ResolveFactoryNodeCommand>
  for FactoryControlHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: ResolveFactoryNodeCommand,
  ) -> Result<FactoryRunControlCommandOutcome, Self::Error> {
    self
      .apply(
        context,
        ApplyFactoryRunControl {
          run_id: command.run_id,
          expected_version: command.expected_version,
          idempotency_key: command.idempotency_key,
          requested_at: command.requested_at,
          intent: FactoryRunControlIntent::ResolveNode {
            node_attempt_id: command.node_attempt_id,
            outcome: command.outcome,
            payload: command.payload,
          },
        },
      )
      .await
  }
}

/// Confirms a bounded retry of one exact independently failed configured node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryFactoryNodeCommand {
  /// Owning Run.
  pub run_id: FactoryRunId,
  /// Exact failed execution shown to the operator.
  pub node_attempt_id: octacity_server_factory::NodeAttemptId,
  /// Current aggregate precondition.
  pub expected_version: FactoryRunVersion,
  /// Scoped transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative acceptance time.
  pub requested_at: Timestamp,
}
impl Command for RetryFactoryNodeCommand {
  type Outcome = FactoryRunControlCommandOutcome;
}
impl ManagementAuthorizationTarget for RetryFactoryNodeCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Execute, ManagementResourceKind::FactoryRun);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryRun, self.run_id)
  }
}
#[async_trait]
impl<S: FactoryRunControlStore + 'static> crate::ManagementCommandUseCase<RetryFactoryNodeCommand>
  for FactoryControlHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: RetryFactoryNodeCommand,
  ) -> Result<FactoryRunControlCommandOutcome, Self::Error> {
    self
      .apply(
        context,
        ApplyFactoryRunControl {
          run_id: command.run_id,
          expected_version: command.expected_version,
          idempotency_key: command.idempotency_key,
          requested_at: command.requested_at,
          intent: FactoryRunControlIntent::RetryNode {
            node_attempt_id: command.node_attempt_id,
          },
        },
      )
      .await
  }
}

/// Safe result shared by bounded, graph-neutral Factory control commands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunControlCommandOutcome {
  /// Whether this invocation applied or replayed the original intent.
  pub disposition: MutationDisposition,
  /// Controlled Factory Run.
  pub run_id: FactoryRunId,
  /// Version produced by the original accepted intent.
  pub version: FactoryRunVersion,
  /// Code-owned state after the intent was recorded.
  pub state: FactoryRunState,
  /// Whether the reconciler must execute cancellation.
  pub cancellation_requested: bool,
}

/// Management handlers for the closed Factory Run command surface.
///
/// Logical approvals are responses to configured HumanGate attempts; execution
/// retry addresses an exact Node Attempt. Fixed-stage commands are not part of
/// the public management API:
///
/// ```compile_fail
/// use octacity_server_application::{
///   RetryFactoryStageCommand, ResolveFactoryEscalationCommand, RequestFactoryDeliveryCommand,
/// };
/// ```
pub struct FactoryControlHandlers<S> {
  store: Arc<S>,
}

impl<S> FactoryControlHandlers<S> {
  /// Creates handlers over one backend-neutral atomic control port.
  #[must_use]
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }

  async fn apply(
    &self,
    context: &crate::ManagementRequestContext,
    request: ApplyFactoryRunControl,
  ) -> Result<FactoryRunControlCommandOutcome, ApplicationError>
  where
    S: FactoryRunControlStore,
  {
    self
      .store
      .apply_factory_run_control(audited_mutation(context, request)?)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<CancelFactoryRunCommand> for FactoryControlHandlers<S>
where
  S: FactoryRunControlStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: CancelFactoryRunCommand,
  ) -> Result<FactoryRunControlCommandOutcome, Self::Error> {
    self
      .apply(
        context,
        ApplyFactoryRunControl {
          run_id: command.run_id,
          expected_version: command.expected_version,
          idempotency_key: command.idempotency_key,
          intent: FactoryRunControlIntent::Cancel,
          requested_at: command.requested_at,
        },
      )
      .await
  }
}

impl From<FactoryRunControlOutcome> for FactoryRunControlCommandOutcome {
  fn from(value: FactoryRunControlOutcome) -> Self {
    Self {
      disposition: value.disposition.into(),
      run_id: value.run_id,
      version: value.version,
      state: value.state,
      cancellation_requested: value.cancellation_requested,
    }
  }
}
