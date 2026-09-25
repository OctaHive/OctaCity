use std::sync::Arc;

use async_trait::async_trait;
use octacity_protocol::{
  AgentCredentialToken, AgentTelemetryIsolation, AgentTelemetryRuntime, AgentTelemetrySample,
  COORDINATOR_PROTOCOL_VERSION, HostCapacity, IngestAgentTelemetryRequest, MAX_AGENT_TELEMETRY_SAMPLES,
};
use octacity_server_domain::{AgentId, PoolId};
use octacity_server_store::RegistrationEpoch;
use uuid::Uuid;

use crate::{
  AgentOperation, AgentRegistrationError, AgentRegistrationInput, AgentRegistrationOutcome, AgentRegistrationUseCases,
  AgentTelemetryBatch, AgentTelemetryError, AgentTelemetryInput, AgentTelemetryService, AgentTelemetryUseCases,
  AuthorizeAgentInput, AuthorizedAgent,
};

struct RegistrationStub;

#[async_trait]
impl AgentRegistrationUseCases for RegistrationStub {
  async fn register(&self, _input: AgentRegistrationInput) -> Result<AgentRegistrationOutcome, AgentRegistrationError> {
    unreachable!("telemetry does not register an Agent")
  }

  async fn authorize(&self, input: AuthorizeAgentInput) -> Result<AuthorizedAgent, AgentRegistrationError> {
    assert_eq!(input.operation, AgentOperation::Telemetry);
    Ok(AuthorizedAgent::for_test(
      AgentId::from_uuid(Uuid::from_u128(1)).unwrap(),
      RegistrationEpoch::new(1).unwrap(),
      PoolId::from_uuid(Uuid::from_u128(2)).unwrap(),
      HostCapacity {
        logical_cpu_count: 1,
        total_memory_bytes: 1,
        work_disk_total_bytes: 1,
        state_disk_total_bytes: 1,
        virtualization_available: false,
      },
      AgentOperation::Telemetry,
    ))
  }
}

#[test]
fn authorizes_a_validated_batch_for_diagnostic_export() {
  let service = AgentTelemetryService::new(Arc::new(RegistrationStub));

  let batch = run_ready(service.authorize_ingest(input())).unwrap();

  assert_eq!(batch.request_id(), "request");
  assert_eq!(batch.samples().len(), 1);
}

#[test]
fn authenticated_batch_cannot_bypass_the_protocol_bound() {
  let mut request = input().request;
  request.samples = vec![request.samples[0].clone(); MAX_AGENT_TELEMETRY_SAMPLES + 1];

  assert!(matches!(
    AgentTelemetryBatch::try_from_request(request),
    Err(AgentTelemetryError::InvalidRequest)
  ));
}

fn input() -> AgentTelemetryInput {
  AgentTelemetryInput {
    request: IngestAgentTelemetryRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: "request".to_owned(),
      registration_id: "registration".to_owned(),
      samples: vec![AgentTelemetrySample {
        observed_at_unix_ms: 1,
        runtime: AgentTelemetryRuntime::Native,
        isolation: AgentTelemetryIsolation::Native,
        cpu_time_ms: 1,
        memory_current_bytes: 2,
        io_read_bytes: 3,
        io_written_bytes: 4,
        network_received_bytes: None,
        network_transmitted_bytes: None,
      }],
    },
    credential: AgentCredentialToken::enrollment("seed", [7; 32])
      .unwrap()
      .promote("registration")
      .unwrap(),
    observed_at_unix_ms: 1,
  }
}

fn run_ready<F: std::future::Future>(future: F) -> F::Output {
  let mut future = std::pin::pin!(future);
  let waker = std::task::Waker::noop();
  let mut context = std::task::Context::from_waker(waker);
  match future.as_mut().poll(&mut context) {
    std::task::Poll::Ready(value) => value,
    std::task::Poll::Pending => panic!("stubbed future unexpectedly blocked"),
  }
}
