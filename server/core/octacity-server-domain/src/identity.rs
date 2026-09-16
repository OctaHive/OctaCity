use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{DomainValueError, TextErrorKind};

/// Maximum UTF-8 bytes in a stable Trigger deduplication identity.
pub const MAX_TRIGGER_IDENTITY_BYTES: usize = 256;
/// Maximum ASCII bytes in a Pipeline node identity.
pub const MAX_PIPELINE_NODE_ID_BYTES: usize = 128;

/// Stable source-scoped identity used to deduplicate Trigger occurrences.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TriggerIdentity(String);

impl TriggerIdentity {
  /// Constructs a bounded stable Trigger identity.
  pub fn new(value: impl Into<String>) -> Result<Self, DomainValueError> {
    let value = value.into();
    let reason = if value.is_empty() {
      Some(TextErrorKind::Empty)
    } else if value.len() > MAX_TRIGGER_IDENTITY_BYTES {
      Some(TextErrorKind::TooLong)
    } else if value.trim() != value {
      Some(TextErrorKind::SurroundingWhitespace)
    } else if value.chars().any(char::is_control) {
      Some(TextErrorKind::ControlCharacter)
    } else {
      None
    };
    match reason {
      Some(reason) => Err(DomainValueError::InvalidTriggerIdentity { reason }),
      None => Ok(Self(value)),
    }
  }

  /// Borrows the stable Trigger identity.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }

  /// Returns the owned stable Trigger identity.
  #[must_use]
  pub fn into_inner(self) -> String {
    self.0
  }
}

impl fmt::Display for TriggerIdentity {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for TriggerIdentity {
  type Err = DomainValueError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for TriggerIdentity {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for TriggerIdentity {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// Stable identity of one node within an immutable Pipeline version.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PipelineNodeId(String);

impl PipelineNodeId {
  /// Constructs a Pipeline node identity using `[A-Za-z0-9][A-Za-z0-9._-]*`.
  pub fn new(value: impl Into<String>) -> Result<Self, DomainValueError> {
    let value = value.into();
    let mut characters = value.chars();
    let reason = if value.is_empty() {
      Some(TextErrorKind::Empty)
    } else if value.len() > MAX_PIPELINE_NODE_ID_BYTES {
      Some(TextErrorKind::TooLong)
    } else if !characters
      .next()
      .is_some_and(|character| character.is_ascii_alphanumeric())
    {
      Some(TextErrorKind::InvalidStart)
    } else if !characters.all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')) {
      Some(TextErrorKind::InvalidCharacter)
    } else {
      None
    };
    match reason {
      Some(reason) => Err(DomainValueError::InvalidPipelineNodeIdentity { reason }),
      None => Ok(Self(value)),
    }
  }

  /// Borrows the Pipeline node identity.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }

  /// Returns the owned Pipeline node identity.
  #[must_use]
  pub fn into_inner(self) -> String {
    self.0
  }
}

impl fmt::Display for PipelineNodeId {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for PipelineNodeId {
  type Err = DomainValueError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for PipelineNodeId {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for PipelineNodeId {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}
