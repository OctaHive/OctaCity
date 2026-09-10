#![doc = include_str!("../README.md")]

use std::{collections::BTreeMap, io, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};

/// Process protocol version implemented by this wire crate.
pub const SOURCE_PLUGIN_PROTOCOL_VERSION: u16 = 1;
/// Manifest format version understood by this wire crate.
pub const SOURCE_PLUGIN_MANIFEST_VERSION: u16 = 1;
/// Maximum encoded size of one newline-terminated command or message.
pub const MAX_SOURCE_FRAME_BYTES: usize = 1024 * 1024;
/// Maximum UTF-8 byte length of a request identifier.
pub const MAX_SOURCE_REQUEST_ID_BYTES: usize = 256;

/// Rejects request identifiers that are unsafe for logs and protocol routing.
pub fn validate_request_id(request_id: &str) -> Result<(), String> {
  if request_id.is_empty() || request_id.len() > MAX_SOURCE_REQUEST_ID_BYTES || request_id.chars().any(char::is_control)
  {
    return Err(format!(
      "request_id must contain 1 to {MAX_SOURCE_REQUEST_ID_BYTES} bytes without control characters"
    ));
  }
  Ok(())
}

/// Operator-installed source-plugin metadata.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePluginManifest {
  /// Version of the `plugin.toml` structure.
  pub manifest_version: u16,
  /// Stable logical provider name used by jobs and the registry directory.
  pub name: String,
  /// Plugin release version reported during the handshake.
  pub version: String,
  /// Oldest process protocol implemented by the executable.
  pub protocol_min: u16,
  /// Newest process protocol implemented by the executable.
  pub protocol_max: u16,
  /// Normalized path to the binary, relative to the plugin directory.
  pub executable: String,
  /// Lowercase SHA-256 digest of the executable.
  pub sha256: String,
  /// Supported targets in `os-arch` form.
  pub platforms: Vec<String>,
  /// Provider configuration owned by the agent operator.
  #[serde(default)]
  pub settings: BTreeMap<String, serde_json::Value>,
}

impl SourcePluginManifest {
  /// Deserializes a strict manifest, rejecting unknown fields.
  pub fn from_toml(contents: &str) -> Result<Self, toml::de::Error> {
    toml::from_str(contents)
  }
}

/// Commands sent by the agent to a source-plugin process.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceCommand {
  /// Starts the single materialization owned by this process.
  Materialize {
    protocol_version: u16,
    request_id: String,
    request: MaterializeRequest,
  },
  /// Requests cooperative termination of the correlated materialization.
  Cancel { request_id: String },
}

/// Provider-independent materialization request carried over the process protocol.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializeRequest {
  /// Absolute empty directory assigned by the agent.
  pub destination: String,
  /// Immutable provider-native revision expected after materialization.
  pub revision: String,
  /// Optional provider-native fetch hint; never the source identity.
  #[serde(default)]
  pub reference: Option<String>,
  /// Provider-specific values covered by the signed job.
  #[serde(default)]
  pub parameters: BTreeMap<String, serde_json::Value>,
  /// Operator-controlled settings copied from the verified plugin manifest.
  #[serde(default)]
  pub settings: BTreeMap<String, serde_json::Value>,
  /// Named paths to agent-provisioned credential files.
  #[serde(default)]
  pub credential_files: BTreeMap<String, String>,
  /// Maximum allowed size of the resulting workspace.
  pub max_workspace_bytes: u64,
}

impl MaterializeRequest {
  /// Validates provider-independent path, revision, credential, and size rules.
  pub fn validate(&self) -> Result<(), String> {
    if !Path::new(&self.destination).is_absolute() {
      return Err("destination must be an absolute path".to_owned());
    }
    if self.revision.trim().is_empty() {
      return Err("revision must not be empty".to_owned());
    }
    if self.max_workspace_bytes == 0 {
      return Err("max_workspace_bytes must be greater than zero".to_owned());
    }
    if self
      .credential_files
      .iter()
      .any(|(name, path)| name.trim().is_empty() || path.is_empty())
    {
      return Err("credential file names and paths must not be empty".to_owned());
    }
    Ok(())
  }
}

/// Lifecycle messages emitted by a source-plugin process.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceMessage {
  /// Declares process identity before the agent sends a request.
  Hello {
    protocol_version: u16,
    plugin_name: String,
    plugin_version: String,
  },
  /// Confirms ownership of a valid materialization request.
  Accepted { request_id: String },
  /// Reports a non-terminal lifecycle milestone.
  Progress { request_id: String, message: String },
  /// Reports a non-terminal warning or troubleshooting detail.
  Diagnostic { request_id: String, message: String },
  /// Reports successful materialization of the exact requested revision.
  Finished {
    request_id: String,
    revision: String,
    provenance: BTreeMap<String, String>,
  },
  /// Confirms cooperative cancellation.
  Cancelled { request_id: String },
  /// Reports a terminal provider or request failure.
  Error {
    request_id: Option<String>,
    message: String,
  },
}

/// Failure while reading one bounded source-protocol frame.
#[derive(Debug, Error)]
pub enum ReadFrameError {
  #[error("failed to read source-plugin frame: {0}")]
  Io(#[source] io::Error),
  #[error("source-plugin frame exceeds the {MAX_SOURCE_FRAME_BYTES}-byte limit")]
  TooLarge,
  #[error("source-plugin frame is not terminated by a newline")]
  Unterminated,
}

/// Reads one newline-delimited source-protocol frame without unbounded allocation.
pub async fn read_frame<R: AsyncRead + Unpin>(
  reader: &mut BufReader<R>,
  frame: &mut String,
) -> Result<usize, ReadFrameError> {
  frame.clear();
  let mut limited = (&mut *reader).take((MAX_SOURCE_FRAME_BYTES + 1) as u64);
  let read = limited.read_line(frame).await.map_err(ReadFrameError::Io)?;
  if read > MAX_SOURCE_FRAME_BYTES {
    return Err(ReadFrameError::TooLarge);
  }
  if read != 0 && !frame.ends_with('\n') {
    return Err(ReadFrameError::Unterminated);
  }
  Ok(read)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_unknown_protocol_fields() {
    let json = r#"{"type":"cancel","request_id":"request-1","command":"bad"}"#;
    assert!(serde_json::from_str::<SourceCommand>(json).is_err());
  }

  #[test]
  fn manifest_rejects_unknown_fields() {
    let manifest = r#"
manifest_version = 1
name = "git"
version = "0.1.0"
protocol_min = 1
protocol_max = 1
executable = "octacity-source-git"
sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
platforms = ["linux-x86_64"]
unknown = true
"#;
    assert!(SourcePluginManifest::from_toml(manifest).is_err());
  }

  #[tokio::test]
  async fn bounds_frames_without_a_newline() {
    let input = vec![b'a'; MAX_SOURCE_FRAME_BYTES + 1];
    let mut reader = BufReader::new(input.as_slice());
    let mut frame = String::new();
    assert!(matches!(
      read_frame(&mut reader, &mut frame).await,
      Err(ReadFrameError::TooLarge)
    ));
  }

  #[tokio::test]
  async fn rejects_an_unterminated_short_frame() {
    let mut reader = BufReader::new(&b"{}"[..]);
    let mut frame = String::new();
    assert!(matches!(
      read_frame(&mut reader, &mut frame).await,
      Err(ReadFrameError::Unterminated)
    ));
  }

  #[test]
  fn validates_materialization_limits() {
    let request = MaterializeRequest {
      destination: std::env::current_dir()
        .unwrap()
        .join("workspace")
        .to_string_lossy()
        .into_owned(),
      revision: "abc123".to_owned(),
      reference: None,
      parameters: BTreeMap::new(),
      settings: BTreeMap::new(),
      credential_files: BTreeMap::new(),
      max_workspace_bytes: 0,
    };
    assert_eq!(
      request.validate().unwrap_err(),
      "max_workspace_bytes must be greater than zero"
    );
  }

  #[test]
  fn validates_request_ids() {
    assert!(validate_request_id("job-1/attempt-1").is_ok());
    assert!(validate_request_id("").is_err());
    assert!(validate_request_id("job\n1").is_err());
    assert!(validate_request_id(&"x".repeat(MAX_SOURCE_REQUEST_ID_BYTES + 1)).is_err());
  }
}
