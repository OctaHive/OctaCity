use std::{
  future::Future,
  sync::Arc,
  task::{Context, Poll, Waker},
  time::Duration,
};

use octacity_protocol::{AgentCredentialToken, RegisterAgentRequest};
use octacity_server_application::{
  AgentEnrollmentHandler, AgentEnrollmentSecretKey, AgentOperation, AgentRegistrationError, AgentRegistrationInput,
  AgentRegistrationService, AgentRegistrationUseCases, AuthorizeAgentInput, CommandHandler, ManagementInputFactory,
  MutationDisposition,
};
use octacity_server_domain::{EnrollmentCredentialId, PoolId, PoolVersion, Timestamp};
use octacity_server_store::{
  AgentCredentialStore, CredentialSecret, ExpectedAgentPlatform, IssueAgentEnrollment, testing::InMemoryStore,
};
use uuid::Uuid;

#[test]
fn management_enrollment_is_replay_safe_and_returns_the_unpersisted_secret() {
  run_ready(async {
    let store = Arc::new(InMemoryStore::new());
    let pool_id = id::<PoolId>(40);
    store.seed_agent_pool(pool_id, PoolVersion::INITIAL).unwrap();
    let factory = ManagementInputFactory::new(
      ["native".to_owned()],
      Duration::from_secs(900),
      AgentEnrollmentSecretKey::new([0x2a; 32]),
    )
    .unwrap();
    let command = factory
      .issue_agent_enrollment(
        &pool_id.to_string(),
        1,
        Some("linux".to_owned()),
        Some("amd64".to_owned()),
        "enroll-native-one",
        1_000,
      )
      .unwrap();
    let expected_token = command.credential.encode().to_string();
    let replay_command = factory
      .issue_agent_enrollment(
        &pool_id.to_string(),
        1,
        Some("linux".to_owned()),
        Some("amd64".to_owned()),
        "enroll-native-one",
        2_000,
      )
      .unwrap();
    let handler = AgentEnrollmentHandler::new(store);
    let applied = handler.handle_command(command).await.unwrap();
    let replayed = handler.handle_command(replay_command).await.unwrap();

    assert_eq!(applied.disposition, MutationDisposition::Applied);
    assert_eq!(replayed.disposition, MutationDisposition::Replayed);
    assert_eq!(applied.credential.encode().as_str(), expected_token);
    assert_eq!(replayed.credential.encode().as_str(), expected_token);
    assert_eq!(applied.expires_at.unix_millis(), 901_000);
  });
}

#[test]
fn restart_issues_a_fresh_epoch_and_fences_every_old_operation() {
  run_ready(async {
    let store = Arc::new(InMemoryStore::new());
    let pool_id = id::<PoolId>(1);
    store.seed_agent_pool(pool_id, PoolVersion::INITIAL).unwrap();
    let first_enrollment_id = id::<EnrollmentCredentialId>(2);
    issue_enrollment(&store, first_enrollment_id, [0x11; 32], pool_id, 100).await;
    let service = AgentRegistrationService::new(store, Duration::from_secs(60)).unwrap();

    let mut first_request = registration_request();
    first_request.request_id = "register-first".to_owned();
    let first_credential = enrollment_credential(first_enrollment_id);
    let first = service
      .register(AgentRegistrationInput {
        request: first_request.clone(),
        credential: first_credential.clone(),
        observed_at_unix_ms: 200,
      })
      .await
      .unwrap();
    let replay = service
      .register(AgentRegistrationInput {
        request: first_request,
        credential: first_credential,
        observed_at_unix_ms: 201,
      })
      .await
      .unwrap();
    assert_eq!(replay, first, "lost-response retry must not create another epoch");

    let mut second_request = registration_request();
    second_request.request_id = "register-second".to_owned();
    let restart_credential = registration_credential(&first.registration_id);
    let second = service
      .register(AgentRegistrationInput {
        request: second_request,
        credential: restart_credential,
        observed_at_unix_ms: 300,
      })
      .await
      .unwrap();
    assert_ne!(first.registration_id, second.registration_id);
    assert_eq!(first.registration_epoch.get(), 1);
    assert_eq!(second.registration_epoch.get(), 2);
    let stale_credential = registration_credential(&first.registration_id);
    let current_credential = registration_credential(&second.registration_id);

    for operation in [
      AgentOperation::Poll,
      AgentOperation::Heartbeat,
      AgentOperation::AppendEvents,
      AgentOperation::Upload,
      AgentOperation::Cache,
      AgentOperation::Complete,
    ] {
      let rejection = service
        .authorize(AuthorizeAgentInput {
          operation,
          registration_id: first.registration_id.clone(),
          credential: stale_credential.clone(),
          observed_at_unix_ms: 301,
        })
        .await
        .unwrap_err();
      assert!(matches!(rejection, AgentRegistrationError::Fenced), "{operation:?}");
    }

    let current = service
      .authorize(AuthorizeAgentInput {
        operation: AgentOperation::Poll,
        registration_id: second.registration_id,
        credential: current_credential,
        observed_at_unix_ms: 301,
      })
      .await
      .unwrap();
    assert_eq!(current.registration_epoch().get(), 2);
    assert_eq!(current.operation(), AgentOperation::Poll);
  });
}

fn registration_request() -> RegisterAgentRequest {
  serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/coordinator/register-request-v1.json"
  ))
  .unwrap()
}

async fn issue_enrollment(
  store: &InMemoryStore,
  credential_id: EnrollmentCredentialId,
  secret: [u8; 32],
  pool_id: PoolId,
  issued_at: i64,
) {
  store
    .issue_agent_enrollment(
      IssueAgentEnrollment::new(
        credential_id,
        CredentialSecret::from_bytes(secret),
        pool_id,
        PoolVersion::INITIAL,
        ExpectedAgentPlatform::Any,
        Timestamp::from_unix_millis(issued_at).unwrap(),
        Timestamp::from_unix_millis(10_000).unwrap(),
      )
      .unwrap(),
    )
    .await
    .unwrap();
}

fn enrollment_credential(credential_id: EnrollmentCredentialId) -> AgentCredentialToken {
  AgentCredentialToken::parse(&format!("enrollment.{credential_id}.{ENCODED_SECRET}")).unwrap()
}

fn registration_credential(credential_id: &str) -> AgentCredentialToken {
  AgentCredentialToken::parse(&format!("registration.{credential_id}.{ENCODED_SECRET}")).unwrap()
}

const ENCODED_SECRET: &str = "ERERERERERERERERERERERERERERERERERERERERERE";

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory registration test unexpectedly waited for external I/O"),
  }
}

fn id<T>(value: u128) -> T
where
  T: TryFromUuid,
{
  T::from_uuid(Uuid::from_u128(value)).unwrap()
}

trait TryFromUuid: Sized {
  fn from_uuid(value: Uuid) -> Result<Self, octacity_server_domain::DomainValueError>;
}

impl TryFromUuid for PoolId {
  fn from_uuid(value: Uuid) -> Result<Self, octacity_server_domain::DomainValueError> {
    Self::from_uuid(value)
  }
}

impl TryFromUuid for EnrollmentCredentialId {
  fn from_uuid(value: Uuid) -> Result<Self, octacity_server_domain::DomainValueError> {
    Self::from_uuid(value)
  }
}
