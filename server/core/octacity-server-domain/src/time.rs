use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{DomainValueError, TimestampErrorKind};

/// Earliest supported Unix millisecond timestamp: `0001-01-01T00:00:00Z`.
pub const MIN_TIMESTAMP_MILLIS: i64 = -62_135_596_800_000;
/// Latest supported Unix millisecond timestamp: `9999-12-31T23:59:59.999Z`.
pub const MAX_TIMESTAMP_MILLIS: i64 = 253_402_300_799_999;

/// UTC instant represented as bounded Unix epoch milliseconds.
///
/// Milliseconds provide a stable database- and transport-neutral precision for
/// orchestration. Calendar and timezone interpretation stays at the boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Timestamp(i64);

impl Timestamp {
  /// Constructs a timestamp within years 0001 through 9999 UTC.
  pub fn from_unix_millis(value: i64) -> Result<Self, DomainValueError> {
    if !(MIN_TIMESTAMP_MILLIS..=MAX_TIMESTAMP_MILLIS).contains(&value) {
      return Err(DomainValueError::InvalidTimestamp {
        reason: TimestampErrorKind::OutOfRange,
      });
    }
    Ok(Self(value))
  }

  /// Returns Unix epoch milliseconds.
  #[must_use]
  pub const fn unix_millis(self) -> i64 {
    self.0
  }
}

impl Serialize for Timestamp {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_i64(self.0)
  }
}

impl<'de> Deserialize<'de> for Timestamp {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    i64::deserialize(deserializer).and_then(|value| Self::from_unix_millis(value).map_err(D::Error::custom))
  }
}
