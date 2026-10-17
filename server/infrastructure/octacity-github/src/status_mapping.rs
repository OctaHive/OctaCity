//! GitHub-owned desired effects; Factory statuses and the Work protocol remain provider neutral.
use crate::issue_selector::IssueState;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// Closed, secret-free rejection of an installed status mapping or value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid GitHub status mapping or unsupported value")]
pub struct MappingError;

/// Explicit effects on managed labels and optional Issue state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StatusEffects {
  add: Vec<String>,
  remove: Vec<String>,
  state: Option<IssueState>,
}
impl StatusEffects {
  /// Literal labels to add while preserving every other label.
  pub fn add(&self) -> &[String] {
    &self.add
  }
  /// Literal managed labels to remove, without clearing a group implicitly.
  pub fn remove(&self) -> &[String] {
    &self.remove
  }
  /// Optional explicit Issue state; absence preserves current state.
  pub fn state(&self) -> Option<IssueState> {
    self.state
  }
}

/// Immutable adapter-owned map of arbitrary logical values to native effects.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MappingDraft {
  config_version: u16,
  #[serde(deserialize_with = "unique_map")]
  managed_label_groups: BTreeMap<String, Vec<String>>,
  #[serde(deserialize_with = "unique_map")]
  statuses: BTreeMap<String, StatusEffects>,
}
fn unique_map<'de, D, V>(deserializer: D) -> Result<BTreeMap<String, V>, D::Error>
where
  D: serde::Deserializer<'de>,
  V: Deserialize<'de>,
{
  struct Unique<V>(std::marker::PhantomData<V>);
  impl<'de, V: Deserialize<'de>> serde::de::Visitor<'de> for Unique<V> {
    type Value = BTreeMap<String, V>;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      formatter.write_str("unique mapping keys")
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
      let mut result = BTreeMap::new();
      while let Some((key, value)) = access.next_entry()? {
        if result.insert(key, value).is_some() {
          return Err(serde::de::Error::custom("duplicate mapping key"));
        }
      }
      Ok(result)
    }
  }
  deserializer.deserialize_map(Unique(std::marker::PhantomData))
}
/// Immutable checked mapping; native effects cannot be deserialized around validation.
#[derive(Clone, Debug)]
pub struct StatusMapping {
  statuses: BTreeMap<String, StatusEffects>,
  groups: BTreeMap<String, Vec<String>>,
}
impl StatusMapping {
  /// Decodes a versioned installed mapping without reading credentials or calling GitHub.
  pub fn from_config(bytes: &[u8]) -> Result<Self, MappingError> {
    if bytes.len() > 64 * 1024 {
      return Err(MappingError);
    }
    let mapping: MappingDraft = serde_json::from_slice(bytes).map_err(|_| MappingError)?;
    if mapping.config_version != 1
      || mapping.managed_label_groups.len() > 32
      || mapping.statuses.is_empty()
      || mapping.statuses.len() > 128
    {
      return Err(MappingError);
    }
    let mut managed = BTreeSet::new();
    for (group, labels) in &mapping.managed_label_groups {
      if !bounded(group) || labels.is_empty() || labels.len() > 64 {
        return Err(MappingError);
      }
      for label in labels {
        if !bounded(label) || matches!(label.as_str(), "." | "..") || !managed.insert(label.to_lowercase()) {
          return Err(MappingError);
        }
      }
    }
    if managed.len() > 128 {
      return Err(MappingError);
    }
    for (status, effect) in &mapping.statuses {
      if !bounded(status) || effect.add.len() > 64 || effect.remove.len() > 64 {
        return Err(MappingError);
      }
      let mut affected = BTreeSet::new();
      for label in effect.add.iter().chain(&effect.remove) {
        let normalized = label.to_lowercase();
        if !bounded(label) || !managed.contains(&normalized) || !affected.insert(normalized) {
          return Err(MappingError);
        }
      }
    }
    Ok(Self {
      statuses: mapping.statuses,
      groups: mapping.managed_label_groups,
    })
  }
  /// Resolves only an exact configured value, without a built-in business vocabulary.
  pub fn effects(&self, status: &str) -> Result<&StatusEffects, MappingError> {
    self.statuses.get(status).ok_or(MappingError)
  }
  pub(crate) fn managed_groups(&self) -> impl Iterator<Item = &Vec<String>> {
    self.groups.values()
  }
}
fn bounded(value: &str) -> bool {
  !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
