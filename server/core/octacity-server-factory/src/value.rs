use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use sha2::{Digest as _, Sha256};

use crate::{FactoryError, FactoryTextKind, TextRejection};

/// Maximum UTF-8 bytes in a provider-scoped external Work identity.
pub const MAX_EXTERNAL_WORK_IDENTITY_BYTES: usize = 256;
/// Maximum ASCII bytes in a provider-neutral Factory key.
pub const MAX_FACTORY_KEY_BYTES: usize = 128;
/// Maximum UTF-8 bytes in one human-readable Factory summary or reason.
pub const MAX_FACTORY_TEXT_BYTES: usize = 4 * 1024;
/// Maximum number of entries in one bounded metadata map.
pub const MAX_FACTORY_METADATA_ENTRIES: usize = 32;
/// Maximum UTF-8 bytes in one metadata key.
pub const MAX_FACTORY_METADATA_KEY_BYTES: usize = 64;
/// Maximum UTF-8 bytes in one metadata value.
pub const MAX_FACTORY_METADATA_VALUE_BYTES: usize = 512;
/// Maximum canonical key-plus-value bytes in one metadata map.
pub const MAX_FACTORY_METADATA_BYTES: usize = 8 * 1024;
/// Highest accepted Work priority.
pub const MAX_WORK_PRIORITY: u8 = 100;

/// Exact SHA-256 identity used for immutable Factory inputs and records.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FactoryDigest([u8; 32]);

impl FactoryDigest {
  /// Constructs a digest from its exact binary representation.
  #[must_use]
  pub const fn from_bytes(value: [u8; 32]) -> Self {
    Self(value)
  }

  /// Parses exactly 64 lowercase hexadecimal characters.
  pub fn from_lower_hex(value: &str) -> Result<Self, FactoryError> {
    if value.len() != 64
      || !value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
      return Err(FactoryError::InvalidDigest);
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
      digest[index] = (hex_digit(pair[0]) << 4) | hex_digit(pair[1]);
    }
    Ok(Self(digest))
  }

  /// Returns the binary SHA-256 representation.
  #[must_use]
  pub const fn as_bytes(self) -> [u8; 32] {
    self.0
  }

  /// Computes a domain-separated SHA-256 digest over length-delimited fields.
  ///
  /// Length framing prevents different field boundaries from producing the
  /// same preimage while the domain keeps unrelated Factory records separate.
  #[must_use]
  pub fn sha256(domain: &str, fields: &[&[u8]]) -> Self {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, domain.as_bytes());
    for field in fields {
      hash_field(&mut hasher, field);
    }
    Self(hasher.finalize().into())
  }
}

fn hash_field(hasher: &mut Sha256, field: &[u8]) {
  hasher.update(
    u64::try_from(field.len())
      .expect("Factory values fit in u64")
      .to_be_bytes(),
  );
  hasher.update(field);
}

impl fmt::Display for FactoryDigest {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    for byte in self.0 {
      write!(formatter, "{byte:02x}")?;
    }
    Ok(())
  }
}

impl FromStr for FactoryDigest {
  type Err = FactoryError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::from_lower_hex(value)
  }
}

impl Serialize for FactoryDigest {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.collect_str(self)
  }
}

impl<'de> Deserialize<'de> for FactoryDigest {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
  }
}

fn hex_digit(byte: u8) -> u8 {
  match byte {
    b'0'..=b'9' => byte - b'0',
    b'a'..=b'f' => byte - b'a' + 10,
    _ => unreachable!("validated hexadecimal digit"),
  }
}

macro_rules! sequence {
  ($name:ident, $documentation:literal) => {
    #[doc = $documentation]
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
    #[serde(transparent)]
    pub struct $name(u64);

    impl $name {
      /// First valid value.
      pub const INITIAL: Self = Self(1);

      /// Constructs a positive sequence value.
      pub const fn new(value: u64) -> Result<Self, FactoryError> {
        if value == 0 {
          return Err(FactoryError::InvalidSequence);
        }
        Ok(Self(value))
      }

      /// Returns the primitive value.
      #[must_use]
      pub const fn get(self) -> u64 {
        self.0
      }
    }

    impl<'de> Deserialize<'de> for $name {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where
        D: Deserializer<'de>,
      {
        u64::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
      }
    }
  };
}

sequence!(
  FactoryConfigurationVersion,
  "Positive immutable Factory Configuration version."
);
sequence!(DecisionPolicyVersion, "Positive immutable Decision policy version.");
sequence!(FactoryRunVersion, "Positive optimistic Factory Run version.");
sequence!(StageAttemptNumber, "Positive append-only Stage Attempt number.");
sequence!(DeliveryAttemptNumber, "Positive append-only delivery attempt number.");
sequence!(
  ReportingAttemptNumber,
  "Positive append-only external reporting attempt number."
);

/// Bounded Work priority where larger values are scheduled first by later policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WorkPriority(u8);

impl WorkPriority {
  /// Constructs a priority in the inclusive `0..=100` range.
  pub const fn new(value: u8) -> Result<Self, FactoryError> {
    if value > MAX_WORK_PRIORITY {
      return Err(FactoryError::InvalidPriority);
    }
    Ok(Self(value))
  }

  /// Returns the primitive priority.
  #[must_use]
  pub const fn get(self) -> u8 {
    self.0
  }
}

impl<'de> Deserialize<'de> for WorkPriority {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    u8::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

macro_rules! bounded_text {
  ($name:ident, $limit:expr, $kind:expr, $documentation:literal) => {
    #[doc = $documentation]
    #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct $name(String);

    impl $name {
      /// Constructs a trimmed, control-free bounded value.
      pub fn new(value: impl Into<String>) -> Result<Self, FactoryError> {
        let value = value.into();
        validate_text(&value, $limit, $kind)?;
        Ok(Self(value))
      }

      /// Borrows the validated text.
      #[must_use]
      pub fn as_str(&self) -> &str {
        &self.0
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
      }
    }

    impl FromStr for $name {
      type Err = FactoryError;

      fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
      }
    }

    impl Serialize for $name {
      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
      where
        S: Serializer,
      {
        serializer.serialize_str(&self.0)
      }
    }

    impl<'de> Deserialize<'de> for $name {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where
        D: Deserializer<'de>,
      {
        String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
      }
    }
  };
}

bounded_text!(
  ExternalWorkIdentity,
  MAX_EXTERNAL_WORK_IDENTITY_BYTES,
  FactoryTextKind::ExternalWorkIdentity,
  "Provider-scoped stable identity used to deduplicate admitted Work."
);
bounded_text!(
  FactoryText,
  MAX_FACTORY_TEXT_BYTES,
  FactoryTextKind::Text,
  "Bounded visible Factory summary or reason that excludes control characters."
);

/// Provider-neutral stable key using `[a-z0-9][a-z0-9._-]*`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FactoryKey(String);

impl FactoryKey {
  /// Constructs a canonical Factory key.
  pub fn new(value: impl Into<String>) -> Result<Self, FactoryError> {
    let value = value.into();
    validate_key(&value, MAX_FACTORY_KEY_BYTES, FactoryTextKind::Key)?;
    Ok(Self(value))
  }

  /// Borrows the canonical key.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for FactoryKey {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for FactoryKey {
  type Err = FactoryError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for FactoryKey {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for FactoryKey {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// Bounded deterministic metadata attached to one admitted Work Envelope.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryMetadata(BTreeMap<String, String>);

impl FactoryMetadata {
  /// Constructs metadata while rejecting duplicates, sensitive keys, and oversize values.
  pub fn try_new(entries: impl IntoIterator<Item = (String, String)>) -> Result<Self, FactoryError> {
    let mut metadata = BTreeMap::new();
    let mut encoded_bytes = 0_usize;
    for (key, value) in entries {
      validate_key(&key, MAX_FACTORY_METADATA_KEY_BYTES, FactoryTextKind::MetadataKey)?;
      if sensitive_key(&key) {
        return Err(FactoryError::InvalidText {
          kind: FactoryTextKind::MetadataKey,
          reason: TextRejection::Sensitive,
        });
      }
      validate_text(&value, MAX_FACTORY_METADATA_VALUE_BYTES, FactoryTextKind::MetadataValue)?;
      encoded_bytes = encoded_bytes
        .checked_add(key.len() + value.len())
        .ok_or(FactoryError::MetadataLimitExceeded)?;
      if metadata.insert(key, value).is_some() {
        return Err(FactoryError::DuplicateMetadataKey);
      }
    }
    if metadata.len() > MAX_FACTORY_METADATA_ENTRIES || encoded_bytes > MAX_FACTORY_METADATA_BYTES {
      return Err(FactoryError::MetadataLimitExceeded);
    }
    Ok(Self(metadata))
  }

  /// Iterates over metadata in canonical key order.
  pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &str)> {
    self.0.iter().map(|(key, value)| (key.as_str(), value.as_str()))
  }

  /// Reports whether the metadata map is empty.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }
}

fn validate_text(value: &str, limit: usize, kind: FactoryTextKind) -> Result<(), FactoryError> {
  let reason = if value.is_empty() {
    Some(TextRejection::Empty)
  } else if value.len() > limit {
    Some(TextRejection::TooLong)
  } else if value.trim() != value {
    Some(TextRejection::SurroundingWhitespace)
  } else if value.chars().any(char::is_control) {
    Some(TextRejection::ControlCharacter)
  } else {
    None
  };
  match reason {
    Some(reason) => Err(FactoryError::InvalidText { kind, reason }),
    None => Ok(()),
  }
}

fn validate_key(value: &str, limit: usize, kind: FactoryTextKind) -> Result<(), FactoryError> {
  if value.is_empty() {
    return Err(FactoryError::InvalidText {
      kind,
      reason: TextRejection::Empty,
    });
  }
  if value.len() > limit {
    return Err(FactoryError::InvalidText {
      kind,
      reason: TextRejection::TooLong,
    });
  }
  let mut characters = value.chars();
  if !characters
    .next()
    .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
  {
    return Err(FactoryError::InvalidText {
      kind,
      reason: TextRejection::InvalidStart,
    });
  }
  if !characters.all(|character| {
    character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '.' | '_' | '-')
  }) {
    return Err(FactoryError::InvalidText {
      kind,
      reason: TextRejection::InvalidCharacter,
    });
  }
  Ok(())
}

fn sensitive_key(key: &str) -> bool {
  matches!(
    key,
    "authorization" | "credential" | "password" | "private_key" | "secret" | "signature" | "token"
  ) || key.contains("presigned")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn bounded_values_reject_ambiguous_or_unsafe_text() {
    assert_eq!(
      FactoryKey::new("Unknown"),
      Err(FactoryError::InvalidText {
        kind: FactoryTextKind::Key,
        reason: TextRejection::InvalidStart,
      })
    );
    assert_eq!(
      FactoryText::new(" trailing "),
      Err(FactoryError::InvalidText {
        kind: FactoryTextKind::Text,
        reason: TextRejection::SurroundingWhitespace,
      })
    );
    assert_eq!(
      FactoryDigest::from_lower_hex(&"a".repeat(63)),
      Err(FactoryError::InvalidDigest)
    );
  }

  #[test]
  fn metadata_rejects_duplicates_sensitive_keys_and_bounds() {
    assert_eq!(
      FactoryMetadata::try_new([
        ("source".to_owned(), "one".to_owned()),
        ("source".to_owned(), "two".to_owned()),
      ]),
      Err(FactoryError::DuplicateMetadataKey)
    );
    assert!(matches!(
      FactoryMetadata::try_new([("token".to_owned(), "value".to_owned())]),
      Err(FactoryError::InvalidText {
        reason: TextRejection::Sensitive,
        ..
      })
    ));
    assert_eq!(
      FactoryMetadata::try_new(
        (0..=MAX_FACTORY_METADATA_ENTRIES).map(|index| { (format!("key_{index}"), "value".to_owned()) })
      ),
      Err(FactoryError::MetadataLimitExceeded)
    );
  }

  #[test]
  fn positive_sequences_reject_zero() {
    assert_eq!(FactoryConfigurationVersion::new(0), Err(FactoryError::InvalidSequence));
    assert_eq!(StageAttemptNumber::new(1).expect("one is valid").get(), 1);
  }

  #[test]
  fn work_priority_is_bounded() {
    assert_eq!(WorkPriority::new(100).expect("upper bound is valid").get(), 100);
    assert_eq!(WorkPriority::new(101), Err(FactoryError::InvalidPriority));
  }
}
