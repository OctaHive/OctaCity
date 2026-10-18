//! Immutable logical status effects; installed adapters retain native authority and interpretation.
use crate::FactoryError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Maximum configured logical statuses in one input version.
pub const MAX_SOURCE_REPORTING_STATUSES: usize = 128;
/// Maximum labels or state choices advertised by one installed Reporter.
pub const MAX_SOURCE_REPORTING_CHOICES: usize = 128;
/// Maximum label additions or removals for one logical status.
pub const MAX_SOURCE_REPORTING_EFFECT_LABELS: usize = 64;
/// Maximum UTF-8 bytes in a literal status, label or state.
pub const MAX_SOURCE_REPORTING_TEXT_BYTES: usize = 256;
/// One logical status and its explicit effects, preserving unrelated labels.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceStatusEffects {
  /// Arbitrary logical value selected only by an accepted configured action.
  pub status: String,
  /// Literal labels to add.
  pub add: Vec<String>,
  /// Literal labels to remove.
  pub remove: Vec<String>,
  /// Opaque adapter-advertised state, absent to preserve it.
  pub state: Option<String>,
}
/// Input-owned mapping frozen with the original admitted Work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceStatusMapping {
  /// Explicit rows; repeated status values are rejected rather than overwritten.
  pub statuses: Vec<WorkSourceStatusEffects>,
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "source reporting mapping",
  }
}
fn text(value: &str) -> bool {
  !value.trim().is_empty() && value.len() <= MAX_SOURCE_REPORTING_TEXT_BYTES && !value.chars().any(char::is_control)
}
fn literals(values: &[String], max: usize) -> bool {
  values.len() <= max && values.iter().all(|v| text(v)) && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}
impl WorkSourceStatusMapping {
  /// Checks finite closed rows without interpreting any provider vocabulary.
  pub fn validate(&self) -> Result<(), FactoryError> {
    if self.statuses.is_empty()
      || self.statuses.len() > MAX_SOURCE_REPORTING_STATUSES
      || self.statuses.iter().map(|e| &e.status).collect::<BTreeSet<_>>().len() != self.statuses.len()
      || self.statuses.iter().any(|e| {
        !text(&e.status)
          || !literals(&e.add, MAX_SOURCE_REPORTING_EFFECT_LABELS)
          || !literals(&e.remove, MAX_SOURCE_REPORTING_EFFECT_LABELS)
          || e.add.iter().any(|l| e.remove.contains(l))
          || e.state.as_ref().is_some_and(|s| !text(s))
      })
      || serde_json::to_vec(self).map_err(|_| invalid())?.len() > 64 * 1024
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Resolves an exact configured value; no default status or inferred effect.
  pub fn effects(&self, status: &str) -> Option<&WorkSourceStatusEffects> {
    self.statuses.iter().find(|e| e.status == status)
  }
}
/// Explicit installed editing ceiling, not a credential or permission grant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceReportingPolicy {
  /// Only these installed literal managed labels may be affected.
  pub labels: Vec<String>,
  /// Only these adapter-owned states may be selected.
  pub states: Vec<String>,
}
impl WorkSourceReportingPolicy {
  /// Validates the installed bounded declaration.
  pub fn validate(&self) -> Result<(), FactoryError> {
    if !literals(&self.labels, MAX_SOURCE_REPORTING_CHOICES) || !literals(&self.states, MAX_SOURCE_REPORTING_CHOICES) {
      return Err(invalid());
    }
    Ok(())
  }
  /// Rechecks an immutable mapping against the installed scope before publication/dispatch.
  pub fn validate_mapping(&self, mapping: &WorkSourceStatusMapping) -> Result<(), FactoryError> {
    self.validate()?;
    mapping.validate()?;
    if mapping.statuses.iter().any(|e| {
      e.add.iter().chain(&e.remove).any(|l| !self.labels.contains(l))
        || e.state.as_ref().is_some_and(|s| !self.states.contains(s))
    }) {
      return Err(invalid());
    }
    Ok(())
  }
}
