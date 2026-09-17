use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{DomainValueError, EntityKind, VersionErrorKind};

macro_rules! positive_version {
  ($name:ident, $entity:expr, $documentation:literal) => {
    #[doc = $documentation]
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct $name(NonZeroU64);

    impl $name {
      /// Initial version assigned to a newly created entity.
      pub const INITIAL: Self = Self(NonZeroU64::MIN);

      /// Constructs a positive entity version.
      pub fn new(value: u64) -> Result<Self, DomainValueError> {
        NonZeroU64::new(value)
          .map(Self)
          .ok_or(DomainValueError::InvalidVersion {
            entity: $entity,
            reason: VersionErrorKind::Zero,
          })
      }

      /// Returns the positive numeric version.
      #[must_use]
      pub const fn get(self) -> u64 {
        self.0.get()
      }

      /// Returns the next version or a typed overflow failure.
      pub fn next(self) -> Result<Self, DomainValueError> {
        self
          .get()
          .checked_add(1)
          .map(|value| Self(NonZeroU64::new(value).expect("increment is non-zero")))
          .ok_or(DomainValueError::InvalidVersion {
            entity: $entity,
            reason: VersionErrorKind::Overflow,
          })
      }
    }

    impl Serialize for $name {
      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
      where
        S: Serializer,
      {
        serializer.serialize_u64(self.get())
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

positive_version!(ProjectVersion, EntityKind::Project, "Optimistic version of a Project.");
positive_version!(
  BuildConfigurationVersion,
  EntityKind::Configuration,
  "Immutable version number of a Build Configuration."
);
positive_version!(
  PipelineVersion,
  EntityKind::Pipeline,
  "Immutable version number of a Pipeline DAG."
);
positive_version!(
  RepositoryVersion,
  EntityKind::Repository,
  "Immutable version number of a source Repository definition."
);
positive_version!(BuildVersion, EntityKind::Build, "Optimistic state version of a Build.");
positive_version!(
  AttemptVersion,
  EntityKind::Attempt,
  "Optimistic state version of an Attempt."
);
positive_version!(JobVersion, EntityKind::Job, "Optimistic state version of a Job.");
positive_version!(PoolVersion, EntityKind::Pool, "Optimistic version of an Agent Pool.");
positive_version!(AgentVersion, EntityKind::Agent, "Optimistic state version of an Agent.");
positive_version!(
  LeaseVersion,
  EntityKind::Lease,
  "Optimistic state version of a Job lease."
);
positive_version!(
  ArtifactVersion,
  EntityKind::Artifact,
  "Optimistic state version of an Artifact."
);
positive_version!(
  IntegrationVersion,
  EntityKind::Integration,
  "Optimistic version of an external-system Integration."
);
positive_version!(
  TriggerVersion,
  EntityKind::Trigger,
  "Immutable version number of a Trigger definition."
);

/// Positive monotonically increasing attempt number within one Build.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AttemptNumber(NonZeroU64);

impl AttemptNumber {
  /// Number assigned to the first Attempt of a Build.
  pub const FIRST: Self = Self(NonZeroU64::MIN);

  /// Constructs a positive Attempt number.
  pub fn new(value: u64) -> Result<Self, DomainValueError> {
    NonZeroU64::new(value)
      .map(Self)
      .ok_or(DomainValueError::InvalidAttemptNumber {
        reason: VersionErrorKind::Zero,
      })
  }

  /// Returns the positive numeric Attempt number.
  #[must_use]
  pub const fn get(self) -> u64 {
    self.0.get()
  }

  /// Returns the next Attempt number or a typed overflow failure.
  pub fn next(self) -> Result<Self, DomainValueError> {
    self
      .get()
      .checked_add(1)
      .map(|value| Self(NonZeroU64::new(value).expect("increment is non-zero")))
      .ok_or(DomainValueError::InvalidAttemptNumber {
        reason: VersionErrorKind::Overflow,
      })
  }
}

impl Serialize for AttemptNumber {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_u64(self.get())
  }
}

impl<'de> Deserialize<'de> for AttemptNumber {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    u64::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}
