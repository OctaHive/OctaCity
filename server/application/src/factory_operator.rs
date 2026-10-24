//! Management visibility guards around the existing atomic Factory control port.
use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_factory::FactoryRunId;
use octacity_server_store::{FactoryDiscoveryStore, FactoryRunControlStore};

use crate::{
  ApplicationError, AuthorizedQueryHandler, CancelFactoryRunCommand, Command, ConfirmFactoryNodeOperationCommand,
  FactoryControlHandlers, FactoryDiscoveryHandlers, FactoryRunControlCommandOutcome, GetFactoryRunQuery,
  ManagementAuthorizationGrant, ManagementAuthorizationPolicy, ManagementCommandUseCase, ManagementHandlerError,
  ManagementRequestContext, RetryFactoryNodeCommand, SubmitFactoryNodeResponseCommand,
};

/// Operator commands must observe their Run and owning Project before mutation.
/// The atomic store retains eligibility, preconditions, replay and audit authority.
pub struct FactoryOperatorHandlers<S> {
  controls: FactoryControlHandlers<S>,
  runs: AuthorizedQueryHandler<FactoryDiscoveryHandlers<S>>,
}

impl<S> FactoryOperatorHandlers<S> {
  /// Uses the same policy for target authorization and parent visibility.
  #[must_use]
  pub fn new(store: Arc<S>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self {
      controls: FactoryControlHandlers::new(store.clone()),
      runs: AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(FactoryDiscoveryHandlers::with_policy(store, policy)),
      ),
    }
  }
}

// Closed to this module: adding a control requires an explicit owning-Run mapping.
trait OperatorIntent: Command<Outcome = FactoryRunControlCommandOutcome> {
  fn owning_run(&self) -> FactoryRunId;
}
impl OperatorIntent for crate::SelectFactoryProfileCommand {
  fn owning_run(&self) -> FactoryRunId {
    self.run_id
  }
}
impl OperatorIntent for CancelFactoryRunCommand {
  fn owning_run(&self) -> FactoryRunId {
    self.run_id
  }
}
impl OperatorIntent for RetryFactoryNodeCommand {
  fn owning_run(&self) -> FactoryRunId {
    self.run_id
  }
}
impl OperatorIntent for SubmitFactoryNodeResponseCommand {
  fn owning_run(&self) -> FactoryRunId {
    self.run_id
  }
}
impl OperatorIntent for ConfirmFactoryNodeOperationCommand {
  fn owning_run(&self) -> FactoryRunId {
    self.run_id
  }
}

#[async_trait]
impl<S, C> ManagementCommandUseCase<C> for FactoryOperatorHandlers<S>
where
  S: FactoryDiscoveryStore + FactoryRunControlStore + 'static,
  C: OperatorIntent + 'static,
  FactoryControlHandlers<S>: ManagementCommandUseCase<C, Error = ApplicationError>,
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: C,
  ) -> Result<FactoryRunControlCommandOutcome, Self::Error> {
    self
      .runs
      .handle_query(
        context,
        GetFactoryRunQuery {
          run_id: command.owning_run(),
        },
      )
      .await
      .map_err(|error| match error {
        ManagementHandlerError::Application(error) => error,
        ManagementHandlerError::Forbidden(_) => ApplicationError::Store(octacity_server_store::StoreError::NotFound {
          entity: octacity_server_domain::EntityKind::FactoryRun,
        }),
      })?;
    self.controls.execute_management_command(context, grant, command).await
  }
}
