//! Private transports for authorizing and executing a rooted read.
use serde::{Deserialize, Serialize};

/// Per-execution guest entrypoint for the explicitly configured scoped MCP server.
/// The Agent binds it to its verified immutable helper before starting the workload.
pub const FACTORY_SCOPED_BRIDGE_PATH: &str = "/workspace/scratch/.octacity/scoped-bridge";

/// Versioned private Agent channel request. Authorization checks return no file
/// bytes and consume no response allocation. Reading always obtains fresh
/// lease-bound authorization; this request never carries a reusable permission.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedReadChannelRequestV1 {
  /// Exact private channel version, currently 1.
  pub schema_version: u16,
  /// Whether to check authority only or execute the checked physical read.
  pub operation: ProtectedReadChannelOperationV1,
  /// Complete unchanged proposal, canonicalized and enforced by the Agent.
  pub proposal: crate::FactoryToolActionProposalV3,
}

/// Finite operations accepted on the private protected-reader channel.
#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectedReadChannelOperationV1 {
  /// Wait for a fenced disposition without reading or reserving response bytes.
  Authorize,
  /// Revalidate authority and execute one bounded physical read.
  Read,
}

/// Private request to the active Job's scoped verification tool.
/// The host supplies all registration and lease information.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedValidationChannelRequestV1 {
  /// Exact private channel version, currently 1.
  pub schema_version: u16,
  /// Private channel operation selector.
  pub operation: ScopedValidationChannelOperationV1,
  /// Start or status for the one admitted runner.
  pub action: crate::ScopedValidationAction,
}

/// Operation discriminator shared by the trusted host and the guest tool.
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopedValidationChannelOperationV1 {
  /// Invoke or observe the one scoped verification runner.
  ScopedValidation,
}

/// Frame ceiling covering a maximally escaped portable path and fixed fields.
pub const MAX_PROTECTED_READ_REQUEST_BYTES: usize = 6 * crate::MAX_FACTORY_PATH_BYTES + 128;

/// One physical read instruction sent exclusively by the trusted dispatcher.
/// This value grants no action authority and cannot select a source root,
/// executable, credentials or command. The supervisor fixes the worker root.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedReadWorkerRequestV1 {
  /// Exact worker transport version, currently 1.
  pub schema_version: u16,
  /// Canonical portable path relative to the supervisor-selected source root.
  pub relative_path: String,
  /// Response ceiling narrowed by the dispatcher's remaining signed quota.
  pub max_bytes: u64,
}

impl std::fmt::Debug for ProtectedReadWorkerRequestV1 {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ProtectedReadWorkerRequestV1")
      .field("schema_version", &self.schema_version)
      .field("relative_path", &"<redacted>")
      .field("max_bytes", &self.max_bytes)
      .finish()
  }
}

impl ProtectedReadWorkerRequestV1 {
  /// Rejects unsupported versions, path escape and invalid response bounds.
  pub fn validate(&self) -> Result<(), String> {
    if self.schema_version != 1
      || self.relative_path.len() > crate::MAX_FACTORY_PATH_BYTES
      || self.max_bytes == 0
      || self.max_bytes > crate::MAX_FACTORY_TOOL_RESPONSE_BYTES
    {
      return Err("protected reader request is invalid".to_owned());
    }
    crate::relative_wire_path("protected reader path", &self.relative_path)
  }
}
