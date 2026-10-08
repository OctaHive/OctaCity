use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{FactoryError, FactoryKey, StageAttemptId};

/// Purpose of one logical credential profile in the Factory trust model.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryCredentialPurpose {
  /// Provider access used by implementation and rework model calls.
  Model,
  /// Provider access used only by independent evaluation calls.
  Evaluator,
  /// Read-only source-provider access used by the trusted source resolver.
  Source,
  /// Write-capable forge access used only by the trusted delivery adapter.
  Delivery,
}

/// Trusted component allowed to consume one Factory credential purpose.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FactoryCredentialConsumer {
  /// Writable implementation or rework stage.
  CodingStage,
  /// Read-only independent evaluation stage.
  EvaluationStage,
  /// Trusted source-resolution adapter.
  SourceResolver,
  /// Trusted write-capable delivery adapter.
  DeliveryAdapter,
}

impl FactoryCredentialConsumer {
  /// Returns the only credential purpose this consumer may resolve.
  #[must_use]
  pub const fn purpose(self) -> FactoryCredentialPurpose {
    match self {
      Self::CodingStage => FactoryCredentialPurpose::Model,
      Self::EvaluationStage => FactoryCredentialPurpose::Evaluator,
      Self::SourceResolver => FactoryCredentialPurpose::Source,
      Self::DeliveryAdapter => FactoryCredentialPurpose::Delivery,
    }
  }
}

/// Four distinct logical profiles frozen for one Factory Configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCredentialProfiles {
  model: FactoryKey,
  evaluator: FactoryKey,
  source: FactoryKey,
  delivery: FactoryKey,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryCredentialProfilesWire {
  model: FactoryKey,
  evaluator: FactoryKey,
  source: FactoryKey,
  delivery: FactoryKey,
}

impl<'de> Deserialize<'de> for FactoryCredentialProfiles {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = FactoryCredentialProfilesWire::deserialize(deserializer)?;
    Self::new(wire.model, wire.evaluator, wire.source, wire.delivery).map_err(D::Error::custom)
  }
}

impl FactoryCredentialProfiles {
  /// Constructs a profile set and rejects any cross-purpose reuse.
  pub fn new(
    model: FactoryKey,
    evaluator: FactoryKey,
    source: FactoryKey,
    delivery: FactoryKey,
  ) -> Result<Self, FactoryError> {
    let profiles = [&model, &evaluator, &source, &delivery]
      .into_iter()
      .collect::<BTreeSet<_>>();
    if profiles.len() != 4 {
      return Err(FactoryError::InvalidReference {
        relationship: "distinct credential profiles",
      });
    }
    Ok(Self {
      model,
      evaluator,
      source,
      delivery,
    })
  }

  /// Returns the logical profile assigned to one purpose.
  #[must_use]
  pub const fn profile(&self, purpose: FactoryCredentialPurpose) -> &FactoryKey {
    match purpose {
      FactoryCredentialPurpose::Model => &self.model,
      FactoryCredentialPurpose::Evaluator => &self.evaluator,
      FactoryCredentialPurpose::Source => &self.source,
      FactoryCredentialPurpose::Delivery => &self.delivery,
    }
  }

  /// Authorizes an exact logical profile for one trusted Stage consumer.
  ///
  /// This operation returns only public logical authority. Sensitive material
  /// must not be requested from a provider until this check succeeds.
  pub fn authorize(
    &self,
    stage_attempt_id: StageAttemptId,
    consumer: FactoryCredentialConsumer,
    requested_profile: &FactoryKey,
  ) -> Result<AuthorizedFactoryCredentialProfile, FactoryError> {
    let purpose = consumer.purpose();
    if self.profile(purpose) != requested_profile {
      return Err(FactoryError::InvalidReference {
        relationship: "credential profile consumer scope",
      });
    }
    Ok(AuthorizedFactoryCredentialProfile {
      stage_attempt_id,
      consumer,
      purpose,
      profile: requested_profile.clone(),
    })
  }
}

/// Public, stage-bound authorization to materialize one logical profile.
///
/// The token deliberately carries no credential bytes and is not serializable,
/// so durable records cannot be mistaken for secret-delivery authority.
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorizedFactoryCredentialProfile {
  stage_attempt_id: StageAttemptId,
  consumer: FactoryCredentialConsumer,
  purpose: FactoryCredentialPurpose,
  profile: FactoryKey,
}

impl AuthorizedFactoryCredentialProfile {
  /// Returns the Stage Attempt to which later materialization must be bound.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the trusted component class allowed to receive the credential.
  #[must_use]
  pub const fn consumer(&self) -> FactoryCredentialConsumer {
    self.consumer
  }

  /// Returns the authorized credential purpose.
  #[must_use]
  pub const fn purpose(&self) -> FactoryCredentialPurpose {
    self.purpose
  }

  /// Returns the authorized logical profile without materialized values.
  #[must_use]
  pub const fn profile(&self) -> &FactoryKey {
    &self.profile
  }
}
