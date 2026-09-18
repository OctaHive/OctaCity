use std::{fmt, net::IpAddr, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

/// Maximum UTF-8 bytes in a provider-neutral VCS source reference.
pub const MAX_SOURCE_REFERENCE_BYTES: usize = 1_024;
/// Maximum UTF-8 bytes in an immutable provider-native source revision.
pub const MAX_IMMUTABLE_REVISION_BYTES: usize = 512;
/// Maximum UTF-8 bytes in a provider-owned Repository locator.
pub const MAX_REPOSITORY_LOCATOR_BYTES: usize = 2_048;
/// Maximum bytes in a normalized DNS host name.
pub const MAX_NETWORK_HOST_BYTES: usize = 253;

/// Exact immutable provider-native source revision selected for a Build.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImmutableRevision(String);

impl ImmutableRevision {
  /// Constructs a bounded, visible immutable revision.
  pub fn new(value: impl Into<String>) -> Result<Self, ImmutableRevisionError> {
    let value = value.into();
    if value.is_empty()
      || value.len() > MAX_IMMUTABLE_REVISION_BYTES
      || value.trim() != value
      || value.chars().any(char::is_control)
    {
      return Err(ImmutableRevisionError);
    }
    Ok(Self(value))
  }

  /// Borrows the provider-native immutable revision.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for ImmutableRevision {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for ImmutableRevision {
  type Err = ImmutableRevisionError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for ImmutableRevision {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for ImmutableRevision {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// An immutable revision is empty, contains control data, or exceeds its bound.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("immutable source revision is invalid")]
pub struct ImmutableRevisionError;

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

/// Provider-owned Repository locator safe to persist and pass to a source adapter.
///
/// The value deliberately remains provider-neutral: an integration may use an
/// HTTPS URL, a Git repository name, or another opaque remote identifier. The
/// shared contract rejects local absolute paths, traversal segments, embedded
/// URL credentials, query/fragment data, and control characters without
/// requiring every provider to use URL syntax.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RepositoryLocator(String);

impl RepositoryLocator {
  /// Constructs one bounded remote Repository locator.
  pub fn new(value: impl Into<String>) -> Result<Self, RepositoryLocatorError> {
    let value = value.into();
    if !valid_repository_locator(&value) {
      return Err(RepositoryLocatorError);
    }
    Ok(Self(value))
  }

  /// Borrows the provider-owned locator.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for RepositoryLocator {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for RepositoryLocator {
  type Err = RepositoryLocatorError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for RepositoryLocator {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for RepositoryLocator {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// A Repository locator is unsafe, ambiguous, contains control data, or is too long.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("repository locator is invalid")]
pub struct RepositoryLocatorError;

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

fn valid_repository_locator(value: &str) -> bool {
  if value.is_empty()
    || value.len() > MAX_REPOSITORY_LOCATOR_BYTES
    || value.trim() != value
    || value.chars().any(char::is_control)
    || value.starts_with(['/', '\\'])
    || value.starts_with('~')
    || has_windows_drive_prefix(value)
    || value.contains(['\\', '?', '#', ' '])
    || value.contains('@')
    || value.to_ascii_lowercase().contains("%2e")
    || value
      .get(..5)
      .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
    || value.split('/').any(|segment| matches!(segment, "." | ".."))
  {
    return false;
  }
  let Some((scheme, remainder)) = value.split_once("://") else {
    return true;
  };
  scheme.bytes().next().is_some_and(|byte| byte.is_ascii_alphabetic())
    && scheme
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    && remainder
      .split_once('/')
      .map_or_else(|| !remainder.is_empty(), |(authority, _)| !authority.is_empty())
}

fn has_windows_drive_prefix(value: &str) -> bool {
  let bytes = value.as_bytes();
  bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
