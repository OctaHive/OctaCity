use serde::{Deserialize, Serialize};

/// Published logical output returned by the management API.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactResource {
  /// Stable logical Artifact identity.
  pub id: String,
  /// Owning Build identity.
  pub build_id: String,
  /// Producing Attempt identity.
  pub attempt_id: String,
  /// Producing Job identity.
  pub job_id: String,
  /// User-visible logical name.
  pub name: String,
  /// Artifact or report semantics.
  pub output_type: ArtifactOutputType,
  /// Logical media type preserved from the output declaration.
  pub media_type: String,
  /// Exact immutable byte length.
  pub size_bytes: u64,
  /// Lowercase SHA-256 content identity.
  pub sha256: String,
  /// Authoritative publication time.
  pub published_at_unix_ms: i64,
}

/// Logical output semantics with an open report-format string.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactOutputType {
  /// User-visible file or archive.
  Artifact,
  /// Machine-readable report.
  Report {
    /// Plugin-owned format identifier.
    format: String,
  },
}

/// Bounded published output list for one Build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactPage {
  /// Published logical outputs.
  pub items: Vec<ArtifactResource>,
}

/// Published output plus a short-lived opaque download capability.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDownload {
  /// Safe logical output metadata.
  pub artifact: ArtifactResource,
  /// Opaque short-lived GET URL.
  pub get_url: String,
  /// Unix millisecond at which the capability expires.
  pub expires_at_unix_ms: i64,
}

impl std::fmt::Debug for ArtifactDownload {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ArtifactDownload")
      .field("artifact", &self.artifact)
      .field("get_url", &"<redacted>")
      .field("expires_at_unix_ms", &self.expires_at_unix_ms)
      .finish()
  }
}
