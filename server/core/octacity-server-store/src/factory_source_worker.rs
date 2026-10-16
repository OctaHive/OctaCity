//! Safe, durable scheduling facts; no provider diagnostics, credentials or raw snapshots.
use octacity_server_domain::Timestamp;

/// Provider-neutral reason for a deferred or blocked source pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkSourceFailure {
  /// Required installed capability is unavailable.
  Unavailable,
  /// Provider explicitly requested a later retry.
  RateLimited,
  /// Temporary dependency failure.
  Transient,
  /// Original operation deadline elapsed.
  Timeout,
  /// Shutdown stopped the original pass.
  Cancelled,
  /// Ordinary Factory WIP is full; retry without spending a provider attempt.
  AdmissionCapacity,
  /// Expired private preparation awaits its bounded independent cleanup sweep.
  RetentionPending,
  /// Verified provider response violated its contract.
  ProtocolFault,
  /// Installed configuration or credentials require repair.
  InvalidConfiguration,
  /// Definition, selection or owner changed.
  Conflict,
}
/// Retained per-target scheduling, reset only for a different immutable definition.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceSchedule {
  /// Earliest next attempt (ignored when exhausted).
  pub retry_at: Timestamp,
  /// Consecutive retry-budget failures; shutdown and Factory capacity do not spend attempts.
  pub attempts: u16,
  /// Safe failure category, independent of ordinary server readiness.
  pub failure: Option<WorkSourceFailure>,
  /// Further attempts require an explicit definition replacement.
  pub exhausted: bool,
}

/// Last classified observation in the existing scan journal; no duplicate/admission authority.
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceSelectionRecord {
  /// Stable native ticket identity; private scan state, not a log field.
  pub item: crate::WorkSourceItem,
  /// Zero is filtered, one is selected, several are ambiguous under this exact scan target.
  pub matching_inputs: Vec<octacity_server_factory::WorkSourceInputId>,
}
impl std::fmt::Debug for WorkSourceSelectionRecord {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("WorkSourceSelectionRecord")
      .field("matches", &self.matching_inputs.len())
      .finish_non_exhaustive()
  }
}
/// Applies provider selection facts only to current enabled members under a live scan fence.
pub fn record_source_selection(
  scan: &mut crate::WorkSourceScan,
  claim: &crate::WorkSourceScanClaim,
  now: Timestamp,
  members: Vec<octacity_server_factory::WorkSourceInput>,
  item: crate::WorkSourceItem,
  matched: Vec<octacity_server_factory::WorkSourceInputId>,
) -> Result<octacity_server_factory::WorkSourceSelection, crate::StoreError> {
  use octacity_server_factory::WorkSourceSelection;
  crate::require_source_scan(scan, claim, now)?;
  let mut unique = std::collections::BTreeSet::new();
  if matched.len() > 64
    || matched
      .iter()
      .any(|id| !unique.insert(*id) || !members.iter().any(|m| m.id() == *id && m.definition().enabled))
  {
    return Err(crate::source_conflict());
  }
  let result = match matched.as_slice() {
    [] => WorkSourceSelection::Filtered,
    [id] => WorkSourceSelection::Selected(Box::new(
      members
        .into_iter()
        .find(|m| m.id() == *id)
        .ok_or_else(crate::source_conflict)?,
    )),
    _ => WorkSourceSelection::Ambiguous(matched.clone()),
  };
  scan.last_selection = Some(WorkSourceSelectionRecord {
    item,
    matching_inputs: matched,
  });
  Ok(result)
}
