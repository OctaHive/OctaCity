use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::StoreInputError;

/// Maximum UTF-8 bytes in a store mutation idempotency key.
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;
/// Maximum ASCII bytes in an opaque management security scope.
pub const MAX_MANAGEMENT_SECURITY_SCOPE_BYTES: usize = 128;

const TRUSTED_NETWORK_SECURITY_SCOPE: &str = "trusted-network";

/// Opaque stable partition that isolates management idempotency outcomes.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagementSecurityScope(String);

impl ManagementSecurityScope {
  /// Constructs a canonical scope using `[a-z0-9][a-z0-9._:-]*`.
  pub fn new(value: impl Into<String>) -> Result<Self, StoreInputError> {
    let value = value.into();
    let mut characters = value.chars();
    let valid = value.len() <= MAX_MANAGEMENT_SECURITY_SCOPE_BYTES
      && characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
      && characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '.' | '_' | ':' | '-')
      });
    if !valid {
      return Err(StoreInputError::InvalidManagementSecurityScope);
    }
    Ok(Self(value))
  }

  /// Constructs the stable scope used by the trusted-network deployment.
  #[must_use]
  pub fn trusted_network() -> Self {
    Self(TRUSTED_NETWORK_SECURITY_SCOPE.to_owned())
  }

  /// Borrows the stable scope identity for authoritative persistence.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Debug for ManagementSecurityScope {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("ManagementSecurityScope(<redacted>)")
  }
}

impl FromStr for ManagementSecurityScope {
  type Err = StoreInputError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

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

/// Complete caller-selected replay identity inside one management security scope.
///
/// Operation scope remains a separate store-adapter concern, so the complete
/// authoritative identity is `(operation, management scope, caller key)`.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagementIdempotencyKey {
  security_scope: ManagementSecurityScope,
  caller_key: IdempotencyKey,
}

impl ManagementIdempotencyKey {
  /// Binds one validated caller key to the accepted management security scope.
  #[must_use]
  pub const fn new(security_scope: ManagementSecurityScope, caller_key: IdempotencyKey) -> Self {
    Self {
      security_scope,
      caller_key,
    }
  }

  /// Borrows the opaque management security scope.
  #[must_use]
  pub const fn security_scope(&self) -> &ManagementSecurityScope {
    &self.security_scope
  }

  /// Borrows the caller-selected idempotency key.
  #[must_use]
  pub const fn caller_key(&self) -> &IdempotencyKey {
    &self.caller_key
  }
}

impl fmt::Debug for ManagementIdempotencyKey {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("ManagementIdempotencyKey")
      .field("security_scope", &self.security_scope)
      .field("caller_key", &self.caller_key)
      .finish()
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

#[cfg(test)]
mod tests {
  use std::collections::BTreeMap;

  use super::*;

  #[derive(Clone, Copy, Debug, Eq, PartialEq)]
  enum ReplayDisposition {
    Applied,
    Replayed,
  }

  #[derive(Default)]
  struct ReplayLedger {
    outcomes: BTreeMap<(&'static str, ManagementIdempotencyKey), (&'static str, u64)>,
  }

  impl ReplayLedger {
    fn execute(
      &mut self,
      operation: &'static str,
      key: ManagementIdempotencyKey,
      fingerprint: &'static str,
      outcome: u64,
    ) -> Result<(ReplayDisposition, u64), ()> {
      match self.outcomes.get(&(operation, key.clone())) {
        Some((stored_fingerprint, stored_outcome)) if *stored_fingerprint == fingerprint => {
          Ok((ReplayDisposition::Replayed, *stored_outcome))
        }
        Some(_) => Err(()),
        None => {
          self.outcomes.insert((operation, key), (fingerprint, outcome));
          Ok((ReplayDisposition::Applied, outcome))
        }
      }
    }
  }

  #[test]
  fn replay_identity_isolated_by_management_security_scope() {
    let caller_key = IdempotencyKey::new("caller-key").unwrap();
    let first_scope = ManagementSecurityScope::new("operator:first").unwrap();
    let second_scope = ManagementSecurityScope::new("operator:second").unwrap();
    let mut ledger = ReplayLedger::default();

    assert_eq!(
      ledger.execute(
        "create-project",
        ManagementIdempotencyKey::new(first_scope.clone(), caller_key.clone()),
        "first-intent",
        41,
      ),
      Ok((ReplayDisposition::Applied, 41))
    );
    assert_eq!(
      ledger.execute(
        "create-project",
        ManagementIdempotencyKey::new(first_scope.clone(), caller_key.clone()),
        "first-intent",
        99,
      ),
      Ok((ReplayDisposition::Replayed, 41))
    );
    assert_eq!(
      ledger.execute(
        "create-project",
        ManagementIdempotencyKey::new(first_scope, caller_key.clone()),
        "different-intent",
        57,
      ),
      Err(()),
      "reuse with different business intent must still conflict inside one scope"
    );
    assert_eq!(
      ledger.execute(
        "create-project",
        ManagementIdempotencyKey::new(second_scope, caller_key),
        "second-intent",
        73,
      ),
      Ok((ReplayDisposition::Applied, 73)),
      "another scope must neither collide with nor observe the first outcome"
    );
    assert_eq!(ledger.outcomes.len(), 2);
  }

  #[test]
  fn management_security_scope_is_bounded_and_redacted() {
    for invalid in ["", "UPPER", " leading", "trailing ", "line\nbreak"] {
      assert_eq!(
        ManagementSecurityScope::new(invalid),
        Err(StoreInputError::InvalidManagementSecurityScope)
      );
    }
    assert_eq!(
      ManagementSecurityScope::new("s".repeat(MAX_MANAGEMENT_SECURITY_SCOPE_BYTES + 1)),
      Err(StoreInputError::InvalidManagementSecurityScope)
    );
    let scope = ManagementSecurityScope::new("private:operator-1").unwrap();
    assert!(!format!("{scope:?}").contains("private:operator-1"));
  }
}
