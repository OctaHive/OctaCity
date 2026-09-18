use std::{fmt, net::IpAddr, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

/// Maximum UTF-8 bytes in a provider-neutral VCS source reference.
pub const MAX_SOURCE_REFERENCE_BYTES: usize = 1_024;
/// Maximum bytes in a normalized DNS host name.
pub const MAX_NETWORK_HOST_BYTES: usize = 253;

/// Provider-neutral mutable source reference such as a branch or tag.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceReference(String);

impl SourceReference {
  /// Constructs one bounded visible source reference.
  pub fn new(value: impl Into<String>) -> Result<Self, SourceReferenceError> {
    let value = value.into();
    if value.is_empty()
      || value.len() > MAX_SOURCE_REFERENCE_BYTES
      || value.trim() != value
      || value.chars().any(char::is_control)
    {
      return Err(SourceReferenceError);
    }
    Ok(Self(value))
  }

  /// Borrows the reference text exactly as supplied by the VCS provider.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for SourceReference {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for SourceReference {
  type Err = SourceReferenceError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for SourceReference {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for SourceReference {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// A source reference is empty, ambiguous, contains control data, or is too long.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("source reference is invalid")]
pub struct SourceReferenceError;

/// Exact network host allowed by a Build configuration.
///
/// Values are either IP addresses or canonical lowercase DNS names. Schemes,
/// paths, ports, and wildcard labels are intentionally not part of this type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NetworkHost(String);

impl NetworkHost {
  /// Parses and canonicalizes one exact IP address or DNS host name.
  pub fn new(value: impl Into<String>) -> Result<Self, NetworkHostError> {
    let value = value.into();
    if let Ok(address) = value.parse::<IpAddr>() {
      return Ok(Self(address.to_string()));
    }
    if value.is_empty()
      || value.len() > MAX_NETWORK_HOST_BYTES
      || !value.is_ascii()
      || value.ends_with('.')
      || value.split('.').any(|label| {
        label.is_empty()
          || label.len() > 63
          || label.starts_with('-')
          || label.ends_with('-')
          || !label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
      })
    {
      return Err(NetworkHostError);
    }
    Ok(Self(value.to_ascii_lowercase()))
  }

  /// Borrows the canonical host text.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for NetworkHost {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for NetworkHost {
  type Err = NetworkHostError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for NetworkHost {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for NetworkHost {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// A network host is neither a canonical IP address nor a valid DNS host name.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("network host is invalid")]
pub struct NetworkHostError;
