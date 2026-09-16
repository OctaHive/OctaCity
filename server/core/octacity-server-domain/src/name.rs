use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{DomainValueError, EntityKind, TextErrorKind};

/// Maximum UTF-8 bytes in a resource display name.
pub const MAX_RESOURCE_NAME_BYTES: usize = 128;
/// Maximum UTF-8 bytes in a logical Artifact name.
pub const MAX_ARTIFACT_NAME_BYTES: usize = 256;

fn validate_name(value: &str, entity: EntityKind, max_bytes: usize) -> Result<(), DomainValueError> {
  let reason = if value.is_empty() {
    Some(TextErrorKind::Empty)
  } else if value.len() > max_bytes {
    Some(TextErrorKind::TooLong)
  } else if value.trim() != value {
    Some(TextErrorKind::SurroundingWhitespace)
  } else if value.chars().any(char::is_control) {
    Some(TextErrorKind::ControlCharacter)
  } else {
    None
  };
  match reason {
    Some(reason) => Err(DomainValueError::InvalidName { entity, reason }),
    None => Ok(()),
  }
}

macro_rules! bounded_name {
  ($name:ident, $entity:expr, $max:expr, $documentation:literal) => {
    #[doc = $documentation]
    #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct $name(String);

    impl $name {
      /// Constructs a validated bounded name.
      pub fn new(value: impl Into<String>) -> Result<Self, DomainValueError> {
        let value = value.into();
        validate_name(&value, $entity, $max)?;
        Ok(Self(value))
      }

      /// Borrows the validated name.
      #[must_use]
      pub fn as_str(&self) -> &str {
        &self.0
      }

      /// Returns the owned validated name.
      #[must_use]
      pub fn into_inner(self) -> String {
        self.0
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
      }
    }

    impl FromStr for $name {
      type Err = DomainValueError;

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

bounded_name!(
  ProjectName,
  EntityKind::Project,
  MAX_RESOURCE_NAME_BYTES,
  "Bounded display name of a Project."
);
bounded_name!(
  BuildConfigurationName,
  EntityKind::Configuration,
  MAX_RESOURCE_NAME_BYTES,
  "Bounded sibling name of a Build Configuration."
);
bounded_name!(
  PipelineName,
  EntityKind::Pipeline,
  MAX_RESOURCE_NAME_BYTES,
  "Bounded display name of a Pipeline."
);
bounded_name!(
  JobName,
  EntityKind::Job,
  MAX_RESOURCE_NAME_BYTES,
  "Bounded display name of a Job template."
);
bounded_name!(
  PoolName,
  EntityKind::Pool,
  MAX_RESOURCE_NAME_BYTES,
  "Bounded display name of an Agent Pool."
);
bounded_name!(
  AgentName,
  EntityKind::Agent,
  MAX_RESOURCE_NAME_BYTES,
  "Bounded operator-facing name of an Agent."
);
bounded_name!(
  ArtifactName,
  EntityKind::Artifact,
  MAX_ARTIFACT_NAME_BYTES,
  "Bounded logical name of an Artifact or report."
);
bounded_name!(
  IntegrationName,
  EntityKind::Integration,
  MAX_RESOURCE_NAME_BYTES,
  "Bounded display name of an external-system Integration."
);
