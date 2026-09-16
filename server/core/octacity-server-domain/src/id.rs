use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use uuid::Uuid;

use crate::{DomainValueError, EntityKind, IdentifierErrorKind};

macro_rules! opaque_id {
  ($name:ident, $entity:expr, $documentation:literal) => {
    #[doc = $documentation]
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct $name(Uuid);

    impl $name {
      /// Generates a new random opaque identifier.
      #[must_use]
      pub fn generate() -> Self {
        Self(Uuid::new_v4())
      }

      /// Constructs the identifier from a non-nil UUID.
      pub fn from_uuid(value: Uuid) -> Result<Self, DomainValueError> {
        if value.is_nil() {
          return Err(DomainValueError::InvalidIdentifier {
            entity: $entity,
            reason: IdentifierErrorKind::Nil,
          });
        }
        Ok(Self(value))
      }

      /// Returns the UUID value without assigning it additional semantics.
      #[must_use]
      pub const fn as_uuid(self) -> Uuid {
        self.0
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.hyphenated())
      }
    }

    impl FromStr for $name {
      type Err = DomainValueError;

      fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = Uuid::parse_str(value).map_err(|_| DomainValueError::InvalidIdentifier {
          entity: $entity,
          reason: IdentifierErrorKind::Malformed,
        })?;
        if parsed.is_nil() {
          return Err(DomainValueError::InvalidIdentifier {
            entity: $entity,
            reason: IdentifierErrorKind::Nil,
          });
        }
        if parsed.hyphenated().to_string() != value {
          return Err(DomainValueError::InvalidIdentifier {
            entity: $entity,
            reason: IdentifierErrorKind::NonCanonical,
          });
        }
        Ok(Self(parsed))
      }
    }

    impl Serialize for $name {
      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
      where
        S: Serializer,
      {
        serializer.collect_str(self)
      }
    }

    impl<'de> Deserialize<'de> for $name {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where
        D: Deserializer<'de>,
      {
        String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
      }
    }
  };
}

opaque_id!(ProjectId, EntityKind::Project, "Opaque identity of a Project.");
opaque_id!(
  BuildConfigurationId,
  EntityKind::Configuration,
  "Opaque identity of a Build Configuration."
);
opaque_id!(PipelineId, EntityKind::Pipeline, "Opaque identity of a Pipeline.");
opaque_id!(BuildId, EntityKind::Build, "Opaque identity of a Build.");
opaque_id!(AttemptId, EntityKind::Attempt, "Opaque identity of a Build Attempt.");
opaque_id!(JobId, EntityKind::Job, "Opaque identity of a materialized Job.");
opaque_id!(PoolId, EntityKind::Pool, "Opaque identity of an Agent Pool.");
opaque_id!(AgentId, EntityKind::Agent, "Opaque identity of an enrolled Agent.");
opaque_id!(LeaseId, EntityKind::Lease, "Opaque identity of a fenced Job lease.");
opaque_id!(
  ArtifactId,
  EntityKind::Artifact,
  "Opaque identity of a logical Artifact."
);
opaque_id!(
  ArtifactUploadId,
  EntityKind::ArtifactUpload,
  "Opaque identity of one Artifact upload attempt."
);
opaque_id!(
  IntegrationId,
  EntityKind::Integration,
  "Opaque identity of an external-system Integration."
);
opaque_id!(
  TriggerId,
  EntityKind::Trigger,
  "Opaque identity of a Trigger definition."
);
opaque_id!(
  TriggerOccurrenceId,
  EntityKind::Trigger,
  "Opaque identity of one normalized Trigger occurrence."
);
