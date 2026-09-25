//! Bounded diagnostic telemetry exchanged outside correctness-critical flows.

use serde::{Deserialize, Serialize};

/// Maximum number of resource snapshots accepted in one telemetry request.
pub const MAX_AGENT_TELEMETRY_SAMPLES: usize = 128;
/// Maximum encoded JSON bytes accepted for one telemetry request.
pub const MAX_AGENT_TELEMETRY_REQUEST_BYTES: usize = 64 * 1024;

/// Stable execution runtime used as a bounded telemetry label.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTelemetryRuntime {
  /// Direct host-process execution.
  Native,
  /// OCI execution through containerd.
  Containerd,
  /// OCI execution inside a microsandbox VM.
  Microsandbox,
}

/// Stable isolation category used as a bounded telemetry label.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTelemetryIsolation {
  /// Direct host-process execution.
  Native,
  /// OCI process isolation on the host kernel.
  OciProcess,
  /// OCI execution behind a hypervisor boundary.
  OciHypervisor,
}

/// One bounded execution resource sample for a completed measurement interval.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentTelemetrySample {
  /// Unix time at which the agent observed the sample.
  pub observed_at_unix_ms: u64,
  /// Runtime implementation producing the sample.
  pub runtime: AgentTelemetryRuntime,
  /// Enforced isolation category.
  pub isolation: AgentTelemetryIsolation,
  /// CPU time used by the execution tree during the measurement interval.
  pub cpu_time_ms: u64,
  /// Current accounted memory in bytes.
  pub memory_current_bytes: u64,
  /// Bytes read from accounted block devices during the measurement interval.
  pub io_read_bytes: u64,
  /// Bytes written to accounted block devices during the measurement interval.
  pub io_written_bytes: u64,
  /// Bytes received during the interval when the backend exposes network accounting.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub network_received_bytes: Option<u64>,
  /// Bytes transmitted during the interval when the backend exposes network accounting.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub network_transmitted_bytes: Option<u64>,
}

/// Authenticated diagnostic upload independent of lease renewal and event ordering.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IngestAgentTelemetryRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Current registration epoch.
  pub registration_id: String,
  /// Non-empty, time-ordered resource samples.
  pub samples: Vec<AgentTelemetrySample>,
}

/// Explicit accounting of accepted and intentionally discarded samples.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IngestAgentTelemetryResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the request identifier.
  pub request_id: String,
  /// Samples handed to the configured exporter.
  pub accepted_samples: u16,
  /// Samples discarded because the diagnostic path was unavailable or saturated.
  pub dropped_samples: u16,
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::COORDINATOR_PROTOCOL_VERSION;

  #[test]
  fn accepts_a_bounded_ordered_batch() {
    assert!(request(vec![sample(1), sample(2)]).validate().is_ok());
  }

  #[test]
  fn rejects_empty_oversized_and_mislabelled_batches() {
    assert!(request(Vec::new()).validate().is_err());
    assert!(
      request(vec![sample(1); MAX_AGENT_TELEMETRY_SAMPLES + 1])
        .validate()
        .is_err()
    );
    let mut inconsistent = sample(1);
    inconsistent.isolation = AgentTelemetryIsolation::OciHypervisor;
    assert!(request(vec![inconsistent]).validate().is_err());
    assert!(request(vec![sample(2), sample(1)]).validate().is_err());
  }

  #[test]
  fn response_accounts_for_every_sample() {
    let response = IngestAgentTelemetryResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: "request".to_owned(),
      accepted_samples: 1,
      dropped_samples: 1,
    };
    assert!(response.validate("request", 2).is_ok());
    assert!(response.validate("request", 3).is_err());
  }

  #[test]
  fn rejects_unknown_wire_fields() {
    let mut value = serde_json::to_value(request(vec![sample(1)])).unwrap();
    value
      .as_object_mut()
      .unwrap()
      .insert("unexpected".to_owned(), serde_json::json!(true));

    assert!(serde_json::from_value::<IngestAgentTelemetryRequest>(value).is_err());
  }

  fn request(samples: Vec<AgentTelemetrySample>) -> IngestAgentTelemetryRequest {
    IngestAgentTelemetryRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: "request".to_owned(),
      registration_id: "registration".to_owned(),
      samples,
    }
  }

  fn sample(observed_at_unix_ms: u64) -> AgentTelemetrySample {
    AgentTelemetrySample {
      observed_at_unix_ms,
      runtime: AgentTelemetryRuntime::Native,
      isolation: AgentTelemetryIsolation::Native,
      cpu_time_ms: 1,
      memory_current_bytes: 2,
      io_read_bytes: 3,
      io_written_bytes: 4,
      network_received_bytes: None,
      network_transmitted_bytes: None,
    }
  }
}
