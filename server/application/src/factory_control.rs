use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  ChangeSetId, DecisionId, EscalationId, FactoryEscalationDisposition, FactoryRunId, FactoryRunState,
  FactoryRunVersion, FactoryText, StageAttemptId,
};
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

/// Confirms retry of the current infrastructure-retryable Stage Attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryFactoryStageCommand {
  /// Factory Run owning the attempt.
  pub run_id: FactoryRunId,
  /// Current retryable Stage Attempt.
  pub stage_attempt_id: StageAttemptId,
  /// Exact aggregate version shown to the operator.
  pub expected_version: FactoryRunVersion,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative request time.
  pub requested_at: Timestamp,
}

impl Command for RetryFactoryStageCommand {
  type Outcome = FactoryRunControlCommandOutcome;
}

impl ManagementAuthorizationTarget for RetryFactoryStageCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Retry, ManagementResourceKind::FactoryRun);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryRun, self.run_id)
  }
}

/// Applies one bounded disposition to the current Factory escalation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveFactoryEscalationCommand {
  /// Factory Run waiting for disposition.
  pub run_id: FactoryRunId,
  /// Current escalation used as a stale-request precondition.
  pub escalation_id: EscalationId,
  /// Closed policy disposition rather than an arbitrary next state.
  pub disposition: FactoryEscalationDisposition,
  /// Bounded human reason retained with the immutable intent.
  pub reason: FactoryText,
  /// Exact aggregate version shown to the operator.
  pub expected_version: FactoryRunVersion,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative request time.
  pub requested_at: Timestamp,
}

impl Command for ResolveFactoryEscalationCommand {
  type Outcome = FactoryRunControlCommandOutcome;
}

impl ManagementAuthorizationTarget for ResolveFactoryEscalationCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::FactoryRun);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryRun, self.run_id)
  }
}

/// Requests delivery for review of the exact currently accepted candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestFactoryDeliveryCommand {
  /// Factory Run ready for delivery.
  pub run_id: FactoryRunId,
  /// Current candidate used as a stale-request precondition.
  pub candidate_id: ChangeSetId,
  /// Current accepting Decision used as a stale-request precondition.
  pub decision_id: DecisionId,
  /// Exact aggregate version shown to the operator.
  pub expected_version: FactoryRunVersion,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative request time.
  pub requested_at: Timestamp,
}

impl Command for RequestFactoryDeliveryCommand {
  type Outcome = FactoryRunControlCommandOutcome;
}

impl ManagementAuthorizationTarget for RequestFactoryDeliveryCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Execute, ManagementResourceKind::FactoryRun);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::FactoryRun, self.run_id)
  }
}

/// Safe result shared by the four bounded Factory control commands.
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

#[async_trait]
impl<S> crate::ManagementCommandUseCase<RetryFactoryStageCommand> for FactoryControlHandlers<S>
where
  S: FactoryRunControlStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: RetryFactoryStageCommand,
  ) -> Result<FactoryRunControlCommandOutcome, Self::Error> {
    self
      .apply(
        context,
        ApplyFactoryRunControl {
          run_id: command.run_id,
          expected_version: command.expected_version,
          idempotency_key: command.idempotency_key,
          intent: FactoryRunControlIntent::RetryInfrastructure {
            stage_attempt_id: command.stage_attempt_id,
          },
          requested_at: command.requested_at,
        },
      )
      .await
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<ResolveFactoryEscalationCommand> for FactoryControlHandlers<S>
where
  S: FactoryRunControlStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: ResolveFactoryEscalationCommand,
  ) -> Result<FactoryRunControlCommandOutcome, Self::Error> {
    self
      .apply(
        context,
        ApplyFactoryRunControl {
          run_id: command.run_id,
          expected_version: command.expected_version,
          idempotency_key: command.idempotency_key,
          intent: FactoryRunControlIntent::ResolveEscalation {
            escalation_id: command.escalation_id,
            disposition: command.disposition,
            reason: command.reason,
          },
          requested_at: command.requested_at,
        },
      )
      .await
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<RequestFactoryDeliveryCommand> for FactoryControlHandlers<S>
where
  S: FactoryRunControlStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: RequestFactoryDeliveryCommand,
  ) -> Result<FactoryRunControlCommandOutcome, Self::Error> {
    self
      .apply(
        context,
        ApplyFactoryRunControl {
          run_id: command.run_id,
          expected_version: command.expected_version,
          idempotency_key: command.idempotency_key,
          intent: FactoryRunControlIntent::RequestDelivery {
            candidate_id: command.candidate_id,
            decision_id: command.decision_id,
          },
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
