#![no_main]

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey};
use libfuzzer_sys::fuzz_target;
use octacity_protocol::*;

// Keep the public JSON seam explicit. Adding a new wire DTO should extend this
// list so arbitrary input reaches its standalone serde implementation as well
// as any parent envelope that happens to contain it.
macro_rules! decode_all {
  ($data:expr; $($wire_type:ty),+ $(,)?) => {
    $(let _ = serde_json::from_slice::<$wire_type>($data);)+
  };
}

fuzz_target!(|data: &[u8]| {
  decode_all!(data;
    SignedEnvelope,
    JobSpecV1,
    CachePolicy,
    SourceSpec,
    OctaSpec,
    ExecutionSpec,
    RuntimeMode,
    PlatformOs,
    PlatformArchitecture,
    PlatformSpec,
    OciIsolation,
    RuntimeTarget,
    RuntimeSpec,
    NetworkPolicy,
    OutputLimits,
    AgentInventory,
    CacheCapability,
    HostCapacity,
    RuntimeCapability,
    OctaInventory,
    TaskPluginInventory,
    SourcePluginInventory,
    RegisterAgentRequest,
    RegisterAgentResponse,
    AcquireLeaseRequest,
    AcquireLeaseResponse,
    LeaseAssignment,
    LeaseFence,
    HostSnapshot,
    ActiveJob,
    ResourceUsageSnapshot,
    RunnerEventPayload,
    AgentLifecycleEvent,
    JobLifecycleState,
    AttemptEventKind,
    AttemptEventEnvelope,
    AppendEventsRequest,
    AppendEventsResponse,
    JobCompletionStatus,
    CompleteLeaseRequest,
    CompleteLeaseResponse,
    OutputKind,
    OutputUploadMetadata,
    BeginOutputUploadRequest,
    BeginOutputUploadResponse,
    CompleteOutputUploadRequest,
    CompleteOutputUploadResponse,
    BeginCacheSessionRequest,
    RemoteCacheGrant,
    BeginCacheSessionResponse,
    RevokeCacheSessionRequest,
    RevokeCacheSessionResponse,
    BackendHealth,
    BackendHealthStatus,
    HeartbeatRequest,
    HeartbeatResponse,
    CoordinatorErrorResponse,
    HeartbeatDirective,
  );

  // Drive arbitrary authenticated bytes through signature verification,
  // payload JSON decoding, and JobSpec semantic validation. Raw envelopes
  // above separately explore malformed base64 and wire fields.
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let envelope = SignedEnvelope {
    key_id: "fuzz".to_owned(),
    algorithm: SIGNATURE_ALGORITHM.to_owned(),
    payload: BASE64.encode(data),
    signature: BASE64.encode(signing_key.sign(data).to_bytes()),
  };
  let keys = BTreeMap::from([("fuzz".to_owned(), signing_key.verifying_key())]);
  let _ = verify_job_spec(
    &envelope,
    &keys,
    JobBinding {
      job_id: "fuzz",
      attempt: 1,
      now: 1,
    },
  );
});
