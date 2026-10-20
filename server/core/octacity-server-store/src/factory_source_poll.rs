//! Private retained pages in the existing fenced scan record, independent of provider wire DTOs.
use octacity_server_factory::FactoryDigest;

/// Shared normalized page item ceiling, independent of provider service quotas.
pub const MAX_SOURCE_POLL_PAGE_ITEMS: u16 = 100;
/// Shared opaque continuation ceiling for normalized Source consumers.
pub const MAX_SOURCE_POLL_CURSOR_BYTES: usize = 4096;
/// Complete normalized private page ceiling.
pub const MAX_SOURCE_POLL_PAGE_BYTES: u32 = 4 * crate::MAX_SOURCE_INPUT_BYTES;

/// Immutable observation bytes remaining in a page, with its original opaque continuation.
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourcePendingPage {
  /// Application-normalized private snapshots, never executable configuration or authority.
  pub candidates: Vec<serde_json::Value>,
  /// Continuation becomes covered only when every candidate is settled.
  pub next_cursor: Option<String>,
  /// Fixed private-cache deadline, never extended by candidate retries.
  pub expires_at: octacity_server_domain::Timestamp,
}
/// Restart-safe details accompanying the existing covered scan position.
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourcePollingState {
  /// The other lane's exact context while one incremental/reconciliation page uses capacity.
  pub other_pass: Option<WorkSourcePollPass>,
  /// Trusted time at which a new full reconciliation becomes due.
  pub reconciliation_due_at: octacity_server_domain::Timestamp,
  /// Page-boundary scheduling choice, retained even when the invocation exhausts its budget.
  pub switch_after_page: bool,
  /// Stable provider pass identity, retained across lease owners and retries.
  pub scan_id: FactoryDigest,
  /// Whether the frozen provider pass reached its end.
  pub finished: bool,
  /// First complete page retained before any candidate effects.
  pub pending_page: Option<WorkSourcePendingPage>,
}
/// One suspended provider pass; its continuation never borrows another lane's scan identity/window.
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourcePollPass {
  /// Original provider operation identity.
  pub scan_id: FactoryDigest,
  /// Inclusive incremental lower bound, or epoch for full reconciliation.
  pub window_start: octacity_server_domain::Timestamp,
  /// Frozen incremental upper bound, or full pass start time.
  pub window_end: octacity_server_domain::Timestamp,
  /// Exact opaque page position.
  pub continuation: Option<String>,
  /// Whether this is the full-reconciliation lane.
  pub reconciliation: bool,
  /// Whether its last page had no continuation.
  pub finished: bool,
}
impl std::fmt::Debug for WorkSourcePollingState {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("WorkSourcePollingState")
      .field("scan_id", &self.scan_id)
      .field("finished", &self.finished)
      .field(
        "pending_page",
        &self.pending_page.as_ref().map(|page| page.candidates.len()),
      )
      .finish()
  }
}
impl WorkSourcePendingPage {
  /// Canonical normalized payload size, excluding persistence-only retention metadata.
  pub fn encoded_payload_bytes(&self) -> Result<usize, crate::StoreError> {
    #[derive(serde::Serialize)]
    struct Payload<'a> {
      candidates: &'a [serde_json::Value],
      next_cursor: &'a Option<String>,
    }
    serde_json::to_vec(&Payload {
      candidates: &self.candidates,
      next_cursor: &self.next_cursor,
    })
    .map(|v| v.len())
    .map_err(|_| crate::source_conflict())
  }
}
impl WorkSourcePollingState {
  /// Checks private data bounds and the two disjoint lane contexts before persistence.
  pub fn validate(&self, progress: &crate::WorkSourceProgress) -> Result<(), crate::StoreError> {
    if self.finished && (self.pending_page.is_some() || progress.continuation.is_some()) {
      return Err(crate::source_conflict());
    }
    if let Some(other) = &self.other_pass {
      if other.reconciliation == progress.reconciliation
        || other.window_start > other.window_end
        || (other.finished && other.continuation.is_some())
      {
        return Err(crate::source_conflict());
      }
      validate_source_cursor(&other.continuation)?;
    }
    if let Some(page) = &self.pending_page {
      validate_source_cursor(&page.next_cursor)?;
      if page.candidates.len() > usize::from(MAX_SOURCE_POLL_PAGE_ITEMS)
        || page.encoded_payload_bytes()? > MAX_SOURCE_POLL_PAGE_BYTES as usize
      {
        return Err(crate::source_conflict());
      }
      for candidate in &page.candidates {
        if serde_json::to_vec(candidate)
          .map_err(|_| crate::source_conflict())?
          .len()
          > crate::MAX_SOURCE_INPUT_BYTES as usize
        {
          return Err(crate::source_conflict());
        }
      }
    }
    Ok(())
  }
}
/// Shared byte/control-character validation before a Source continuation gains authority.
pub fn validate_source_cursor(value: &Option<String>) -> Result<(), crate::StoreError> {
  if value
    .as_ref()
    .is_some_and(|v| v.is_empty() || v.len() > MAX_SOURCE_POLL_CURSOR_BYTES || v.chars().any(char::is_control))
  {
    return Err(crate::source_conflict());
  }
  Ok(())
}
