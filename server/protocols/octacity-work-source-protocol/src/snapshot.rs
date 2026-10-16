//! Bounded private normalized ticket data, never provider DTOs or authority.
use crate::*;
use std::collections::BTreeMap;

/// Hard per-candidate encoded ceiling, matching private Factory input limits.
pub const MAX_SNAPSHOT_BYTES: usize = 1_048_576;
/// Hard title ceiling in UTF-8 bytes.
pub const MAX_TITLE_BYTES: usize = 4096;
/// Hard body or mapped-field ceiling in UTF-8 bytes.
pub const MAX_BODY_BYTES: usize = 262_144;
/// Normalized immutable candidate, with no URLs, credentials or fetch instructions.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
  /// Stable native ticket container, verified against the scan's installed resource.
  pub container_id: String,
  /// Stable native item identity; independent of title, labels and update time.
  pub item_id: String,
  /// Human-facing number or key used only as provenance.
  pub display_id: String,
  /// Opaque observed update identity, not a deduplication key.
  pub update_identity: String,
  /// Untrusted private task title.
  pub title: String,
  /// Optional untrusted private body; absence is preserved.
  pub body: Option<String>,
  /// Explicitly configured normalized fields, including tracker-specific acceptance data.
  pub fields: BTreeMap<String, String>,
  /// Literal observed labels with no business-stage meaning.
  pub labels: Vec<String>,
  /// Literal provider state; no shared forge/workflow enum exists.
  pub state: String,
}
impl std::fmt::Debug for Candidate {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Candidate").finish_non_exhaustive()
  }
}
impl Candidate {
  /// Validates one normalized private snapshot without interpreting its text.
  pub fn validate(&self, container: &str, maximum_bytes: usize) -> Result<(), ProtocolError> {
    for value in [&self.item_id, &self.display_id, &self.update_identity, &self.state] {
      field(value)?;
    }
    if self.container_id != container
      || self.display_id.len() > 256
      || self.update_identity.len() > 256
      || self.state.len() > 128
      || self.fields.len() > 32
      || self.labels.len() > 64
    {
      return Err(ProtocolError::Invalid);
    }
    common::require_bounded_text(&self.title, MAX_TITLE_BYTES).map_err(|_| ProtocolError::Invalid)?;
    if let Some(body) = &self.body {
      private_text(body, MAX_BODY_BYTES)?;
    }
    for (name, value) in &self.fields {
      common::require_bounded_text(name, 128).map_err(|_| ProtocolError::Invalid)?;
      private_text(value, MAX_BODY_BYTES)?;
    }
    let mut labels = std::collections::BTreeSet::new();
    for label in &self.labels {
      common::require_bounded_text(label, 256).map_err(|_| ProtocolError::Invalid)?;
      if !labels.insert(label) {
        return Err(ProtocolError::Invalid);
      }
    }
    let bytes = serde_json::to_vec(self).map_err(|_| ProtocolError::Invalid)?;
    common::require_message_size(&bytes, maximum_bytes.min(MAX_SNAPSHOT_BYTES))
      .map_err(|_| ProtocolError::LimitExceeded)
  }
}
fn private_text(value: &str, maximum: usize) -> Result<(), ProtocolError> {
  if value.len() > maximum
    || value
      .chars()
      .any(|c| c.is_control() && c != '\n' && c != '\t' && c != '\r')
  {
    return Err(ProtocolError::Invalid);
  }
  Ok(())
}
