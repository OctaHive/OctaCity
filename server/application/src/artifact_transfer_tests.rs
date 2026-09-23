use std::{
  collections::BTreeMap,
  future::Future,
  sync::{Arc, Mutex},
  task::{Context, Poll, Waker},
  time::Duration,
};

use crate::{
  AgentArtifactError, AgentArtifactTransferUseCases, AgentOperation, AgentRegistrationError, AgentRegistrationInput,
  AgentRegistrationOutcome, AgentRegistrationUseCases, ArtifactHandlers, AuthorizeAgentInput,
  AuthorizeArtifactDownloadQuery, AuthorizedAgent, BeginAgentArtifactUploadInput, CompleteAgentArtifactUploadInput,
  GetArtifactQuery, ListBuildArtifactsQuery, QueryHandler,
};
use async_trait::async_trait;
use octacity_artifact_store::{
  ArtifactIntegrityError, ArtifactObject, ArtifactStore, ArtifactStoreError, DownloadAuthorization, UploadAuthorization,
};
use octacity_protocol::{
  AgentCredentialToken, BeginOutputUploadRequest, COORDINATOR_PROTOCOL_VERSION, CompleteOutputUploadRequest,
  LeaseFence as ProtocolLeaseFence, OutputKind, OutputUploadMetadata,
};
use octacity_server_domain::{AttemptNumber, JobId, LeaseId, Timestamp};
use octacity_server_store::{
  ArtifactRecordStore as _, LeaseAccess, LeaseFence, RegistrationEpoch,
  testing::{ArtifactLeaseFixture, InMemoryArtifactRecordStore},
};

struct RegistrationStub;

#[async_trait]
impl AgentRegistrationUseCases for RegistrationStub {
  async fn register(&self, _input: AgentRegistrationInput) -> Result<AgentRegistrationOutcome, AgentRegistrationError> {
    unreachable!("Artifact operations do not register Agents")
  }

  async fn authorize(&self, input: AuthorizeAgentInput) -> Result<AuthorizedAgent, AgentRegistrationError> {
    assert_eq!(input.operation, AgentOperation::Upload);
    Ok(AuthorizedAgent::for_test(
      id(6),
      RegistrationEpoch::new(1).unwrap(),
      id(7),
      octacity_protocol::HostCapacity {
        logical_cpu_count: 1,
        total_memory_bytes: 1,
        work_disk_total_bytes: 1,
        state_disk_total_bytes: 1,
        virtualization_available: false,
      },
      input.operation,
    ))
  }
}

struct ByteStoreStub {
  reject_completion: bool,
  authorized: Mutex<Vec<ArtifactObject>>,
}

#[async_trait]
impl ArtifactStore for ByteStoreStub {
  async fn authorize_upload(
    &self,
    object: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<UploadAuthorization, ArtifactStoreError> {
    self.authorized.lock().unwrap().push(object.clone());
    Ok(UploadAuthorization {
      url: "https://objects.example/upload?signature=secret".to_owned(),
      required_headers: BTreeMap::from([("x-checksum".to_owned(), "secret-value".to_owned())]),
      expires_in,
    })
  }

  async fn complete_upload(&self, _object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    if self.reject_completion {
      Err(ArtifactStoreError::Integrity {
        reason: ArtifactIntegrityError::DigestMismatch,
      })
    } else {
      Ok(())
    }
  }

  async fn authorize_download(
    &self,
    _object: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<DownloadAuthorization, ArtifactStoreError> {
    Ok(DownloadAuthorization {
      url: "https://objects.example/download?signature=secret".to_owned(),
      expires_in,
    })
  }

  async fn delete(&self, _object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    Ok(())
  }
}

#[test]
fn upload_service_replays_reservations_and_publishes_only_verified_bytes() {
  let (store, service) = service(false);
  let first = run_ready(service.begin_upload(begin_input("request-1"))).unwrap();
  let replay = run_ready(service.begin_upload(begin_input("request-2"))).unwrap();
  assert_eq!(first.upload_id, replay.upload_id);
  assert!(format!("{first:?}").contains("<redacted>"));

  let completed = run_ready(service.complete_upload(complete_input("complete-1", &first.upload_id))).unwrap();
  assert_eq!(completed.upload_id, first.upload_id);
  let replayed = run_ready(service.complete_upload(complete_input("complete-2", &first.upload_id))).unwrap();
  assert_eq!(replayed.upload_id, first.upload_id);
  let upload = run_ready(store.artifact_upload(first.upload_id.parse().unwrap())).unwrap();
  assert!(upload.artifact.state().is_visible());
  let artifact_id = upload.artifact.identity().artifact_id;
  let projection = run_ready(QueryHandler::handle_query(&service, GetArtifactQuery { artifact_id })).unwrap();
  assert_eq!(projection.name, "results.xml");
  assert_eq!(projection.media_type, "application/xml");
  let listed = run_ready(QueryHandler::handle_query(
    &service,
    ListBuildArtifactsQuery {
      build_id: id(2),
      limit: 10,
    },
  ))
  .unwrap();
  assert_eq!(listed, vec![projection.clone()]);
  let download = run_ready(QueryHandler::handle_query(
    &service,
    AuthorizeArtifactDownloadQuery {
      artifact_id,
      observed_at_unix_ms: 3_000,
    },
  ))
  .unwrap();
  assert_eq!(download.artifact, projection);
  let debug = format!("{download:?}");
  assert!(debug.contains("<redacted>"));
  assert!(!debug.contains("signature=secret"));
}

#[test]
fn integrity_failure_returns_the_logical_upload_to_unpublished_pending_state() {
  let (store, service) = service(true);
  let begun = run_ready(service.begin_upload(begin_input("request-1"))).unwrap();
  assert_eq!(
    run_ready(service.complete_upload(complete_input("complete-1", &begun.upload_id))),
    Err(AgentArtifactError::Integrity)
  );
  let upload = run_ready(store.artifact_upload(begun.upload_id.parse().unwrap())).unwrap();
  assert!(!upload.artifact.state().is_visible());
}

fn service(
  reject_completion: bool,
) -> (
  Arc<InMemoryArtifactRecordStore>,
  ArtifactHandlers<InMemoryArtifactRecordStore, ByteStoreStub>,
) {
  let access = lease_access();
  let store = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
    access,
    build_id: id(2),
    attempt_id: id(3),
    attempt: AttemptNumber::new(1).unwrap(),
    job_id: id(4),
    expires_at: time(10_000),
  }));
  let service = ArtifactHandlers::new(
    Arc::new(RegistrationStub),
    store.clone(),
    Arc::new(ByteStoreStub {
      reject_completion,
      authorized: Mutex::default(),
    }),
    Duration::from_secs(60),
    Duration::from_secs(30),
  )
  .unwrap();
  (store, service)
}

fn begin_input(request_id: &str) -> BeginAgentArtifactUploadInput {
  BeginAgentArtifactUploadInput {
    route_lease_id: id::<LeaseId>(5).to_string(),
    request: BeginOutputUploadRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.to_owned(),
      registration_id: "registration-1".to_owned(),
      lease: protocol_lease(),
      upload_key: "output-1".to_owned(),
      output: OutputUploadMetadata {
        run_id: 1,
        task_id: 1,
        kind: OutputKind::Report,
        name: "results.xml".to_owned(),
        content_type: None,
        report_format: Some("junit/custom-v2".to_owned()),
        transport_content_type: "application/xml".to_owned(),
        size_bytes: 42,
        sha256: "42".repeat(32),
      },
    },
    credential: credential(),
    observed_at_unix_ms: 1_000,
  }
}

fn complete_input(request_id: &str, upload_id: &str) -> CompleteAgentArtifactUploadInput {
  CompleteAgentArtifactUploadInput {
    route_lease_id: id::<LeaseId>(5).to_string(),
    request: CompleteOutputUploadRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.to_owned(),
      registration_id: "registration-1".to_owned(),
      lease: protocol_lease(),
      upload_id: upload_id.to_owned(),
    },
    credential: credential(),
    observed_at_unix_ms: 2_000,
  }
}

fn protocol_lease() -> ProtocolLeaseFence {
  ProtocolLeaseFence {
    lease_id: id::<LeaseId>(5).to_string(),
    job_id: id::<JobId>(4).to_string(),
    attempt: 1,
    fencing_token: "05".repeat(32),
  }
}

fn lease_access() -> LeaseAccess {
  LeaseAccess {
    lease_id: id(5),
    fence: LeaseFence::from_bytes([5; 32]),
    agent_id: id(6),
    registration_epoch: RegistrationEpoch::new(1).unwrap(),
  }
}

fn credential() -> AgentCredentialToken {
  AgentCredentialToken::parse(
    "registration.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
  )
  .unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

fn id<T>(value: u64) -> T
where
  T: std::str::FromStr,
  T::Err: std::fmt::Debug,
{
  format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory Artifact test unexpectedly awaited external I/O"),
  }
}
