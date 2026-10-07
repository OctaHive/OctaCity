use std::{
  future::Future,
  pin::pin,
  sync::{Arc, Mutex},
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_server_factory::{
  ChangeSetId, DecisionId, EscalationId, FactoryEscalationDisposition, FactoryRunId, FactoryRunState,
  FactoryRunVersion, FactoryText, StageAttemptId,
};
use octacity_server_store::{
  ApplyFactoryRunControl, FactoryRunControlIntent, FactoryRunControlOutcome, FactoryRunControlStore, IdempotencyKey,
  ManagementMutation, MutationDisposition as StoreMutationDisposition, StoreError,
};
use uuid::Uuid;

use crate::{
  CancelFactoryRunCommand, FactoryControlHandlers, ManagementAction, ManagementAuthorizationGrant,
  ManagementAuthorizationTarget, ManagementCommandUseCase, ManagementRequestContext, ManagementRequestId,
  ManagementVisibility, MutationDisposition, RequestFactoryDeliveryCommand, ResolveFactoryEscalationCommand,
  RetryFactoryStageCommand,
};

#[derive(Default)]
struct CapturingStore {
  requests: Mutex<Vec<ApplyFactoryRunControl>>,
}

#[async_trait]
impl FactoryRunControlStore for CapturingStore {
  async fn apply_factory_run_control(
    &self,
    request: ManagementMutation<ApplyFactoryRunControl>,
  ) -> Result<FactoryRunControlOutcome, StoreError> {
    let (request, _) = request.into_parts();
    let version = FactoryRunVersion::new(request.expected_version.get() + 1).unwrap();
    let state = match &request.intent {
      FactoryRunControlIntent::ResolveEscalation {
        disposition: FactoryEscalationDisposition::Acknowledge,
        ..
      } => FactoryRunState::Completed,
      _ => FactoryRunState::Implementing,
    };
    self.requests.lock().unwrap().push(request.clone());
    Ok(FactoryRunControlOutcome {
      disposition: StoreMutationDisposition::Applied,
      run_id: request.run_id,
      version,
      state,
      cancellation_requested: matches!(
        &request.intent,
        FactoryRunControlIntent::Cancel
          | FactoryRunControlIntent::ResolveEscalation {
            disposition: FactoryEscalationDisposition::Cancel,
            ..
          }
      ),
    })
  }
}

fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(
    ManagementRequestId::new(Uuid::from_u128(0x3b44_5783_32cb_46ed_8fd4_13d7_4e48_ab37)).unwrap(),
  )
}

fn grant() -> ManagementAuthorizationGrant {
  ManagementAuthorizationGrant::new(ManagementVisibility::all())
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

#[test]
fn commands_have_target_specific_authorization_and_only_closed_inputs() {
  assert_eq!(
    CancelFactoryRunCommand::AUTHORIZATION.action(),
    ManagementAction::Cancel
  );
  assert_eq!(
    RetryFactoryStageCommand::AUTHORIZATION.action(),
    ManagementAction::Retry
  );
  assert_eq!(
    ResolveFactoryEscalationCommand::AUTHORIZATION.action(),
    ManagementAction::Administer
  );
  assert_eq!(
    RequestFactoryDeliveryCommand::AUTHORIZATION.action(),
    ManagementAction::Execute
  );
}

#[test]
fn handlers_translate_only_typed_control_intents() {
  run_ready(async {
    let store = Arc::new(CapturingStore::default());
    let handlers = FactoryControlHandlers::new(Arc::clone(&store));
    let run_id = FactoryRunId::generate();
    let version = FactoryRunVersion::INITIAL;
    let requested_at = octacity_server_domain::Timestamp::from_unix_millis(10).unwrap();

    let cancelled = handlers
      .execute_management_command(
        &context(),
        &grant(),
        CancelFactoryRunCommand {
          run_id,
          expected_version: version,
          idempotency_key: key("cancel"),
          requested_at,
        },
      )
      .await
      .unwrap();
    assert_eq!(cancelled.disposition, MutationDisposition::Applied);
    assert!(cancelled.cancellation_requested);

    handlers
      .execute_management_command(
        &context(),
        &grant(),
        RetryFactoryStageCommand {
          run_id,
          stage_attempt_id: StageAttemptId::generate(),
          expected_version: version,
          idempotency_key: key("retry"),
          requested_at,
        },
      )
      .await
      .unwrap();
    handlers
      .execute_management_command(
        &context(),
        &grant(),
        ResolveFactoryEscalationCommand {
          run_id,
          escalation_id: EscalationId::generate(),
          disposition: FactoryEscalationDisposition::Acknowledge,
          reason: FactoryText::new("reviewed").unwrap(),
          expected_version: version,
          idempotency_key: key("resolve"),
          requested_at,
        },
      )
      .await
      .unwrap();
    handlers
      .execute_management_command(
        &context(),
        &grant(),
        RequestFactoryDeliveryCommand {
          run_id,
          candidate_id: ChangeSetId::generate(),
          decision_id: DecisionId::generate(),
          expected_version: version,
          idempotency_key: key("delivery"),
          requested_at,
        },
      )
      .await
      .unwrap();

    let requests = store.requests.lock().unwrap();
    assert!(matches!(requests[0].intent, FactoryRunControlIntent::Cancel));
    assert!(matches!(
      requests[1].intent,
      FactoryRunControlIntent::RetryInfrastructure { .. }
    ));
    assert!(matches!(
      requests[2].intent,
      FactoryRunControlIntent::ResolveEscalation { .. }
    ));
    assert!(matches!(
      requests[3].intent,
      FactoryRunControlIntent::RequestDelivery { .. }
    ));
  });
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let waker = Waker::noop();
  let mut context = Context::from_waker(waker);
  let mut future = pin!(future);
  match future.as_mut().poll(&mut context) {
    Poll::Ready(output) => output,
    Poll::Pending => panic!("in-memory Factory control future must be immediately ready"),
  }
}
