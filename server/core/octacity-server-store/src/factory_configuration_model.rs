use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{
  FactoryConfiguration, FactoryConfigurationChoices, FactoryConfigurationDraft, FactoryConfigurationId,
  FactoryConfigurationVersion, FactoryDigest,
};
use serde::{Deserialize, Serialize};

use crate::{IdempotencyKey, MutationDisposition, StoreError};

/// Trusted Project-scoped capability state used during configuration validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryConfigurationAvailability {
  /// Factory mode is not configured for this Project.
  Disabled,
  /// Factory mode is available with these exact selectable references.
  Available(FactoryConfigurationChoices),
}

/// One immutable published Factory Configuration version and its publication time.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedFactoryConfiguration {
  /// Fully resolved immutable Factory Configuration.
  pub configuration: FactoryConfiguration,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Immutable command input used to detect an exact replay before resolving
/// mutable deployment capabilities again.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum FactoryConfigurationMutationIntent {
  /// Initial publication intent.
  Create {
    /// Stable configuration identity chosen by the caller.
    id: FactoryConfigurationId,
    /// Existing owning Project.
    project_id: ProjectId,
    /// Digest of the canonical submitted definition.
    definition_digest: FactoryDigest,
    /// Complete unresolved definition as originally submitted.
    draft: FactoryConfigurationDraft,
  },
  /// Immutable-version replacement intent.
  Replace {
    /// Existing configuration identity.
    id: FactoryConfigurationId,
    /// Version that was current when the intent was created.
    expected_current_version: FactoryConfigurationVersion,
    /// Digest of the canonical submitted definition.
    definition_digest: FactoryDigest,
    /// Complete unresolved replacement as originally submitted.
    draft: FactoryConfigurationDraft,
  },
}

impl FactoryConfigurationMutationIntent {
  /// Reports whether this immutable caller intent describes the supplied
  /// resolved configuration and replacement precondition exactly.
  #[must_use]
  pub fn matches_configuration(
    &self,
    configuration: &FactoryConfiguration,
    expected_current_version: Option<FactoryConfigurationVersion>,
  ) -> bool {
    let reference = configuration.reference();
    match self {
      Self::Create {
        id,
        project_id,
        definition_digest,
        ..
      } => {
        expected_current_version.is_none()
          && reference.id() == *id
          && reference.project_id() == *project_id
          && reference.version() == FactoryConfigurationVersion::INITIAL
          && reference.definition_digest() == *definition_digest
      }
      Self::Replace {
        id,
        expected_current_version: intent_version,
        definition_digest,
        ..
      } => {
        expected_current_version == Some(*intent_version)
          && reference.id() == *id
          && reference.version().get() == intent_version.get().saturating_add(1)
          && reference.definition_digest() == *definition_digest
      }
    }
  }
}

/// Read-only replay probe for one Factory Configuration mutation intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayFactoryConfigurationMutation {
  /// Stable idempotency identity of the original command.
  pub idempotency_key: IdempotencyKey,
  /// Exact unresolved command intent, independent of mutable capability state.
  pub intent: FactoryConfigurationMutationIntent,
}

/// Atomic request to create one Factory Configuration and immutable version one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateFactoryConfiguration {
  /// Fully resolved immutable initial version.
  pub configuration: FactoryConfiguration,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
  /// Exact unresolved intent used for replay consistency checks.
  pub intent: FactoryConfigurationMutationIntent,
}

/// Atomic request to append exactly the next Factory Configuration version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceFactoryConfiguration {
  /// Version that must still be current.
  pub expected_current_version: FactoryConfigurationVersion,
  /// Fully resolved immutable replacement version.
  pub configuration: FactoryConfiguration,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
  /// Exact unresolved intent used for replay consistency checks.
  pub intent: FactoryConfigurationMutationIntent,
}

/// Result of one Factory Configuration create or replacement command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfigurationMutationOutcome {
  /// Whether the mutation was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Immutable version committed by the original command.
  pub configuration: PublishedFactoryConfiguration,
}

/// Failure from a strongly preconditioned Factory Configuration replacement.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReplaceFactoryConfigurationError {
  /// The supplied current version is no longer authoritative.
  #[error("the Factory Configuration version precondition failed")]
  PreconditionFailed,
  /// The authoritative store rejected or could not complete the operation.
  #[error(transparent)]
  Store(#[from] StoreError),
}
