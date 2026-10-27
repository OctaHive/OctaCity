//! Bounded narrowing of adapter-normalized task observations; no native provider DTOs or I/O.
use crate::{FactoryError, FactoryKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
/// Maximum literal labels in each input predicate.
pub const MAX_WORK_SOURCE_PREDICATE_LABELS: usize = 64;
/// UTF-8 byte ceiling per literal label.
pub const MAX_WORK_SOURCE_PREDICATE_LABEL_BYTES: usize = 256;
/// UTF-8 ceiling for an opaque normalized Source state, independent of key syntax.
pub const MAX_WORK_SOURCE_PREDICATE_STATE_BYTES: usize = 128;
/// Restriction on the complete observed label set, independent of literal exclusions.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkSourceLabelPresence {
  /// Both labeled and unlabeled tasks are eligible; preserves legacy semantics.
  #[default]
  Any,
  /// Only tasks with an actually empty observed label set are eligible.
  None,
}
impl WorkSourceLabelPresence {
  /// Whether label presence is unrestricted, including the legacy omitted setting.
  #[must_use]
  pub fn is_any(&self) -> bool {
    *self == Self::Any
  }
}
/// Safe explanation of a normalized selection mismatch.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkSourcePredicateMismatch {
  /// The observed state differs from the selected state.
  StateMismatch,
  /// A task has labels when strictly unlabeled selection was requested.
  LabelsPresent,
  /// A required literal label is absent.
  RequiredLabelMissing,
  /// No configured alternative label is present.
  AlternativeLabelMissing,
  /// An excluded label is present.
  ExcludedLabelPresent,
  /// Another installed provider predicate did not match.
  PredicateMismatch,
}
/// Immutable literal narrowing. Intake retains canonical keys; native status translation can use opaque text.
/// The installed Source scope remains authoritative for either representation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourcePredicate<State = FactoryKey> {
  /// An advertised opaque normalized state; None means no extra state restriction.
  pub state: Option<State>,
  /// Omission and `any` retain the original canonical bytes and input identity.
  #[serde(default, skip_serializing_if = "WorkSourceLabelPresence::is_any")]
  pub label_presence: WorkSourceLabelPresence,
  /// Every literal label must be present.
  pub all_of: Vec<String>,
  /// At least one alternative must be present when nonempty.
  pub any_of: Vec<String>,
  /// None of these literal labels may be present.
  pub none_of: Vec<String>,
}
impl<State: AsRef<str>> WorkSourcePredicate<State> {
  /// Rejects unbounded, repeated, invalid and unsatisfiable predicates before publication.
  pub fn validate(&self) -> Result<(), FactoryError> {
    let invalid = || FactoryError::InvalidConfiguration {
      field: "source input predicate",
    };
    if self.state.as_ref().is_some_and(|s| {
      let s = s.as_ref();
      s.trim().is_empty() || s.len() > MAX_WORK_SOURCE_PREDICATE_STATE_BYTES || s.chars().any(char::is_control)
    }) {
      return Err(invalid());
    }
    for labels in [&self.all_of, &self.any_of, &self.none_of] {
      if labels.len() > MAX_WORK_SOURCE_PREDICATE_LABELS
        || labels.iter().any(|s| {
          s.trim().is_empty() || s.len() > MAX_WORK_SOURCE_PREDICATE_LABEL_BYTES || s.chars().any(char::is_control)
        })
        || labels.iter().collect::<BTreeSet<_>>().len() != labels.len()
      {
        return Err(invalid());
      }
    }
    if (self.label_presence == WorkSourceLabelPresence::None && (!self.all_of.is_empty() || !self.any_of.is_empty()))
      || self.all_of.iter().any(|s| self.none_of.contains(s))
      || (!self.any_of.is_empty() && self.any_of.iter().all(|s| self.none_of.contains(s)))
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Evaluates literal normalized data; empty predicates also match unlabeled tasks.
  pub fn mismatches(&self, state: &str, labels: &[String]) -> Vec<WorkSourcePredicateMismatch> {
    use WorkSourcePredicateMismatch::*;
    let mut reasons = vec![];
    if self.state.as_ref().is_some_and(|s| s.as_ref() != state) {
      reasons.push(StateMismatch);
    }
    if self.label_presence == WorkSourceLabelPresence::None && !labels.is_empty() {
      reasons.push(LabelsPresent);
    }
    if self.all_of.iter().any(|s| !labels.contains(s)) {
      reasons.push(RequiredLabelMissing);
    }
    if !self.any_of.is_empty() && !self.any_of.iter().any(|s| labels.contains(s)) {
      reasons.push(AlternativeLabelMissing);
    }
    if self.none_of.iter().any(|s| labels.contains(s)) {
      reasons.push(ExcludedLabelPresent);
    }
    reasons
  }
}
