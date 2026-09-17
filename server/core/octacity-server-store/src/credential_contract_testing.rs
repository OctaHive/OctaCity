//! Reusable behavioral contract for Agent-credential store adapters.

use std::sync::Arc;

use octacity_server_domain::{AgentId, AgentName, EntityKind, PoolId, PoolVersion};
use serde_json::json;

use crate::test_support::{id, run_ready, time};
use crate::testing::{InMemoryStore, MutationEvidenceCounts, MutationEvidenceProbe};
use crate::{
  AgentCredentialStore, AgentCredentialTarget, AgentPlatform, AgentRegistrationProof, AuthenticateAgentRegistration,
  ExpectedAgentPlatform, IssueAgentEnrollment, MutationDisposition, RegisterAgent, RegistrationEpoch,
  RevokeAgentCredential, StoreError, StoreOperation,
};

/// Deterministic identities and requests used by the reusable credential contract.
#[derive(Clone)]
pub struct AgentCredentialStoreContractFixture {
  /// Enrollment credential issued before initial registration.
  pub enrollment: IssueAgentEnrollment,
  /// Stable Agent identity created by enrollment.
  pub agent_id: AgentId,
}

/// Builds deterministic prerequisite data for an Agent credential contract run.
#[must_use]
pub fn agent_credential_store_contract_fixture() -> AgentCredentialStoreContractFixture {
  AgentCredentialStoreContractFixture {
    enrollment: IssueAgentEnrollment::new(
      id(1),
      crate::CredentialSecret::from_bytes([0x11; 32]),
      id(2),
      PoolVersion::INITIAL,
      ExpectedAgentPlatform::Exact(platform()),
      time(100),
      time(1_000),
    )
    .unwrap(),
    agent_id: id(3),
  }
}

/// Runs the backend-neutral enrollment, replay, rotation, and supersession contract.
pub async fn verify_agent_credential_store_contract<S, P>(store: Arc<S>, evidence: Arc<P>)
where
  S: AgentCredentialStore + 'static,
  P: MutationEvidenceProbe + 'static,
{
  let fixture = agent_credential_store_contract_fixture();
  let enrollment = fixture.enrollment;
  let mut unknown_pool = enrollment.clone();
  unknown_pool.pool_id = id::<PoolId>(999);
  assert_eq!(
    store.issue_agent_enrollment(unknown_pool).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Pool
    },
    "an adapter must reject enrollment for an unknown immutable Pool version"
  );
  let mut invalid_enrollment = enrollment.clone();
  invalid_enrollment.expires_at = invalid_enrollment.issued_at;
  assert_eq!(
    store.issue_agent_enrollment(invalid_enrollment).await.unwrap_err(),
    StoreError::InvalidInput {
      operation: StoreOperation::IssueAgentEnrollment,
      source: crate::StoreInputError::InvalidCredentialWindow,
    },
    "adapters must revalidate mutable requests before consuming their identity"
  );
  assert_eq!(
    store
      .issue_agent_enrollment(enrollment.clone())
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Applied
  );
  assert_eq!(
    {
      let mut replay = enrollment.clone();
      replay.issued_at = time(101);
      store.issue_agent_enrollment(replay).await.unwrap().disposition
    },
    MutationDisposition::Replayed,
    "an enrollment replay must ignore a newly observed server issue time"
  );

  let first = registration(
    10,
    0x21,
    fixture.agent_id,
    AgentRegistrationProof::Enrollment {
      credential_id: enrollment.credential_id,
      credential: enrollment.credential.clone(),
    },
    200,
  );
  let registered = store.register_agent(first.clone()).await.unwrap();
  assert_eq!(registered.registration_epoch, RegistrationEpoch::new(1).unwrap());
  let mut first_replay = first;
  first_replay.registered_at = time(201);
  assert_eq!(
    store.register_agent(first_replay).await.unwrap().disposition,
    MutationDisposition::Replayed,
    "a registration replay must ignore a newly observed server registration time"
  );

  let consumed_replay = registration(
    11,
    0x22,
    fixture.agent_id,
    AgentRegistrationProof::Enrollment {
      credential_id: enrollment.credential_id,
      credential: enrollment.credential,
    },
    201,
  );
  assert_eq!(
    store.register_agent(consumed_replay).await.unwrap_err(),
    StoreError::CredentialRejected
  );

  let rotated = registration(
    12,
    0x23,
    fixture.agent_id,
    AgentRegistrationProof::Registration {
      credential_id: registered.credential_id,
      epoch: registered.registration_epoch,
      credential: crate::CredentialSecret::from_bytes([0x21; 32]),
    },
    300,
  );
  let current = store.register_agent(rotated).await.unwrap();
  assert_eq!(current.registration_epoch, RegistrationEpoch::new(2).unwrap());

  assert_eq!(
    store
      .authenticate_agent_registration(AuthenticateAgentRegistration {
        credential_id: registered.credential_id,
        credential: crate::CredentialSecret::from_bytes([0x21; 32]),
        authenticated_at: time(301),
      })
      .await
      .unwrap_err(),
    StoreError::CredentialRejected,
  );
  let authenticated = store
    .authenticate_agent_registration(AuthenticateAgentRegistration {
      credential_id: current.credential_id,
      credential: crate::CredentialSecret::from_bytes([0x23; 32]),
      authenticated_at: time(301),
    })
    .await
    .unwrap();
  assert_eq!(authenticated.registration_epoch, RegistrationEpoch::new(2).unwrap());
  assert_eq!(authenticated.pool_id, current.pool_id);

  assert_eq!(
    store
      .authenticate_agent_registration(AuthenticateAgentRegistration {
        credential_id: current.credential_id,
        credential: crate::CredentialSecret::from_bytes([0x23; 32]),
        authenticated_at: time(900),
      })
      .await
      .unwrap_err(),
    StoreError::CredentialRejected,
    "registration expiry is exclusive"
  );

  let platform_enrollment = enrollment_request(20, 0x31, ExpectedAgentPlatform::Exact(platform()), 1_000);
  store.issue_agent_enrollment(platform_enrollment.clone()).await.unwrap();
  let mut wrong_platform = registration(
    21,
    0x32,
    id(4),
    AgentRegistrationProof::Enrollment {
      credential_id: platform_enrollment.credential_id,
      credential: platform_enrollment.credential.clone(),
    },
    400,
  );
  wrong_platform.platform = AgentPlatform::new("windows", "amd64").unwrap();
  assert_eq!(
    store.register_agent(wrong_platform).await.unwrap_err(),
    StoreError::CredentialRejected
  );
  store
    .register_agent(registration(
      22,
      0x33,
      id(4),
      AgentRegistrationProof::Enrollment {
        credential_id: platform_enrollment.credential_id,
        credential: platform_enrollment.credential,
      },
      401,
    ))
    .await
    .unwrap();

  let expired = enrollment_request(30, 0x41, ExpectedAgentPlatform::Any, 150);
  store.issue_agent_enrollment(expired.clone()).await.unwrap();
  assert_eq!(
    store
      .register_agent(registration(
        31,
        0x42,
        id(5),
        AgentRegistrationProof::Enrollment {
          credential_id: expired.credential_id,
          credential: expired.credential,
        },
        200,
      ))
      .await
      .unwrap_err(),
    StoreError::CredentialRejected
  );

  let revoked = enrollment_request(40, 0x51, ExpectedAgentPlatform::Any, 1_000);
  store.issue_agent_enrollment(revoked.clone()).await.unwrap();
  let revoke_enrollment = RevokeAgentCredential {
    target: AgentCredentialTarget::Enrollment(revoked.credential_id),
    revoked_at: time(300),
  };
  assert_eq!(
    store.revoke_agent_credential(revoke_enrollment).await.unwrap(),
    MutationDisposition::Applied
  );
  assert_eq!(
    store.revoke_agent_credential(revoke_enrollment).await.unwrap(),
    MutationDisposition::Replayed
  );
  assert_eq!(
    store
      .register_agent(registration(
        41,
        0x52,
        id(6),
        AgentRegistrationProof::Enrollment {
          credential_id: revoked.credential_id,
          credential: revoked.credential,
        },
        301,
      ))
      .await
      .unwrap_err(),
    StoreError::CredentialRejected
  );

  let revoke_registration = RevokeAgentCredential {
    target: AgentCredentialTarget::Registration(current.credential_id),
    revoked_at: time(400),
  };
  assert_eq!(
    store.revoke_agent_credential(revoke_registration).await.unwrap(),
    MutationDisposition::Applied
  );
  assert_eq!(
    store.revoke_agent_credential(revoke_registration).await.unwrap(),
    MutationDisposition::Replayed
  );
  assert_eq!(
    store
      .authenticate_agent_registration(AuthenticateAgentRegistration {
        credential_id: current.credential_id,
        credential: crate::CredentialSecret::from_bytes([0x23; 32]),
        authenticated_at: time(401),
      })
      .await
      .unwrap_err(),
    StoreError::CredentialRejected
  );

  let recovery = enrollment_request(50, 0x61, ExpectedAgentPlatform::Exact(platform()), 1_000);
  store.issue_agent_enrollment(recovery.clone()).await.unwrap();
  let recovered = store
    .register_agent(registration(
      51,
      0x62,
      fixture.agent_id,
      AgentRegistrationProof::Enrollment {
        credential_id: recovery.credential_id,
        credential: recovery.credential,
      },
      500,
    ))
    .await
    .unwrap();
  assert_eq!(
    recovered.registration_epoch,
    RegistrationEpoch::new(3).unwrap(),
    "a fresh operator-issued enrollment credential must recover an Agent whose registration can no longer rotate"
  );
  assert_eq!(
    evidence.mutation_evidence_counts().await,
    MutationEvidenceCounts {
      idempotency: 11,
      audit: 11,
      outbox: 11,
    },
    "every accepted credential mutation must atomically persist its evidence"
  );
}

/// Runs the credential contract against a new in-memory adapter without a runtime.
pub fn verify_in_memory_agent_credential_contract() {
  let store = Arc::new(InMemoryStore::new());
  let fixture = agent_credential_store_contract_fixture();
  store
    .seed_agent_pool(fixture.enrollment.pool_id, fixture.enrollment.pool_version)
    .unwrap();
  run_ready(
    verify_agent_credential_store_contract(store.clone(), store),
    "the in-memory credential adapter unexpectedly yielded to an external runtime",
  );
}

fn registration(
  credential_id: u64,
  secret: u8,
  agent_id: AgentId,
  proof: AgentRegistrationProof,
  registered_at: i64,
) -> RegisterAgent {
  RegisterAgent::new(
    id(credential_id),
    crate::CredentialSecret::from_bytes([secret; 32]),
    agent_id,
    AgentName::new("contract-agent").unwrap(),
    proof,
    platform(),
    json!({"host_platform": "linux-amd64"}),
    time(registered_at),
    time(900),
  )
  .unwrap()
}

fn enrollment_request(
  credential_id: u64,
  secret: u8,
  expected_platform: ExpectedAgentPlatform,
  expires_at: i64,
) -> IssueAgentEnrollment {
  IssueAgentEnrollment::new(
    id(credential_id),
    crate::CredentialSecret::from_bytes([secret; 32]),
    id(2),
    PoolVersion::INITIAL,
    expected_platform,
    time(100),
    time(expires_at),
  )
  .unwrap()
}

fn platform() -> AgentPlatform {
  AgentPlatform::new("linux", "amd64").unwrap()
}
