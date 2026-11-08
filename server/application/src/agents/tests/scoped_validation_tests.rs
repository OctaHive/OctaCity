use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use octacity_protocol::{
  AgentCredentialToken, COORDINATOR_PROTOCOL_VERSION, LeaseFence, ScopedValidationAction, ScopedValidationRequest,
};
use octacity_server_domain::{JobId, LeaseId, Timestamp};
use octacity_server_store::{
  JobClaim, JobClaimOutcome, JobExecutionStore, LeaseFence as StoreFence, LeaseWindow, TriggerAcceptanceStore,
  testing::{
    InMemoryStore, StoreContractFixture, authoritative_store_contract_fixture, compatible_inventory,
    compatible_snapshot,
  },
};

use crate::*;

struct OrdinaryBuild(octacity_server_store::AcceptTrigger);

#[async_trait]
impl octacity_server_store::BuildQueryStore for OrdinaryBuild {
  async fn job(&self, id: JobId) -> Result<octacity_server_store::JobRecord, octacity_server_store::StoreError> {
    Ok(octacity_server_store::JobRecord {
      attempt_id: self.0.attempt_id,
      job: self.0.jobs.iter().find(|job| job.id == id).unwrap().clone(),
      state: octacity_server_job::JobState::Leased,
      version: octacity_server_domain::JobVersion::INITIAL,
      created_at: self.0.accepted_at,
      updated_at: self.0.accepted_at,
      queue: None,
      assignment: None,
      terminal: None,
      event_cursor: 0,
    })
  }
  async fn attempt(
    &self,
    id: octacity_server_domain::AttemptId,
  ) -> Result<octacity_server_store::AttemptRecord, octacity_server_store::StoreError> {
    Ok(octacity_server_store::AttemptRecord {
      id,
      build_id: self.0.build.id,
      number: self.0.attempt_number,
      retry_of_attempt_id: None,
      state: octacity_server_orchestrator::AttemptState::Running,
      version: octacity_server_domain::AttemptVersion::INITIAL,
      created_at: self.0.accepted_at,
      updated_at: self.0.accepted_at,
      jobs: vec![],
    })
  }
  async fn latest_attempt(
    &self,
    _: octacity_server_domain::BuildId,
  ) -> Result<octacity_server_store::AttemptRecord, octacity_server_store::StoreError> {
    unreachable!("a trigger Build is rejected before resolving a Factory attempt")
  }
  async fn build(
    &self,
    _: octacity_server_domain::BuildId,
  ) -> Result<octacity_server_store::BuildRecord, octacity_server_store::StoreError> {
    Ok(octacity_server_store::BuildRecord {
      build: self.0.build.clone(),
      state: octacity_server_orchestrator::BuildState::Running,
      version: octacity_server_domain::BuildVersion::INITIAL,
      origin: octacity_server_store::BuildOrigin::Trigger(Box::new(octacity_server_store::TriggerBuildOrigin {
        occurrence: self.0.trigger.clone(),
        state: octacity_server_trigger::TriggerOccurrenceState::Accepted,
        created_at: self.0.accepted_at,
        updated_at: self.0.accepted_at,
      })),
      created_at: self.0.accepted_at,
      updated_at: self.0.accepted_at,
    })
  }
}

struct Registration {
  fixture: StoreContractFixture,
}

#[async_trait]
impl AgentRegistrationUseCases for Registration {
  async fn register(&self, _: AgentRegistrationInput) -> Result<AgentRegistrationOutcome, AgentRegistrationError> {
    unreachable!("scoped validation does not register an Agent")
  }

  async fn authorize(&self, input: AuthorizeAgentInput) -> Result<AuthorizedAgent, AgentRegistrationError> {
    Ok(AuthorizedAgent::for_test(
      self.fixture.agent_id,
      self.fixture.registration_epoch,
      self.fixture.allowed_pool,
      compatible_inventory(self.fixture.agent_id).host_capacity,
      false,
      input.operation,
    ))
  }
}

#[derive(Default)]
struct Runner(Mutex<Vec<(JobId, ScopedValidationAction)>>);

#[async_trait]
impl LeaseBoundScopedValidation for Runner {
  async fn run(
    &self,
    input: LeaseBoundScopedValidationInput,
  ) -> Result<FactoryScopedValidationVerdict, AgentToolActionError> {
    self.0.lock().unwrap().push((input.job_id, input.action));
    Ok(FactoryScopedValidationVerdict::Waiting)
  }
}

#[test]
fn scoped_validation_reaches_the_runner_only_with_the_current_job_fence() {
  crate::factory::admission::tests::factory_admission_tests::run_ready(async {
    let fixture = authoritative_store_contract_fixture();
    let store = Arc::new(InMemoryStore::new());
    store.seed_authoritative_contract_prerequisites(&fixture).unwrap();
    store.accept_trigger(fixture.request.clone()).await.unwrap();
    let time = |millis| Timestamp::from_unix_millis(millis).unwrap();
    let lease_id = LeaseId::generate();
    let claim = JobClaim::new(
      lease_id,
      StoreFence::from_bytes([17; 32]),
      fixture.agent_id,
      fixture.registration_epoch,
      fixture.allowed_pool,
      compatible_snapshot(),
      LeaseWindow::new(time(1_000), time(5_000)).unwrap(),
    )
    .unwrap();
    let JobClaimOutcome::Claimed(grant) = store.claim_ready_job(claim).await.unwrap() else {
      panic!("the fixture must lease a Job");
    };
    let runner = Arc::new(Runner::default());
    let ordinary = Arc::new(OrdinaryBuild(fixture.request.clone()));
    let service = AgentToolActionService::new(
      Arc::new(Registration { fixture }),
      store.clone(),
      Arc::new(FailClosedToolActionAuthorizer),
    )
    .with_scoped_validation(runner.clone());
    let input = |job: JobId, observed_at_unix_ms, action| AgentScopedValidationInput {
      route_lease_id: lease_id.to_string(),
      request: ScopedValidationRequest {
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
        request_id: "validation-1".to_owned(),
        registration_id: "registration-1".to_owned(),
        lease: LeaseFence {
          lease_id: lease_id.to_string(),
          job_id: job.to_string(),
          attempt: 1,
          fencing_token: "11".repeat(32),
        },
        action,
      },
      credential: AgentCredentialToken::parse(
        "registration.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
      )
      .unwrap(),
      observed_at_unix_ms,
    };
    for (job, at, expected) in [
      (JobId::generate(), 2_000, AgentToolActionError::Fenced),
      (grant.job_id, 5_000, AgentToolActionError::Expired),
    ] {
      assert_eq!(
        service
          .scoped_validation(input(job, at, ScopedValidationAction::Start))
          .await,
        Err(expected)
      );
    }
    assert!(runner.0.lock().unwrap().is_empty());
    for action in [ScopedValidationAction::Start, ScopedValidationAction::Status] {
      assert_eq!(
        service.scoped_validation(input(grant.job_id, 2_000, action)).await,
        Ok(FactoryScopedValidationVerdict::Waiting)
      );
    }
    assert_eq!(
      *runner.0.lock().unwrap(),
      [
        (grant.job_id, ScopedValidationAction::Start),
        (grant.job_id, ScopedValidationAction::Status)
      ]
    );
    let flows = Arc::new(FactoryFlowRunner::new(
      Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new()),
      Arc::new(FactoryNodeExecutorRegistry::new(vec![]).unwrap()),
      Arc::new(crate::factory::execution::tests::factory_flow_tests::Inputs),
      Arc::new(SystemFactoryExecutionClock),
    ));
    let service = service.with_scoped_validation(Arc::new(FactoryScopedValidationService::new(flows, ordinary, store)));
    assert_eq!(
      service
        .scoped_validation(input(grant.job_id, 2_000, ScopedValidationAction::Start))
        .await,
      Err(AgentToolActionError::InvalidRequest),
      "a valid lease from a normal Trigger Build carries no scoped Factory authority",
    );
  });
}
