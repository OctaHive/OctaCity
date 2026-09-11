//! Strict server-agent transport values independent of HTTP and persistence.
//!
//! Every request carries a caller-generated idempotency identifier. Responses
//! echo that identifier so a client can reject stale or incorrectly routed
//! data before it influences lease state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{OciIsolation, PlatformSpec, RuntimeMode, SignedEnvelope};

mod validation;

/// Server-agent transport version implemented by these DTOs.
pub const COORDINATOR_PROTOCOL_VERSION: u16 = 1;
/// Maximum UTF-8 length of a request or opaque protocol identifier.
pub const MAX_COORDINATOR_IDENTIFIER_BYTES: usize = 256;
/// Maximum scheduler labels or inventory entries of one kind.
pub const MAX_INVENTORY_ENTRIES: usize = 1024;
/// Maximum UTF-8 length of an advisory health message.
pub const MAX_HEALTH_MESSAGE_BYTES: usize = 1024;

/// Complete scheduler-visible inventory sent during registration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentInventory {
  /// Stable operator-configured agent identity.
  pub agent_id: String,
  /// Running OctaCity agent release version.
  pub agent_version: String,
  /// Coordinator protocol versions accepted by the agent.
  pub coordinator_protocols: Vec<u16>,
  /// Operator-owned scheduler labels.
  pub labels: BTreeMap<String, String>,
  /// Host operating system and architecture.
  pub host_platform: PlatformSpec,
  /// Static host capacity measured at registration.
  pub host_capacity: HostCapacity,
  /// Exact execution capabilities validated at startup.
  pub runtimes: Vec<RuntimeCapability>,
  /// Verified Octa runner and task-plugin release.
  pub octa: OctaInventory,
  /// Verified operator-installed source plugins.
  pub source_plugins: Vec<SourcePluginInventory>,
}

/// Static host resources used only as advisory scheduling input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostCapacity {
  /// Number of logical CPUs visible to the agent process.
  pub logical_cpu_count: u32,
  /// Total physical memory visible to the host in bytes.
  pub total_memory_bytes: u64,
  /// Total capacity of the filesystem containing `work_root`.
  pub work_disk_total_bytes: u64,
  /// Total capacity of the filesystem containing `state_root`.
  pub state_disk_total_bytes: u64,
  /// Whether at least one validated runtime provides hypervisor isolation.
  pub virtualization_available: bool,
}

/// One exact execution route the agent can enforce without fallback.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCapability {
  /// Stable implementation name such as `native`, `containerd`, or `microsandbox`.
  pub backend: String,
  /// Top-level execution mode.
  pub mode: RuntimeMode,
  /// Exact host or guest platform.
  pub platform: PlatformSpec,
  /// OCI isolation tier; absent only for Native execution.
  pub isolation: Option<OciIsolation>,
}

/// Verified installed Octa release advertised to the scheduler.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OctaInventory {
  /// Octa release version.
  pub version: String,
  /// SHA-256 digest of the runner executable.
  pub runner_sha256: String,
  /// Optional release source commit.
  pub build_commit: Option<String>,
  /// Supported runner protocol versions.
  pub runner_protocols: Vec<u16>,
  /// Supported runner event-schema versions.
  pub event_schemas: Vec<u16>,
  /// Supported Octa task-plugin protocol versions.
  pub plugin_protocols: Vec<u16>,
  /// Supported Octafile schema versions.
  pub octafile_versions: Vec<u8>,
  /// Optional compiled feature identifiers.
  pub features: Vec<String>,
  /// Task plugins verified from the installed lock file.
  pub plugins: Vec<TaskPluginInventory>,
}

/// One task plugin verified from the installed Octa lock.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPluginInventory {
  /// Logical task-plugin name.
  pub name: String,
  /// Exact plugin version.
  pub version: String,
  /// Process-protocol version.
  pub protocol: u16,
  /// Supported host or guest platform identifiers.
  pub platforms: Vec<String>,
  /// SHA-256 digest of the executable.
  pub sha256: String,
  /// Semantic task capabilities used for scheduling and UI hints.
  pub capabilities: Vec<String>,
}

/// One source plugin verified from the operator-owned registry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePluginInventory {
  /// Logical source-provider name.
  pub name: String,
  /// Exact plugin version.
  pub version: String,
  /// Oldest source protocol implemented by the plugin.
  pub protocol_min: u16,
  /// Newest source protocol implemented by the plugin.
  pub protocol_max: u16,
  /// Supported host platform identifiers.
  pub platforms: Vec<String>,
  /// SHA-256 digest of the executable.
  pub sha256: String,
}

/// Registration call body.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterAgentRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Complete validated agent inventory.
  pub inventory: AgentInventory,
}

/// Successful registration result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterAgentResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the request identifier.
  pub request_id: String,
  /// Opaque identity of this registration epoch.
  pub registration_id: String,
  /// Server-provided ceiling for retry delays.
  pub max_retry_delay_ms: u64,
}

/// Long-poll lease acquisition body.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcquireLeaseRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Current registration epoch.
  pub registration_id: String,
  /// Maximum server wait before returning no work.
  pub wait_seconds: u64,
}

/// Result of one lease long poll.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum AcquireLeaseResponse {
  /// The server assigned one fenced job attempt.
  Lease {
    /// Coordinator wire version.
    protocol_version: u16,
    /// Echo of the request identifier.
    request_id: String,
    /// Complete lease and signed execution intent.
    lease: LeaseAssignment,
  },
  /// No matching job became available during the poll.
  NoWork {
    /// Coordinator wire version.
    protocol_version: u16,
    /// Echo of the request identifier.
    request_id: String,
    /// Minimum delay before beginning another poll.
    retry_after_ms: u64,
  },
  /// The server requests that this agent stop acquiring jobs.
  Drain {
    /// Coordinator wire version.
    protocol_version: u16,
    /// Echo of the request identifier.
    request_id: String,
  },
}

/// One fenced job attempt assigned to the agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseAssignment {
  /// Opaque lease identity.
  pub lease_id: String,
  /// Stable job identity bound into the signed JobSpec.
  pub job_id: String,
  /// Positive execution attempt bound into the signed JobSpec.
  pub attempt: u32,
  /// Opaque fencing value required on every lease operation.
  pub fencing_token: String,
  /// Unix second at which this lease was issued.
  pub issued_at: u64,
  /// First Unix second at which this lease is no longer owned.
  pub expires_at: u64,
  /// Signed execution intent for this exact job attempt.
  pub signed_job_spec: SignedEnvelope,
}

/// Lease identity copied onto heartbeat and future job operations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseFence {
  /// Opaque lease identity.
  pub lease_id: String,
  /// Stable job identity.
  pub job_id: String,
  /// Positive execution attempt.
  pub attempt: u32,
  /// Opaque fencing value issued for this attempt.
  pub fencing_token: String,
}

impl From<&LeaseAssignment> for LeaseFence {
  fn from(lease: &LeaseAssignment) -> Self {
    Self {
      lease_id: lease.lease_id.clone(),
      job_id: lease.job_id.clone(),
      attempt: lease.attempt,
      fencing_token: lease.fencing_token.clone(),
    }
  }
}

/// Current advisory host and backend availability sent with a heartbeat.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostSnapshot {
  /// Estimated currently available CPU capacity in millicpu.
  pub available_cpu_millis: u64,
  /// Currently available host memory in bytes.
  pub available_memory_bytes: u64,
  /// Free bytes on the filesystem containing `work_root`.
  pub work_disk_free_bytes: u64,
  /// Free bytes on the filesystem containing `state_root`.
  pub state_disk_free_bytes: u64,
  /// Current job identity, when the agent is not idle.
  pub active_job: Option<ActiveJob>,
  /// Current advisory health of every configured backend.
  pub backends: Vec<BackendHealth>,
}

/// Job identity included in an advisory host snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveJob {
  /// Stable job identity.
  pub job_id: String,
  /// Positive execution attempt.
  pub attempt: u32,
  /// Opaque current lease identity.
  pub lease_id: String,
}

/// Advisory state of one concrete execution implementation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BackendHealth {
  /// Stable backend name matching a registered runtime capability.
  pub backend: String,
  /// Current readiness state.
  pub status: BackendHealthStatus,
  /// Optional bounded diagnostic safe to send to the coordinator.
  pub message: Option<String>,
}

/// Scheduler-facing backend readiness state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendHealthStatus {
  /// Backend is ready to accept its advertised jobs.
  Ready,
  /// Backend remains usable but an advisory probe reported degradation.
  Degraded,
  /// Backend must not receive new jobs.
  Unavailable,
}

/// Heartbeat body for one active lease.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Current registration epoch.
  pub registration_id: String,
  /// Fenced lease identity.
  pub lease: LeaseFence,
  /// Latest advisory host snapshot.
  pub snapshot: HostSnapshot,
}

/// Successful heartbeat response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the request identifier.
  pub request_id: String,
  /// Explicit lease action selected by the server.
  pub directive: HeartbeatDirective,
}

/// Structured body returned with a non-success coordinator status.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinatorErrorResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the rejected request identifier.
  pub request_id: String,
  /// Stable machine-readable failure code.
  pub code: String,
  /// Bounded human-readable diagnostic without credentials.
  pub message: String,
  /// Whether the exact idempotent request may be retried.
  pub retryable: bool,
  /// Optional server-suggested delay before retrying.
  pub retry_after_ms: Option<u64>,
}

/// Explicit action returned by every successful heartbeat.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum HeartbeatDirective {
  /// Continue the job under the renewed expiry.
  Continue {
    /// First Unix second at which the renewed lease is no longer owned.
    expires_at: u64,
  },
  /// Cooperatively cancel the active job.
  Cancel,
  /// Stop immediately because a newer fencing token owns the attempt.
  Fenced,
  /// Finish the active job but acquire no subsequent work.
  Drain {
    /// First Unix second at which the renewed lease is no longer owned.
    expires_at: u64,
  },
}

/// Semantic validation failure for coordinator transport values.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("invalid coordinator protocol value: {0}")]
pub struct CoordinatorProtocolError(String);

impl CoordinatorProtocolError {
  fn new(message: impl Into<String>) -> Self {
    Self(message.into())
  }
}

#[cfg(test)]
mod tests;
