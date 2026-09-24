use std::{collections::BTreeSet, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

/// Maximum UTF-8 bytes in one logical provider, profile, or secret reference.
pub const MAX_LOGICAL_REFERENCE_BYTES: usize = 128;
/// Maximum distinct logical entries in one delegated grant request.
pub const MAX_PROFILE_REFERENCES: usize = 128;

fn valid_reference(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= MAX_LOGICAL_REFERENCE_BYTES
    && value.trim() == value
    && !value.chars().any(char::is_control)
}

fn valid_identifier(value: &str) -> bool {
  let mut characters = value.chars();
  valid_reference(value)
    && characters
      .next()
      .is_some_and(|character| character.is_ascii_alphanumeric())
    && characters.all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
}

macro_rules! logical_name {
  ($name:ident, $documentation:literal, $error:literal, $validator:ident) => {
    #[doc = $documentation]
    #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct $name(String);

    impl $name {
      /// Constructs a bounded visible logical name.
      pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if !$validator(&value) {
          return Err($error);
        }
        Ok(Self(value))
      }

      /// Borrows the validated logical value.
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
      type Err = &'static str;

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

logical_name!(
  SecretProfileName,
  "Logical Octa secret profile selectable by Builds in a Project.",
  "invalid secret profile name",
  valid_reference
);
logical_name!(
  IdentityProfileName,
  "Logical workload-identity profile selectable by Builds in a Project.",
  "invalid workload identity profile name",
  valid_reference
);
logical_name!(
  SecretProviderId,
  "Stable logical identity of a configured secret provider.",
  "invalid secret provider identity",
  valid_identifier
);

/// Provider-scoped logical locator that never contains a secret value.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogicalSecretReference {
  provider: SecretProviderId,
  reference: String,
}

impl LogicalSecretReference {
  /// Constructs a bounded provider-owned locator.
  pub fn new(provider: SecretProviderId, reference: impl Into<String>) -> Result<Self, &'static str> {
    let reference = reference.into();
    if !valid_reference(&reference) {
      return Err("invalid logical secret reference");
    }
    Ok(Self { provider, reference })
  }

  /// Returns the configured provider identity.
  #[must_use]
  pub const fn provider(&self) -> &SecretProviderId {
    &self.provider
  }

  /// Borrows the provider-owned logical locator.
  #[must_use]
  pub fn reference(&self) -> &str {
    &self.reference
  }
}

pub(crate) fn validate_reference_set(references: &BTreeSet<LogicalSecretReference>) -> bool {
  !references.is_empty() && references.len() <= MAX_PROFILE_REFERENCES
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn logical_names_and_references_are_bounded_and_strict() {
    let provider = SecretProviderId::new("vault.prod").unwrap();
    let reference = LogicalSecretReference::new(provider.clone(), "teams/build/signing-key").unwrap();
    assert_eq!(reference.provider(), &provider);
    assert_eq!(reference.reference(), "teams/build/signing-key");
    assert!(SecretProviderId::new("bad/provider").is_err());
    assert!(SecretProfileName::new(" ci ").is_err());
    assert!(LogicalSecretReference::new(provider, "line\nbreak").is_err());
  }

  #[test]
  fn serialized_references_contain_only_logical_values() {
    let reference =
      LogicalSecretReference::new(SecretProviderId::new("vault").unwrap(), "projects/release/token").unwrap();
    assert_eq!(
      serde_json::to_value(reference).unwrap(),
      serde_json::json!({"provider": "vault", "reference": "projects/release/token"})
    );
  }
}
