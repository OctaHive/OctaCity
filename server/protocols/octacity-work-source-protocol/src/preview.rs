//! Read-only native summaries; no private body, mapped fields or event publication.
use crate::*;
use std::collections::BTreeSet;
/// UTF-8 display-summary ceiling, independent of private admission inputs.
pub const MAX_SUMMARY_BYTES: usize = 512;
/// Safe selector explanations; their spelling never selects a Factory route.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMismatch {
  /// Observed provider state fails the selected state predicate.
  StateMismatch,
  /// Labels were observed by a selector requiring an empty label set.
  LabelsPresent,
  /// At least one required literal label is absent.
  RequiredLabelMissing,
  /// No permitted alternative label is present.
  AlternativeLabelMissing,
  /// An excluded literal label is present.
  ExcludedLabelPresent,
  /// Another installed provider predicate did not match.
  PredicateMismatch,
}
/// Evaluation of one exact installed selector; empty reasons means a match.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
  /// Original installed selector, with no target authority.
  pub selection: ConfigurationReference,
  /// Classified failures only; provider error prose is forbidden.
  pub reasons: Vec<SelectionMismatch>,
}
/// Bounded display facts, independent of any Work, card or input identity.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
  /// Stable native item under the page's exact container.
  pub item_id: String,
  /// Human key, never an identity or route.
  pub display_id: String,
  /// Bounded untrusted display text, never an execution input.
  pub title: String,
  /// Literal observed state, not a shared provider enum.
  pub state: String,
  /// Bounded literal observed labels.
  pub labels: Vec<String>,
  /// One evaluation for every requested selector, including excluded candidates.
  pub selections: Vec<Selection>,
}
impl std::fmt::Debug for Candidate {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("PreviewCandidate").finish_non_exhaustive()
  }
}
/// A correlated preview page; continuation is private, never a production checkpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
  /// Original preview operation identity.
  pub scan_id: String,
  /// Verified native container.
  pub resource_id: String,
  /// Original installed profile pin.
  pub configuration_digest: String,
  /// Complete bounded summary page; empty success differs from provider failure.
  pub candidates: Vec<Candidate>,
  /// Private provider continuation, if more data exists.
  pub next_cursor: Option<String>,
}
impl Page {
  /// Rejects substituted scope, arbitrary reasons, private fields and response-bound violations.
  pub fn validate_for(&self, intent: &source::ScanIntent) -> Result<(), ProtocolError> {
    intent.validate()?;
    if self.scan_id != intent.scan_id
      || self.resource_id != intent.resource.resource_id
      || self.configuration_digest != intent.configuration_digest
      || self.candidates.len() > usize::from(intent.max_items)
    {
      return Err(ProtocolError::Invalid);
    }
    if let Some(cursor) = &self.next_cursor {
      common::require_bounded_text(cursor, source::MAX_CURSOR_BYTES).map_err(|_| ProtocolError::Invalid)?;
    }
    let expected: BTreeSet<_> = std::iter::once(&intent.selection)
      .chain(&intent.additional_selections)
      .map(|s| (&s.identity, &s.sha256))
      .collect();
    let mut seen = BTreeSet::new();
    for candidate in &self.candidates {
      field(&candidate.item_id)?;
      common::require_bounded_text(&candidate.display_id, 256).map_err(|_| ProtocolError::Invalid)?;
      common::require_bounded_text(&candidate.title, MAX_SUMMARY_BYTES).map_err(|_| ProtocolError::Invalid)?;
      common::require_bounded_text(&candidate.state, 128).map_err(|_| ProtocolError::Invalid)?;
      if !seen.insert(&candidate.item_id) || candidate.labels.len() > 64 || candidate.selections.len() != expected.len()
      {
        return Err(ProtocolError::Invalid);
      }
      let mut labels = BTreeSet::new();
      for label in &candidate.labels {
        common::require_bounded_text(label, 256).map_err(|_| ProtocolError::Invalid)?;
        if !labels.insert(label) {
          return Err(ProtocolError::Invalid);
        }
      }
      let mut actual = BTreeSet::new();
      for selection in &candidate.selections {
        selection.selection.validate()?;
        if !actual.insert((&selection.selection.identity, &selection.selection.sha256))
          || selection.reasons.iter().collect::<BTreeSet<_>>().len() != selection.reasons.len()
        {
          return Err(ProtocolError::Invalid);
        }
      }
      if actual != expected {
        return Err(ProtocolError::Invalid);
      }
    }
    Ok(())
  }
}
