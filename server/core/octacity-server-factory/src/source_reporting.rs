//! Immutable logical status effects; installed adapters retain native authority and interpretation.
use crate::{FactoryError, FactoryKey};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum configured logical statuses in one input version.
pub const MAX_SOURCE_REPORTING_STATUSES: usize = 128;
/// Maximum labels or state choices advertised by one installed Reporter.
pub const MAX_SOURCE_REPORTING_CHOICES: usize = 128;
/// Maximum label additions or removals for one logical status.
pub const MAX_SOURCE_REPORTING_EFFECT_LABELS: usize = 64;
/// Maximum UTF-8 bytes in a literal status, label or state.
pub const MAX_SOURCE_REPORTING_TEXT_BYTES: usize = 256;
/// Maximum encoded bytes of installed native ownership metadata.
pub const MAX_SOURCE_REPORTING_OWNERSHIP_BYTES: usize = 64 * 1024;
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
  /// Installed native field scopes. Absence means unknown ownership, not independent labels.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ownership: Option<WorkSourceReportingOwnership>,
}

/// Provider-declared field identities, stable across profiles for the same native item.
/// Several literal labels may replace the same managed group. This metadata grants no authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceReportingOwnership {
  /// Every advertised label names all native scopes that its mutation may replace.
  pub labels: BTreeMap<String, BTreeSet<FactoryKey>>,
  /// Every advertised state replaces these same native scopes.
  pub state: BTreeSet<FactoryKey>,
}

impl WorkSourceReportingOwnership {
  /// Resolves a reviewed effect; the enclosing policy validates complete installed coverage.
  pub fn scopes(&self, effect: &WorkSourceStatusEffects) -> BTreeSet<FactoryKey> {
    effect
      .add
      .iter()
      .chain(&effect.remove)
      .filter_map(|label| self.labels.get(label))
      .flatten()
      .chain(effect.state.iter().flat_map(|_| &self.state))
      .cloned()
      .collect()
  }
}
impl WorkSourceReportingPolicy {
  /// Validates the installed bounded declaration.
  pub fn validate(&self) -> Result<(), FactoryError> {
    if !literals(&self.labels, MAX_SOURCE_REPORTING_CHOICES) || !literals(&self.states, MAX_SOURCE_REPORTING_CHOICES) {
      return Err(invalid());
    }
    if let Some(ownership) = &self.ownership
      && (ownership.labels.keys().collect::<BTreeSet<_>>() != self.labels.iter().collect()
        || ownership
          .labels
          .values()
          .any(|scopes| scopes.is_empty() || scopes.len() > MAX_SOURCE_REPORTING_CHOICES)
        || ownership.state.is_empty() != self.states.is_empty()
        || ownership.state.len() > MAX_SOURCE_REPORTING_CHOICES
        || serde_json::to_vec(ownership).map_err(|_| invalid())?.len() > MAX_SOURCE_REPORTING_OWNERSHIP_BYTES)
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Resolves native ownership without guessing from literal label names.
  /// An empty effect owns nothing, including for legacy policies without metadata.
  pub fn ownership_for(&self, effect: &WorkSourceStatusEffects) -> Result<Option<BTreeSet<FactoryKey>>, FactoryError> {
    self.validate_mapping(&WorkSourceStatusMapping {
      statuses: vec![effect.clone()],
    })?;
    if effect.add.is_empty() && effect.remove.is_empty() && effect.state.is_none() {
      return Ok(Some(BTreeSet::new()));
    }
    Ok(self.ownership.as_ref().map(|ownership| ownership.scopes(effect)))
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
