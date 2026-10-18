//! GitHub-owned typed Issue selection; no provider predicates enter the Flow interpreter.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Local maximum labels in each all/any/excluded predicate, independent of provider quotas.
pub const MAX_SELECTOR_LABELS_PER_PREDICATE: usize = 32;
/// Local UTF-8 byte bound for one configured label name.
pub const MAX_SELECTOR_LABEL_BYTES: usize = 256;
/// Maximum raw JSON configuration bytes accepted by the checked decoder.
pub const MAX_SELECTOR_CONFIG_BYTES: usize = 64 * 1024;

/// Actual Issue state observed by the provider adapter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueState {
  /// Open Issue.
  Open,
  /// Closed Issue.
  Closed,
}

/// Independently configured Issue-state predicate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueStateSelection {
  /// Select open Issues.
  Open,
  /// Select closed Issues.
  Closed,
  /// Select either actual Issue state.
  All,
}

/// Closed typed selector definition owned by the GitHub adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSelectorDraft {
  /// Explicit state selection, independent of label predicates.
  pub state: IssueStateSelection,
  /// Every configured label must be observed.
  pub all_of: Vec<String>,
  /// At least one configured alternative must be observed when nonempty.
  pub any_of: Vec<String>,
  /// No configured excluded label may be observed.
  pub none_of: Vec<String>,
}

/// Closed selector field names safe to expose in validation diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectorField {
  /// Whole configuration document, before typed field validation.
  Selector,
  /// Required labels.
  AllOf,
  /// Alternative labels.
  AnyOf,
  /// Excluded labels.
  NoneOf,
}

/// Stable validation reasons without raw label or provider content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectorRejection {
  /// The configuration is malformed or uses an unsupported shape/state/predicate.
  Malformed,
  /// A configured collection or document exceeds its local safety bound.
  LimitExceeded,
  /// A label is empty, whitespace-only, oversized or contains control characters.
  InvalidLabel,
  /// A predicate repeats a label name.
  Duplicate,
  /// The predicates cannot be satisfied by any observed label set.
  Contradictory,
}

/// Safe rejection of a selector definition, without provider content.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid GitHub Issue selector: {field:?} ({reason:?})")]
pub struct SelectorError {
  /// Stable field classification.
  pub field: SelectorField,
  /// Stable rejection classification.
  pub reason: SelectorRejection,
}

/// Immutable selector checked before discovery or admission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IssueSelector {
  state: IssueStateSelection,
  all_of: Vec<String>,
  any_of: Vec<String>,
  none_of: Vec<String>,
}

impl IssueSelector {
  /// Decodes bounded configuration and returns only safe field/reason errors.
  pub fn from_config(bytes: &[u8]) -> Result<Self, SelectorError> {
    if bytes.len() > MAX_SELECTOR_CONFIG_BYTES {
      return Err(SelectorError {
        field: SelectorField::Selector,
        reason: SelectorRejection::LimitExceeded,
      });
    }
    let draft = serde_json::from_slice(bytes).map_err(|_| SelectorError {
      field: SelectorField::Selector,
      reason: SelectorRejection::Malformed,
    })?;
    Self::new(draft)
  }

  /// Validates a complete typed selector without provider access or side effects.
  pub fn new(draft: IssueSelectorDraft) -> Result<Self, SelectorError> {
    for (field, labels) in [
      (SelectorField::AllOf, &draft.all_of),
      (SelectorField::AnyOf, &draft.any_of),
      (SelectorField::NoneOf, &draft.none_of),
    ] {
      if labels.len() > MAX_SELECTOR_LABELS_PER_PREDICATE {
        return Err(SelectorError {
          field,
          reason: SelectorRejection::LimitExceeded,
        });
      }
      if labels.iter().any(|label| {
        label.trim().is_empty() || label.len() > MAX_SELECTOR_LABEL_BYTES || label.chars().any(char::is_control)
      }) {
        return Err(SelectorError {
          field,
          reason: SelectorRejection::InvalidLabel,
        });
      }
      if labels
        .iter()
        .map(|label| label.to_lowercase())
        .collect::<BTreeSet<_>>()
        .len()
        != labels.len()
      {
        return Err(SelectorError {
          field,
          reason: SelectorRejection::Duplicate,
        });
      }
    }
    if draft.all_of.iter().any(|label| contains_label(&draft.none_of, label)) {
      return Err(SelectorError {
        field: SelectorField::NoneOf,
        reason: SelectorRejection::Contradictory,
      });
    }
    if !draft.any_of.is_empty() && draft.any_of.iter().all(|label| contains_label(&draft.none_of, label)) {
      return Err(SelectorError {
        field: SelectorField::AnyOf,
        reason: SelectorRejection::Contradictory,
      });
    }
    Ok(Self {
      state: draft.state,
      all_of: draft.all_of,
      any_of: draft.any_of,
      none_of: draft.none_of,
    })
  }

  /// Evaluates the locally observed Issue facts rather than a provider search result.
  pub fn matches(&self, state: IssueState, labels: &[String]) -> bool {
    self.mismatches(state, labels).is_empty()
  }

  /// Explains the same validated local predicate without exposing private task content.
  pub(crate) fn mismatches(
    &self,
    state: IssueState,
    labels: &[String],
  ) -> Vec<octacity_work_source_protocol::preview::SelectionMismatch> {
    use octacity_work_source_protocol::preview::SelectionMismatch as Reason;
    let mut reasons = Vec::new();
    if !matches!(
      (self.state, state),
      (IssueStateSelection::All, _)
        | (IssueStateSelection::Open, IssueState::Open)
        | (IssueStateSelection::Closed, IssueState::Closed)
    ) {
      reasons.push(Reason::StateMismatch);
    }
    if !self.all_of.iter().all(|label| contains_label(labels, label)) {
      reasons.push(Reason::RequiredLabelMissing);
    }
    if !self.any_of.is_empty() && !self.any_of.iter().any(|label| contains_label(labels, label)) {
      reasons.push(Reason::AlternativeLabelMissing);
    }
    if self.none_of.iter().any(|label| contains_label(labels, label)) {
      reasons.push(Reason::ExcludedLabelPresent);
    }
    reasons
  }
}

fn contains_label(labels: &[String], expected: &str) -> bool {
  let expected = expected.to_lowercase();
  labels.iter().any(|label| label.to_lowercase() == expected)
}
