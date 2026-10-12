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
/// Maximum UTF-8 bytes in a retained operator reason, shared with the core contract.
pub const MAX_FACTORY_OPERATOR_REASON_BYTES: usize = octacity_server_factory::MAX_FACTORY_TEXT_BYTES;

/// Schema-free transport intent; the server selects the exact retained HumanGate
/// outcome contract. It never accepts a caller-authored schema or lifecycle fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmitFactoryNodeResponseCommand {
  /// Owning Run.
  pub run_id: FactoryRunId,
  /// Exact immutable pending attempt.
  pub node_attempt_id: octacity_server_factory::NodeAttemptId,
  /// One configured finite outcome.
  pub outcome: octacity_server_factory::FactoryKey,
  /// Bounded observations validated against that outcome's pinned schema.
  pub value: serde_json::Value,
  /// Exact aggregate version shown to the operator.
  pub expected_version: FactoryRunVersion,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted request time.
  pub requested_at: Timestamp,
}
impl Command for SubmitFactoryNodeResponseCommand {
  type Outcome = FactoryRunControlCommandOutcome;
}
impl ManagementAuthorizationTarget for SubmitFactoryNodeResponseCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ResolveFactoryNodeCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryNodeAttempt, self.node_attempt_id)
  }
}

/// Closed escalation, delivery or promotion confirmation for a pinned HumanGate.
/// A confirmation cannot supply an outcome key, candidate, Decision or success.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfirmFactoryNodeOperationCommand {
  /// Owning Run.
  pub run_id: FactoryRunId,
  /// Exact immutable operator gate attempt.
  pub node_attempt_id: octacity_server_factory::NodeAttemptId,
  /// Closed purpose matching the retained node binding.
  pub operation: octacity_server_factory::FlowHumanOperation,
  /// One bounded choice explicitly enabled by that binding.
  pub disposition: octacity_server_factory::FlowHumanDisposition,
  /// Bounded human reason retained in the private control journal.
  pub reason: octacity_server_factory::FactoryText,
  /// Exact aggregate version shown to the operator.
  pub expected_version: FactoryRunVersion,
  /// Stable scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted acceptance time.
  pub requested_at: Timestamp,
}
impl Command for ConfirmFactoryNodeOperationCommand {
  type Outcome = FactoryRunControlCommandOutcome;
}
impl ManagementAuthorizationTarget for ConfirmFactoryNodeOperationCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ResolveFactoryNodeCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryNodeAttempt, self.node_attempt_id)
  }
}

#[async_trait]
impl<S: FactoryRunControlStore + octacity_server_store::FactoryRunStore + 'static>
  crate::ManagementCommandUseCase<ConfirmFactoryNodeOperationCommand> for FactoryControlHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    command: ConfirmFactoryNodeOperationCommand,
  ) -> Result<FactoryRunControlCommandOutcome, Self::Error> {
    let snapshot = self.store.factory_run_snapshot(command.run_id).await?;
    let node = configured_node(&snapshot, command.node_attempt_id)?;
    let binding = node
      .operator_requests()
      .filter(|binding| binding.operation() == command.operation)
      .ok_or_else(ApplicationError::invalid)?;
    let outcome = binding
      .outcome(command.disposition)
      .cloned()
      .ok_or_else(ApplicationError::invalid)?;
    let schema = node
      .outcome(&outcome)
      .and_then(|outcome| snapshot.admitted_flow.data_schema(outcome.schema()))
      .ok_or_else(ApplicationError::invalid)?;
    let value = binding
      .response(command.disposition, &command.reason)
      .map_err(|_| ApplicationError::invalid())?;
    let payload = octacity_server_factory::FlowPayload::new(schema, value).map_err(|_| ApplicationError::invalid())?;
    self
      .execute_management_command(
        context,
        grant,
        ResolveFactoryNodeCommand {
          run_id: command.run_id,
          node_attempt_id: command.node_attempt_id,
          outcome,
          payload,
          expected_version: command.expected_version,
          idempotency_key: command.idempotency_key,
          requested_at: command.requested_at,
        },
      )
      .await
  }
}

fn configured_node(
  snapshot: &octacity_server_store::FactoryRunSnapshot,
  attempt: octacity_server_factory::NodeAttemptId,
) -> Result<&octacity_server_factory::FlowNodeDefinition, ApplicationError> {
  let attempt = snapshot
    .flow
    .attempts
    .iter()
    .find(|node| node.id() == attempt)
    .ok_or_else(ApplicationError::invalid)?;
  snapshot
    .flow
    .runs
    .iter()
    .find(|flow| flow.id() == attempt.flow_run_id())
    .and_then(|flow| snapshot.admitted_flow.closure().definition(flow.definition()))
    .and_then(|definition| definition.node(attempt.node_key()))
    .ok_or_else(ApplicationError::invalid)
}

#[async_trait]
impl<S: FactoryRunControlStore + octacity_server_store::FactoryRunStore + 'static>
  crate::ManagementCommandUseCase<SubmitFactoryNodeResponseCommand> for FactoryControlHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    command: SubmitFactoryNodeResponseCommand,
  ) -> Result<FactoryRunControlCommandOutcome, Self::Error> {
    let snapshot = self.store.factory_run_snapshot(command.run_id).await?;
    let node = configured_node(&snapshot, command.node_attempt_id)?;
    if node.operator_requests().is_some() {
      return Err(ApplicationError::invalid());
    }
    let schema = node
      .outcome(&command.outcome)
      .and_then(|outcome| snapshot.admitted_flow.data_schema(outcome.schema()))
      .ok_or_else(ApplicationError::invalid)?;
    let payload =
      octacity_server_factory::FlowPayload::new(schema, command.value).map_err(|_| ApplicationError::invalid())?;
    // Only immutable declaration lookup precedes the atomic replay check. Active
    // state, cycle and deadline eligibility remain under the store's Run lock.
    self
      .execute_management_command(
        context,
        grant,
        ResolveFactoryNodeCommand {
          run_id: command.run_id,
          node_attempt_id: command.node_attempt_id,
          outcome: command.outcome,
          payload,
          expected_version: command.expected_version,
          idempotency_key: command.idempotency_key,
          requested_at: command.requested_at,
        },
      )
      .await
  }
}

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
    ManagementAuthorizationMapping::instance(ManagementAction::Execute, ManagementResourceKind::FactoryNodeAttempt);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryNodeAttempt, self.node_attempt_id)
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
    ManagementAuthorizationMapping::instance(ManagementAction::Retry, ManagementResourceKind::FactoryNodeAttempt);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryNodeAttempt, self.node_attempt_id)
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
