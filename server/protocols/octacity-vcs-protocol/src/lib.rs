//! Strict provider-neutral VCS adapter process protocol.
//!
//! See the [language-neutral v1 specification](https://github.com/OctaHive/OctaCity/blob/main/docs/protocols/vcs-provider-v1.md).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_adapter_protocol::{self as adapter_core, ValidationFailure};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Protocol version implemented by this crate.
pub const VCS_PROTOCOL_VERSION: u16 = 1;
/// Maximum bytes in an opaque identifier, reference, revision, or path.
pub const MAX_FIELD_BYTES: usize = 1024;
/// Maximum reference or tree entries in one page.
pub const MAX_PAGE_ENTRIES: u16 = 256;
/// Maximum bytes returned by one file-content request.
pub const MAX_FILE_CONTENT_BYTES: u32 = 1024 * 1024;
/// Maximum base64 characters that can encode one bounded file fragment.
pub const MAX_FILE_CONTENT_BASE64_BYTES: usize = (MAX_FILE_CONTENT_BYTES as usize).div_ceil(3) * 4;
/// Maximum bounded implementation metadata entries.
pub const MAX_METADATA_ENTRIES: usize = 32;
/// Maximum bytes in one metadata value, message, or diagnostic.
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
/// Maximum encoded bytes accepted for one complete request or response.
pub const MAX_VCS_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

/// Inclusive range of VCS protocol versions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRange {
  /// Oldest supported version.
  pub min: u16,
  /// Newest supported version.
  pub max: u16,
}

impl ProtocolRange {
  /// Selects the newest mutually supported version.
  pub fn negotiate(self, other: Self) -> Result<u16, ProtocolError> {
    adapter_core::negotiate_version(self.min, self.max, other.min, other.max).map_err(|failure| match failure {
      ValidationFailure::InvalidRange => ProtocolError::Invalid("invalid protocol range"),
      ValidationFailure::IncompatibleVersion => ProtocolError::IncompatibleVersion,
      _ => unreachable!("version negotiation returns only range failures"),
    })
  }
}

/// Provider-neutral operations advertised by an installed adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
  /// Lists bounded branches and tags.
  pub list_references: bool,
  /// Returns immutable commit metadata.
  pub read_commit: bool,
  /// Lists one tree page without materializing a working tree.
  pub list_tree: bool,
  /// Reads bounded file bytes.
  pub read_file: bool,
  /// Resolves an allowed mutable reference to one immutable revision.
  pub resolve_revision: bool,
}

/// Identity and compatibility declaration for one installed adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
  /// Operator-configured adapter identity.
  pub adapter_id: String,
  /// Lowercase SHA-256 of the immutable executable.
  pub executable_sha256: String,
  /// Supported protocol range.
  pub protocol: ProtocolRange,
  /// Supported operations.
  pub capabilities: Capabilities,
}

impl AdapterManifest {
  /// Validates adapter identity, digest, and protocol range.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    field("adapter_id", &self.adapter_id)?;
    sha256(&self.executable_sha256)?;
    self.protocol.negotiate(self.protocol)?;
    Ok(())
  }
}

/// One strict VCS adapter request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Request {
  /// Exact negotiated version.
  pub protocol_version: u16,
  /// Caller-generated response-correlation identity.
  pub request_id: String,
  /// Requested operation.
  pub command: Command,
}

/// One strict VCS adapter response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Response {
  /// Exact negotiated protocol version.
  pub protocol_version: u16,
  /// Echo of the request identity.
  pub request_id: String,
  /// Operation result or classified failure.
  pub outcome: Outcome,
}

impl Response {
  /// Validates version, bounds, and result invariants.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.protocol_version != VCS_PROTOCOL_VERSION {
      return Err(ProtocolError::IncompatibleVersion);
    }
    field("request_id", &self.request_id)?;
    match &self.outcome {
      Outcome::References(value) => value.validate(),
      Outcome::Commit(value) => value.validate(),
      Outcome::Tree(value) => value.validate(),
      Outcome::File(value) => value.validate(),
      Outcome::Resolved(value) => {
        field("reference", &value.reference)?;
        field("revision", &value.revision)
      }
      Outcome::Acknowledged { operation_id } => field("operation_id", operation_id),
      Outcome::Failure(value) => value.validate(),
    }
  }

  /// Validates the response and its version and identifier correlation.
  pub fn validate_for(&self, request: &Request) -> Result<(), ProtocolError> {
    request.validate()?;
    self.validate()?;
    adapter_core::require_correlation(
      self.protocol_version,
      &self.request_id,
      request.protocol_version,
      &request.request_id,
    )
    .map_err(|_| ProtocolError::CorrelationMismatch)?;
    validate_outcome_for(&request.command, &self.outcome)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestWire {
  protocol_version: u16,
  request_id: String,
  command: Command,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseWire {
  protocol_version: u16,
  request_id: String,
  outcome: Outcome,
}

/// Decodes and validates one size-bounded VCS request.
pub fn decode_request(message: &[u8]) -> Result<Request, ProtocolError> {
  bounded_message(message)?;
  let wire: RequestWire = adapter_core::decode_json(message).map_err(|_| ProtocolError::MalformedMessage)?;
  let request = Request {
    protocol_version: wire.protocol_version,
    request_id: wire.request_id,
    command: wire.command,
  };
  request.validate()?;
  Ok(request)
}

/// Decodes a size-bounded response and verifies correlation to `request`.
pub fn decode_response(message: &[u8], request: &Request) -> Result<Response, ProtocolError> {
  bounded_message(message)?;
  let wire: ResponseWire = adapter_core::decode_json(message).map_err(|_| ProtocolError::MalformedMessage)?;
  let response = Response {
    protocol_version: wire.protocol_version,
    request_id: wire.request_id,
    outcome: wire.outcome,
  };
  response.validate_for(request)?;
  Ok(response)
}

/// Provider-neutral VCS operation outcomes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
  /// One bounded reference page.
  References(ReferencePage),
  /// Immutable commit metadata.
  Commit(Commit),
  /// One bounded tree page.
  Tree(TreePage),
  /// One bounded file-content fragment.
  File(FileContent),
  /// Exact immutable resolution of one reference.
  Resolved(ResolvedRevision),
  /// Cancellation or another idempotent operation completed.
  Acknowledged {
    /// Stable operation identity.
    operation_id: String,
  },
  /// Classified adapter failure.
  Failure(Failure),
}

impl Request {
  /// Validates version, bounds, and operation semantics.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.protocol_version != VCS_PROTOCOL_VERSION {
      return Err(ProtocolError::IncompatibleVersion);
    }
    field("request_id", &self.request_id)?;
    match &self.command {
      Command::ListReferences(value) => {
        value.repository.validate()?;
        if let Some(cursor) = &value.cursor {
          field("reference cursor", cursor)?;
        }
        if let Some(prefix) = &value.prefix {
          field("reference prefix", prefix)?;
        }
        page(value.page_size)
      }
      Command::ReadCommit(value) => {
        value.repository.validate()?;
        field("revision", &value.revision)
      }
      Command::ListTree(value) => {
        value.repository.validate()?;
        field("revision", &value.revision)?;
        if let Some(path) = &value.path {
          repository_path("path", path)?;
        }
        if let Some(cursor) = &value.cursor {
          field("tree cursor", cursor)?;
        }
        page(value.page_size)
      }
      Command::ReadFile(value) => {
        value.repository.validate()?;
        field("revision", &value.revision)?;
        repository_path("path", &value.path)?;
        if value.max_bytes == 0 || value.max_bytes > MAX_FILE_CONTENT_BYTES {
          return Err(ProtocolError::LimitExceeded("file content bytes"));
        }
        Ok(())
      }
      Command::ResolveRevision(value) => {
        value.repository.validate()?;
        field("reference", &value.reference)
      }
      Command::Cancel(value) => field("target_operation_id", &value.target_operation_id),
    }
  }
}

/// VCS protocol operations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
  tag = "operation",
  content = "payload",
  rename_all = "snake_case",
  deny_unknown_fields
)]
pub enum Command {
  /// List a bounded page of branches or tags.
  ListReferences(ListReferences),
  /// Read metadata for one immutable commit.
  ReadCommit(ReadCommit),
  /// List a bounded tree page at one immutable revision.
  ListTree(ListTree),
  /// Read bounded file content at one immutable revision.
  ReadFile(ReadFile),
  /// Resolve a mutable reference exactly once.
  ResolveRevision(ResolveRevision),
  /// Cooperatively cancel an in-flight operation.
  Cancel(CancelOperation),
}

/// Common repository access fields.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryAccess {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Server-owned logical repository identity.
  pub repository_id: String,
  /// Credential-free repository locator from the immutable repository snapshot.
  pub repository_locator: String,
  /// Host-owned credential handle, never raw provider credentials.
  pub credential_handle: String,
}

impl std::fmt::Debug for RepositoryAccess {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("RepositoryAccess")
      .field("operation_id", &self.operation_id)
      .field("repository_id", &self.repository_id)
      .field("repository_locator", &"<redacted>")
      .field("credential_handle", &"<redacted>")
      .finish()
  }
}

impl RepositoryAccess {
  fn validate(&self) -> Result<(), ProtocolError> {
    field("operation_id", &self.operation_id)?;
    field("repository_id", &self.repository_id)?;
    field("repository_locator", &self.repository_locator)?;
    field("credential_handle", &self.credential_handle)
  }
}

/// Bounded reference listing request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListReferences {
  /// Repository and credential context.
  pub repository: RepositoryAccess,
  /// Optional opaque continuation cursor.
  pub cursor: Option<String>,
  /// Maximum entries requested.
  pub page_size: u16,
  /// Optional prefix filter interpreted as data.
  pub prefix: Option<String>,
}

/// Immutable commit metadata request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadCommit {
  /// Repository and credential context.
  pub repository: RepositoryAccess,
  /// Immutable revision identity.
  pub revision: String,
}

/// Bounded tree listing request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListTree {
  /// Repository and credential context.
  pub repository: RepositoryAccess,
  /// Immutable revision identity.
  pub revision: String,
  /// Optional repository-relative tree path.
  pub path: Option<String>,
  /// Optional opaque continuation cursor.
  pub cursor: Option<String>,
  /// Maximum entries requested.
  pub page_size: u16,
}

/// Bounded immutable file read request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadFile {
  /// Repository and credential context.
  pub repository: RepositoryAccess,
  /// Immutable revision identity.
  pub revision: String,
  /// Repository-relative file path.
  pub path: String,
  /// Byte offset for this bounded read.
  pub offset: u64,
  /// Maximum bytes returned.
  pub max_bytes: u32,
}

/// Mutable-reference resolution request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveRevision {
  /// Repository and credential context.
  pub repository: RepositoryAccess,
  /// Allowed branch, tag, or revision expression.
  pub reference: String,
}

/// Cooperative cancellation request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelOperation {
  /// Operation to cancel; repeated cancellation is safe.
  pub target_operation_id: String,
}

/// Reference kind independent of Git or another provider.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
  /// Branch-like mutable reference.
  Branch,
  /// Tag-like reference.
  Tag,
  /// Another provider-neutral advertised reference.
  Other,
}

/// One normalized reference entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
  /// Full reference name.
  pub name: String,
  /// Normalized reference kind.
  pub kind: ReferenceKind,
  /// Immutable resolved revision.
  pub revision: String,
}

impl Reference {
  fn validate(&self) -> Result<(), ProtocolError> {
    field("reference name", &self.name)?;
    field("reference revision", &self.revision)
  }
}

/// One deterministic bounded page of references.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferencePage {
  /// Ordered reference entries.
  pub entries: Vec<Reference>,
  /// Opaque continuation cursor when more entries remain.
  pub next_cursor: Option<String>,
}

impl ReferencePage {
  fn validate(&self) -> Result<(), ProtocolError> {
    if self.entries.len() > usize::from(MAX_PAGE_ENTRIES) {
      return Err(ProtocolError::LimitExceeded("reference page"));
    }
    for entry in &self.entries {
      entry.validate()?;
    }
    if let Some(cursor) = &self.next_cursor {
      field("reference cursor", cursor)?;
    }
    Ok(())
  }
}

/// Provider-neutral immutable commit metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
  /// Immutable revision identity.
  pub revision: String,
  /// Ordered immutable parent revisions.
  pub parents: Vec<String>,
  /// Bounded commit message.
  pub message: String,
  /// Author display metadata, not an authenticated identity.
  pub author_display: Option<String>,
  /// Provider commit time in Unix milliseconds.
  pub committed_at_unix_ms: Option<u64>,
  /// Bounded implementation metadata not used for policy.
  #[serde(default)]
  pub metadata: BTreeMap<String, String>,
}

impl Commit {
  fn validate(&self) -> Result<(), ProtocolError> {
    field("commit revision", &self.revision)?;
    if self.parents.len() > usize::from(MAX_PAGE_ENTRIES) {
      return Err(ProtocolError::LimitExceeded("commit parents"));
    }
    for parent in &self.parents {
      field("commit parent", parent)?;
    }
    bounded_text("commit message", &self.message, MAX_TEXT_BYTES)?;
    if let Some(author) = &self.author_display {
      bounded_text("author display", author, MAX_TEXT_BYTES)?;
    }
    validate_metadata(&self.metadata)
  }
}

/// Normalized tree entry kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeEntryKind {
  /// Regular file-like entry.
  File,
  /// Directory-like entry.
  Directory,
  /// Symbolic-link entry returned as data, never followed by the server.
  Symlink,
  /// Provider-specific special entry represented without execution.
  Other,
}

/// One bounded repository tree entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TreeEntry {
  /// Repository-relative path.
  pub path: String,
  /// Normalized kind.
  pub kind: TreeEntryKind,
  /// Optional byte size known without reading content.
  pub size_bytes: Option<u64>,
}

impl TreeEntry {
  fn validate(&self) -> Result<(), ProtocolError> {
    repository_path("tree path", &self.path)
  }
}

/// One deterministic bounded page of repository tree entries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TreePage {
  /// Ordered tree entries.
  pub entries: Vec<TreeEntry>,
  /// Opaque continuation cursor when more entries remain.
  pub next_cursor: Option<String>,
}

impl TreePage {
  fn validate(&self) -> Result<(), ProtocolError> {
    if self.entries.len() > usize::from(MAX_PAGE_ENTRIES) {
      return Err(ProtocolError::LimitExceeded("tree page"));
    }
    for entry in &self.entries {
      entry.validate()?;
    }
    if let Some(cursor) = &self.next_cursor {
      field("tree cursor", cursor)?;
    }
    Ok(())
  }
}

/// Bounded file content returned as base64 without interpretation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileContent {
  /// Repository-relative path.
  pub path: String,
  /// Byte offset of this fragment.
  pub offset: u64,
  /// Base64 bytes for this fragment.
  pub content_base64: String,
  /// Whether more bytes remain.
  pub truncated: bool,
}

impl FileContent {
  fn validate(&self) -> Result<(), ProtocolError> {
    repository_path("file path", &self.path)?;
    if self.content_base64.len() > MAX_FILE_CONTENT_BASE64_BYTES {
      return Err(ProtocolError::LimitExceeded("file content bytes"));
    }
    let bytes = STANDARD
      .decode(&self.content_base64)
      .map_err(|_| ProtocolError::Invalid("file content is not valid base64"))?;
    if bytes.len() > MAX_FILE_CONTENT_BYTES as usize {
      return Err(ProtocolError::LimitExceeded("file content bytes"));
    }
    Ok(())
  }
}

/// Result of resolving one allowed reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedRevision {
  /// Original reference expression.
  pub reference: String,
  /// Exact immutable revision recorded by a Build.
  pub revision: String,
}

/// Stable VCS failure classes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
  /// Request is malformed or violates repository policy.
  InvalidRequest,
  /// Adapter does not support the operation.
  Unsupported,
  /// Repository or credential configuration cannot succeed on retry.
  Permanent,
  /// Provider failure permits bounded idempotent retry.
  Transient,
  /// Operation was cooperatively cancelled.
  Cancelled,
  /// Peer violated the negotiated protocol.
  ProtocolFault,
}

/// Bounded secret-free VCS failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
  /// Stable semantic class.
  pub class: FailureClass,
  /// Stable machine-readable code.
  pub code: String,
  /// Bounded diagnostic safe for durable retry records.
  pub diagnostic: String,
  /// Optional retry delay, valid only for transient failures.
  pub retry_after_ms: Option<u64>,
}

impl Failure {
  /// Validates failure bounds and retry semantics.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    field("failure code", &self.code)?;
    bounded_text("failure diagnostic", &self.diagnostic, MAX_TEXT_BYTES)?;
    if self.retry_after_ms.is_some() && self.class != FailureClass::Transient {
      return Err(ProtocolError::Invalid("only transient failures may request retry"));
    }
    Ok(())
  }
}

/// VCS protocol validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProtocolError {
  /// Peers have no compatible version.
  #[error("VCS protocol versions are incompatible")]
  IncompatibleVersion,
  /// A semantic invariant is invalid.
  #[error("invalid VCS protocol value: {0}")]
  Invalid(&'static str),
  /// A bounded value exceeds its limit.
  #[error("VCS protocol limit exceeded: {0}")]
  LimitExceeded(&'static str),
  /// Encoded JSON is malformed or contains unknown fields.
  #[error("malformed VCS protocol message")]
  MalformedMessage,
  /// A response does not match the request version or identifier.
  #[error("VCS protocol response does not correlate to its request")]
  CorrelationMismatch,
}

fn bounded_message(message: &[u8]) -> Result<(), ProtocolError> {
  adapter_core::require_message_size(message, MAX_VCS_MESSAGE_BYTES)
    .map_err(|_| ProtocolError::LimitExceeded("encoded message"))
}

fn field(name: &'static str, value: &str) -> Result<(), ProtocolError> {
  bounded_text(name, value, MAX_FIELD_BYTES)
}

fn repository_path(name: &'static str, value: &str) -> Result<(), ProtocolError> {
  field(name, value)?;
  if value.starts_with('/')
    || value.contains('\\')
    || value
      .split('/')
      .any(|component| component.is_empty() || component == "." || component == "..")
  {
    return Err(ProtocolError::Invalid(
      "repository path must be normalized and relative",
    ));
  }
  Ok(())
}

fn bounded_text(name: &'static str, value: &str, max: usize) -> Result<(), ProtocolError> {
  adapter_core::require_bounded_text(value, max).map_err(|failure| match failure {
    ValidationFailure::LimitExceeded => ProtocolError::LimitExceeded(name),
    _ => ProtocolError::Invalid(name),
  })
}

fn page(size: u16) -> Result<(), ProtocolError> {
  if size == 0 {
    return Err(ProtocolError::Invalid("page size"));
  }
  if size > MAX_PAGE_ENTRIES {
    return Err(ProtocolError::LimitExceeded("page size"));
  }
  Ok(())
}

fn sha256(value: &str) -> Result<(), ProtocolError> {
  adapter_core::require_sha256(value).map_err(|_| ProtocolError::Invalid("executable digest must be lowercase SHA-256"))
}

fn validate_metadata(values: &BTreeMap<String, String>) -> Result<(), ProtocolError> {
  if values.len() > MAX_METADATA_ENTRIES {
    return Err(ProtocolError::LimitExceeded("metadata entries"));
  }
  for (key, value) in values {
    field("metadata key", key)?;
    bounded_text("metadata value", value, MAX_TEXT_BYTES)?;
  }
  Ok(())
}

fn validate_outcome_for(command: &Command, outcome: &Outcome) -> Result<(), ProtocolError> {
  let valid = match (command, outcome) {
    (Command::ListReferences(request), Outcome::References(page)) => {
      page.entries.len() <= usize::from(request.page_size)
    }
    (Command::ReadCommit(request), Outcome::Commit(commit)) => commit.revision == request.revision,
    (Command::ListTree(request), Outcome::Tree(page)) => page.entries.len() <= usize::from(request.page_size),
    (Command::ReadFile(request), Outcome::File(file)) => {
      let decoded_length = STANDARD.decode(&file.content_base64).map(|content| content.len());
      file.path == request.path
        && file.offset == request.offset
        && decoded_length.is_ok_and(|length| length <= request.max_bytes as usize)
    }
    (Command::ResolveRevision(request), Outcome::Resolved(resolved)) => resolved.reference == request.reference,
    (Command::Cancel(request), Outcome::Acknowledged { operation_id }) => operation_id == &request.target_operation_id,
    (Command::Cancel(_), Outcome::Failure(failure)) => failure.class == FailureClass::Cancelled,
    (Command::Cancel(_), _) => false,
    (_, Outcome::Failure(_)) => true,
    _ => false,
  };
  if valid {
    Ok(())
  } else {
    Err(ProtocolError::Invalid(
      "response outcome does not match the requested VCS operation",
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const FIXTURES: [&str; 5] = [
    include_str!("../fixtures/list-references-v1.json"),
    include_str!("../fixtures/read-commit-v1.json"),
    include_str!("../fixtures/list-tree-v1.json"),
    include_str!("../fixtures/read-file-v1.json"),
    include_str!("../fixtures/resolve-revision-v1.json"),
  ];

  #[test]
  fn operation_fixtures_round_trip_and_nested_unknown_fields_are_rejected() {
    for fixture in FIXTURES {
      let request = decode_request(fixture.as_bytes()).unwrap();
      let expected: serde_json::Value = serde_json::from_str(fixture).unwrap();
      assert_eq!(serde_json::to_value(request).unwrap(), expected);
      let mut unknown = expected;
      unknown["command"]["payload"]["checkout"] = serde_json::json!(true);
      assert!(decode_request(&serde_json::to_vec(&unknown).unwrap()).is_err());
    }
  }

  #[test]
  fn canonical_decoders_enforce_total_size_semantics_and_correlation() {
    assert_eq!(
      decode_request(&vec![b' '; MAX_VCS_MESSAGE_BYTES + 1]),
      Err(ProtocolError::LimitExceeded("encoded message"))
    );
    let request = decode_request(include_str!("../fixtures/resolve-revision-v1.json").as_bytes()).unwrap();
    let response = Response {
      protocol_version: VCS_PROTOCOL_VERSION,
      request_id: "different-request".to_owned(),
      outcome: Outcome::Acknowledged {
        operation_id: "operation-01".to_owned(),
      },
    };
    assert_eq!(response.validate_for(&request), Err(ProtocolError::CorrelationMismatch));

    for path in ["/etc/passwd", "../secret", "src/../secret", r"src\\secret"] {
      assert!(repository_path("path", path).is_err(), "accepted unsafe path: {path}");
    }
  }

  #[test]
  fn incompatible_versions_cancellation_limits_and_failure_classes_are_enforced() {
    assert_eq!(
      ProtocolRange { min: 1, max: 1 }.negotiate(ProtocolRange { min: 2, max: 2 }),
      Err(ProtocolError::IncompatibleVersion)
    );
    Request {
      protocol_version: 1,
      request_id: "cancel-01".to_owned(),
      command: Command::Cancel(CancelOperation {
        target_operation_id: "operation-01".to_owned(),
      }),
    }
    .validate()
    .unwrap();
    assert!(page(MAX_PAGE_ENTRIES + 1).is_err());
    assert!(
      FileContent {
        path: "README.md".to_owned(),
        offset: 0,
        content_base64: "not base64".to_owned(),
        truncated: false,
      }
      .validate()
      .is_err()
    );
    assert!(
      Failure {
        class: FailureClass::InvalidRequest,
        code: "bad_revision".to_owned(),
        diagnostic: "revision is invalid".to_owned(),
        retry_after_ms: Some(1),
      }
      .validate()
      .is_err()
    );
  }

  #[test]
  fn validation_distinguishes_invalid_values_from_size_limits() {
    assert_eq!(field("revision", ""), Err(ProtocolError::Invalid("revision")));
    assert_eq!(
      field("revision", "line\nbreak"),
      Err(ProtocolError::Invalid("revision"))
    );
    assert_eq!(
      field("revision", &"x".repeat(MAX_FIELD_BYTES + 1)),
      Err(ProtocolError::LimitExceeded("revision"))
    );
    assert_eq!(page(0), Err(ProtocolError::Invalid("page size")));
    assert_eq!(
      page(MAX_PAGE_ENTRIES + 1),
      Err(ProtocolError::LimitExceeded("page size"))
    );
  }

  #[test]
  fn response_semantics_are_bounded_by_the_correlated_request() {
    let request = Request {
      protocol_version: VCS_PROTOCOL_VERSION,
      request_id: "request-01".to_owned(),
      command: Command::ReadFile(ReadFile {
        repository: RepositoryAccess {
          operation_id: "operation-01".to_owned(),
          repository_id: "repository-01".to_owned(),
          repository_locator: "https://example.invalid/repository.git".to_owned(),
          credential_handle: "secret:repository".to_owned(),
        },
        revision: "abc".to_owned(),
        path: "README.md".to_owned(),
        offset: 4,
        max_bytes: 4,
      }),
    };
    let response = Response {
      protocol_version: VCS_PROTOCOL_VERSION,
      request_id: request.request_id.clone(),
      outcome: Outcome::File(FileContent {
        path: "README.md".to_owned(),
        offset: 4,
        content_base64: STANDARD.encode(b"too large"),
        truncated: false,
      }),
    };
    assert!(matches!(
      response.validate_for(&request),
      Err(ProtocolError::Invalid(_))
    ));

    let debug = format!("{request:?}");
    assert!(!debug.contains("secret:repository"));
    assert!(!debug.contains("example.invalid"));
    assert!(debug.contains("<redacted>"));
  }
}
