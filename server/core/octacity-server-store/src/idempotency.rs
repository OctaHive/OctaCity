use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::StoreInputError;

/// Maximum UTF-8 bytes in a store mutation idempotency key.
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;

/// Bounded caller identity that makes one mutation safely replayable.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
  /// Constructs a key using visible, trimmed text within the declared bound.
  pub fn new(value: impl Into<String>) -> Result<Self, StoreInputError> {
    let value = value.into();
    if value.is_empty()
      || value.len() > MAX_IDEMPOTENCY_KEY_BYTES
      || value.trim() != value
      || value.chars().any(char::is_control)
    {
      return Err(StoreInputError::InvalidIdempotencyKey);
    }
    Ok(Self(value))
  }

  /// Borrows the validated key.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for IdempotencyKey {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for IdempotencyKey {
  type Err = StoreInputError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for IdempotencyKey {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for IdempotencyKey {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}
