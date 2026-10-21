//! Provider-neutral retained check facts; these grant no execution or acceptance authority.
use crate::*;
use octacity_server_domain::{ImmutableRevision, Timestamp};
use octacity_server_factory::FactorySafeText;
use serde::{Deserialize, Serialize};
/// Retained check lifecycle, separate from task status or acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCheckState {
  /// Declared, without a materialized observation.
  Missing,
  /// Waiting for execution.
  Pending,
  /// Execution is active.
  Running,
  /// The selected check reports success; its source determines proof authority.
  Pass,
  /// The selected check reports failure.
  Fail,
  /// Execution was cancelled.
  Cancelled,
  /// Check was skipped; this is not a pass.
  Skipped,
  /// Observations do not establish an outcome.
  Indeterminate,
  /// Independently protected retained detail cannot be read.
  Unavailable,
}

/// External observations remain explicitly distinct from trusted Factory evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueExternalCheckSource {
  /// Observed forge CI; never a Factory Decision.
  ExternalCi,
}
/// One normalized retained CI observation, excluding executable/provider content URLs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueExternalCheck {
  /// Original provider-scoped identity, safe display only.
  pub id: FactorySafeText,
  /// Original visible check name.
  pub name: FactorySafeText,
  /// Visible CI producer, if known.
  pub producer: Option<FactorySafeText>,
  /// Observational lifecycle; skipped/unknown never become successful.
  pub state: IssueCheckState,
  /// Absent means the provider did not establish required-check policy.
  pub required: Option<bool>,
}
/// Bounded original-head CI retained atomically with the existing review observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueExternalChecks {
  /// Explicitly external, without acceptance authority.
  pub source: IssueExternalCheckSource,
  /// Original immutable head of every item.
  pub head: ImmutableRevision,
  /// Original observation time, not a read freshness claim.
  pub observed_at: Timestamp,
  /// Further provider facts were not retained in this bounded observation.
  pub has_more: bool,
  /// At most the existing diagnostic page ceiling; no provider cursor is exposed.
  pub items: Vec<IssueExternalCheck>,
}
impl IssueExternalChecks {
  pub(crate) fn validate(&self) -> Result<(), StoreError> {
    if self.items.len() > usize::from(MAX_FACTORY_RUN_DIAGNOSTIC_PAGE_SIZE)
      || self
        .items
        .iter()
        .enumerate()
        .any(|(index, item)| self.items[..index].iter().any(|prior| prior.id == item.id))
    {
      return Err(crate::issue_code_review::invalid_issue_code_review());
    }
    Ok(())
  }
}
