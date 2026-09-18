use std::{collections::BTreeSet, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::PipelineError;

/// Maximum ASCII bytes in one execution-capability identity.
pub const MAX_EXECUTION_CAPABILITY_BYTES: usize = 64;

/// Stable provider-neutral execution capability referenced by a Pipeline Job.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionCapability(String);

impl ExecutionCapability {
  /// Constructs a capability using `[a-z0-9][a-z0-9._-]*`.
  pub fn new(value: impl Into<String>) -> Result<Self, PipelineError> {
    let value = value.into();
    let mut characters = value.chars();
    let valid = !value.is_empty()
      && value.len() <= MAX_EXECUTION_CAPABILITY_BYTES
      && characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
      && characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '.' | '_' | '-')
      });
    if !valid {
      return Err(PipelineError::InvalidCapability);
    }
    Ok(Self(value))
  }

  /// Borrows the canonical capability identity.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for ExecutionCapability {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for ExecutionCapability {
  type Err = PipelineError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for ExecutionCapability {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for ExecutionCapability {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// Capabilities that the current server release permits Pipeline versions to request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilityCatalog(BTreeSet<ExecutionCapability>);

impl CapabilityCatalog {
  /// Creates a deterministic catalog from validated capability identities.
  #[must_use]
  pub fn new(capabilities: impl IntoIterator<Item = ExecutionCapability>) -> Self {
    Self(capabilities.into_iter().collect())
  }

  /// Reports whether one capability may be referenced by a new Pipeline version.
  #[must_use]
  pub fn supports(&self, capability: &ExecutionCapability) -> bool {
    self.0.contains(capability)
  }
}
