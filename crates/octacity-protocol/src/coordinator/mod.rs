//! Strict server-agent transport values independent of HTTP and persistence.
//!
//! Every request carries a caller-generated idempotency identifier. Responses
//! echo that identifier so a client can reject stale or incorrectly routed
//! data before it influences lease state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroize as _;

pub use octa_cache_protocol::{
  TASK_RESULT_CACHE_FEATURE_V1 as CACHE_FEATURE_V1, TASK_RESULT_CACHE_HTTP_FEATURE_V1 as CACHE_HTTP_FEATURE_V1,
};

use crate::{CachePolicy, OciIsolation, PlatformSpec, RuntimeMode, SignedEnvelope};

mod validation;

/// Server-agent transport version implemented by these DTOs.
pub const COORDINATOR_PROTOCOL_VERSION: u16 = 1;
/// Maximum UTF-8 length of a request or opaque protocol identifier.
pub const MAX_COORDINATOR_IDENTIFIER_BYTES: usize = 256;
/// Maximum scheduler labels or inventory entries of one kind.
pub const MAX_INVENTORY_ENTRIES: usize = 1024;
/// Maximum UTF-8 length of an advisory health message.
pub const MAX_HEALTH_MESSAGE_BYTES: usize = 1024;
/// Maximum number of ordered attempt events accepted in one append call.
pub const MAX_EVENT_BATCH_RECORDS: usize = 256;
/// Reserved JSON bytes for append metadata outside the encoded event records.
///
/// This covers maximally escaped bounded registration and lease identifiers,
/// field names, punctuation, and the generated request identifier.
pub const MAX_APPEND_REQUEST_OVERHEAD_BYTES: usize = 4096;
/// Maximum number of required headers on one presigned upload target.
pub const MAX_UPLOAD_HEADERS: usize = 32;
/// Maximum UTF-8 bytes in one presigned upload URL.
pub const MAX_PRESIGNED_UPLOAD_URL_BYTES: usize = 8192;
/// Maximum UTF-8 bytes in one server-required upload header name.
pub const MAX_UPLOAD_HEADER_NAME_BYTES: usize = 256;
/// Maximum UTF-8 bytes in one server-required upload header value.
pub const MAX_UPLOAD_HEADER_VALUE_BYTES: usize = 4096;
/// Maximum bytes accepted for one short-lived cache bearer credential.
pub const MAX_CACHE_CREDENTIAL_BYTES: usize = 16 * 1024;
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
  /// Verified Octa task-result cache capability, when installed.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cache: Option<CacheCapability>,
}

/// Scheduler-visible cache support derived from the installed Octa runner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CacheCapability {
  /// Runner wire version carrying explicit L1 capacity.
  pub runner_protocol: u16,
  /// Canonical action-key format understood by the installed cache engine.
  pub action_key_format: u16,
  /// Whether the agent can negotiate Octa's HTTP L2 transport.
  pub remote_http: bool,
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
  /// Latest cumulative sample, when execution accounting has started.
  pub resource_usage: Option<ResourceUsageSnapshot>,
}

/// Backend-neutral cumulative resource values carried on the coordinator wire.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceUsageSnapshot {
  /// Unix time at which the agent observed this sample.
  pub observed_at_unix_ms: u64,
  /// Monotonic elapsed execution time.
  pub elapsed_ms: u64,
  /// Cumulative CPU time used by the complete execution tree.
  pub cpu_time_ms: u64,
  /// Currently accounted memory in bytes.
  pub memory_current_bytes: u64,
  /// Highest accounted memory usage in bytes.
  pub memory_peak_bytes: u64,
  /// Current writable storage consumption in bytes.
  pub disk_current_bytes: u64,
  /// Highest writable storage consumption in bytes.
  pub disk_peak_bytes: u64,
  /// Cumulative bytes read from accounted block devices.
  pub io_read_bytes: u64,
  /// Cumulative bytes written to accounted block devices.
  pub io_written_bytes: u64,
  /// Cumulative received bytes when the backend exposes network accounting.
  pub network_received_bytes: Option<u64>,
  /// Cumulative transmitted bytes when the backend exposes network accounting.
  pub network_transmitted_bytes: Option<u64>,
}

/// Complete runner event retained without rewriting its schema or sequence.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerEventPayload {
  /// Runner-owned event schema version.
  pub schema_version: u16,
  /// Sequence in the runner's own event stream.
  pub sequence: u64,
  /// Runner-generated RFC 3339 timestamp.
  pub timestamp: String,
  /// Semantic event category defined by the runner schema.
  pub category: String,
  /// Category-specific data preserved without interpretation by the agent.
  pub data: serde_json::Map<String, serde_json::Value>,
}

/// Agent-owned lifecycle information kept separate from Octa runner events.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentLifecycleEvent {
  /// The agent durably entered another attempt lifecycle phase.
  StateChanged {
    /// Newly entered phase.
    state: JobLifecycleState,
  },
  /// One cumulative resource-accounting sample.
  ResourceUsage {
    /// Backend-neutral cumulative counters.
    usage: ResourceUsageSnapshot,
  },
  /// Resource accounting failed but remains below the configured failure limit.
  AccountingUnavailable {
    /// Number of adjacent failed samples including this one.
    consecutive_failures: usize,
  },
}

/// Persisted phases owned by the one-job agent state machine.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobLifecycleState {
  /// Validating dependencies and materializing the source workspace.
  Preparing,
  /// The runner is active or has begun emitting events.
  Running,
  /// Runner execution ended and immutable outputs are being established.
  Freezing,
  /// Frozen artifacts and reports are being published.
  Uploading,
  /// Backend resources and workspace state are being removed.
  Cleaning,
  /// All events are being flushed before the terminal result is recorded.
  Completing,
}

/// One item in a job-attempt stream.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttemptEventKind {
  /// Event emitted by `octa-runner`, preserving its original inner envelope.
  Runner {
    /// Original runner event.
    event: RunnerEventPayload,
  },
  /// Event emitted by the agent's attempt lifecycle.
  Agent {
    /// Agent-owned event.
    event: AgentLifecycleEvent,
  },
}

/// Durable, globally ordered envelope for one fenced attempt.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptEventEnvelope {
  /// Stable job identity.
  pub job_id: String,
  /// Positive attempt number.
  pub attempt: u32,
  /// Lease that owned the attempt when this event was persisted.
  pub lease_id: String,
  /// Fencing value authorizing this event.
  pub fencing_token: String,
  /// Global contiguous sequence across runner and agent events for the attempt.
  pub stream_sequence: u64,
  /// Producer-specific event kept inside the common ordered envelope.
  pub kind: AttemptEventKind,
}

/// Idempotent append of one contiguous event range.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppendEventsRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Current registration epoch.
  pub registration_id: String,
  /// Lease fencing copied onto the append operation.
  pub lease: LeaseFence,
  /// Non-empty contiguous event range in ascending sequence order.
  pub events: Vec<AttemptEventEnvelope>,
}

/// Largest contiguous attempt sequence durably accepted by the server.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppendEventsResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the request identifier.
  pub request_id: String,
  /// Largest contiguous attempt sequence durably stored by the coordinator.
  pub acknowledged_sequence: u64,
}

/// Terminal status of one fenced attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobCompletionStatus {
  /// Every requested task completed successfully.
  Succeeded,
  /// The runner returned a normal task failure.
  Failed,
  /// The control plane or local shutdown cancelled execution.
  Cancelled,
  /// The signed execution deadline elapsed.
  TimedOut,
  /// Source, runner, backend, protocol, or cleanup infrastructure failed.
  InfrastructureFailed,
}

/// Durable terminal result sent only after cleanup and event acknowledgement.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteLeaseRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Current registration epoch.
  pub registration_id: String,
  /// Lease fencing authorizing terminal completion.
  pub lease: LeaseFence,
  /// Stable retry-safe identity derived from the fenced attempt.
  pub completion_id: String,
  /// Last event sequence that must already be durably acknowledged.
  pub last_event_sequence: u64,
  /// Terminal attempt outcome.
  pub status: JobCompletionStatus,
  /// Final cumulative resource totals, when the backend supplied them.
  pub final_usage: Option<ResourceUsageSnapshot>,
  /// Runner-produced structured task results.
  #[serde(default)]
  pub results: Vec<serde_json::Value>,
}

/// Acknowledgement of an idempotently recorded terminal result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteLeaseResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the request identifier.
  pub request_id: String,
  /// Echo of the stable completion identity.
  pub completion_id: String,
}

/// Semantic kind of one runner-declared output.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
  /// User-visible build artifact.
  Artifact,
  /// Machine-readable report with a plugin-owned format identifier.
  Report,
}

/// Logical and physical metadata used to authorize one immutable upload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputUploadMetadata {
  /// Runner execution containing the declaration.
  pub run_id: u64,
  /// Runner task containing the declaration.
  pub task_id: u64,
  /// Artifact or report semantics.
  pub kind: OutputKind,
  /// User-visible registration name.
  pub name: String,
  /// Plugin-provided media type for an artifact.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub content_type: Option<String>,
  /// Opaque plugin-owned format for a report.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub report_format: Option<String>,
  /// Stable transport media type, including the directory archive version.
  pub transport_content_type: String,
  /// Exact bytes sent by the agent.
  pub size_bytes: u64,
  /// Lowercase SHA-256 of those exact bytes.
  pub sha256: String,
}

/// Fenced request for one short-lived presigned upload target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BeginOutputUploadRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Current registration epoch.
  pub registration_id: String,
  /// Fenced lease identity.
  pub lease: LeaseFence,
  /// Stable agent-generated identity for this logical output.
  pub upload_key: String,
  /// Validated output metadata.
  pub output: OutputUploadMetadata,
}

/// Server-authorized upload destination without object-store credentials.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BeginOutputUploadResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the request identifier.
  pub request_id: String,
  /// Opaque server-side upload record identity.
  pub upload_id: String,
  /// Short-lived presigned HTTP PUT URL.
  pub put_url: String,
  /// Exact non-credential headers covered by the presigned request.
  #[serde(default)]
  pub required_headers: BTreeMap<String, String>,
  /// Unix second at which the target expires.
  pub expires_at: u64,
}

impl std::fmt::Debug for BeginOutputUploadResponse {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("BeginOutputUploadResponse")
      .field("protocol_version", &self.protocol_version)
      .field("request_id", &self.request_id)
      .field("upload_id", &self.upload_id)
      .field("put_url", &"<redacted>")
      .field(
        "required_header_names",
        &self.required_headers.keys().collect::<Vec<_>>(),
      )
      .field("expires_at", &self.expires_at)
      .finish()
  }
}

/// Idempotent notification that all expected object bytes were uploaded.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteOutputUploadRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Current registration epoch.
  pub registration_id: String,
  /// Fenced lease identity.
  pub lease: LeaseFence,
  /// Opaque upload identity returned by begin.
  pub upload_id: String,
}

/// Acknowledgement that the server verified and published one object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteOutputUploadResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the request identifier.
  pub request_id: String,
  /// Echo of the completed upload identity.
  pub upload_id: String,
}

/// Fenced request for server authorization of one runner cache session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BeginCacheSessionRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Current registration epoch.
  pub registration_id: String,
  /// Lease fence authorizing the operation.
  pub lease: LeaseFence,
  /// Signed namespace and independently narrowed access permissions.
  pub cache: CachePolicy,
}

/// Optional remote L2 authority returned for an active fenced cache session.
#[derive(Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteCacheGrant {
  /// HTTPS base URL implementing Octa's cache HTTP protocol.
  pub endpoint: String,
  /// Short-lived bearer value written only to a private per-job file.
  pub bearer_token: String,
  /// First Unix second at which the bearer is no longer usable.
  pub expires_at: u64,
}

impl std::fmt::Debug for RemoteCacheGrant {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("RemoteCacheGrant")
      .field("endpoint", &self.endpoint)
      .field("bearer_token", &"<redacted>")
      .field("expires_at", &self.expires_at)
      .finish()
  }
}

impl Drop for RemoteCacheGrant {
  fn drop(&mut self) {
    self.bearer_token.zeroize();
  }
}

/// Server-authorized cache scope and optional remote transport authority.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BeginCacheSessionResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the request identifier.
  pub request_id: String,
  /// Opaque revocable cache-session identity.
  pub session_id: String,
  /// Opaque trust-domain scope used to isolate persistent L1 state.
  pub scope_id: String,
  /// Optional remote L2 authority; absence requests an L1-only session.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub remote: Option<RemoteCacheGrant>,
}

/// Idempotent fenced revocation of a cache session after runner shutdown.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeCacheSessionRequest {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Idempotency and response-correlation identifier.
  pub request_id: String,
  /// Current registration epoch.
  pub registration_id: String,
  /// Lease fence authorizing the operation.
  pub lease: LeaseFence,
  /// Opaque session returned by the corresponding begin operation.
  pub session_id: String,
}

/// Acknowledgement that remote cache authority has been revoked.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeCacheSessionResponse {
  /// Coordinator wire version.
  pub protocol_version: u16,
  /// Echo of the request identifier.
  pub request_id: String,
  /// Echo of the revoked session identity.
  pub session_id: String,
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
