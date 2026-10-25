//! Operation-shaped Runtime configuration journal over existing management mutation authority.
use crate::*;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use serde::{Deserialize, Serialize};

/// Original administrator intent; mutable installation resolution is deliberately absent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRuntimeSettingsIntent {
  /// Owning existing Project.
  pub project_id: ProjectId,
  /// Existing installed Runtime alias.
  pub runtime: FactoryKey,
  /// None creates; replacement requires the exact displayed revision.
  pub expected_version: Option<FactoryRuntimeSettingsVersion>,
  /// Bounded structured additions and reviewed installation pins.
  pub definition: FactoryRuntimeSettingsDefinition,
}
impl FactoryRuntimeSettingsIntent {
  /// Shared core validation and PostgreSQL-compatible positive revision bound.
  pub fn settings(&self) -> Result<FactoryRuntimeSettings, StoreError> {
    let version = self
      .expected_version
      .map_or(Some(1), |v| v.get().checked_add(1))
      .filter(|v| *v <= i64::MAX as u64)
      .ok_or_else(invalid_runtime_settings)?;
    FactoryRuntimeSettings::new(
      self.project_id,
      self.runtime.clone(),
      FactoryRuntimeSettingsVersion::new(version).map_err(|_| invalid_runtime_settings())?,
      self.definition.clone(),
    )
    .map_err(|_| invalid_runtime_settings())
  }
}
/// Accepted publication; audit and replay use the original intent, not current dependencies.
#[derive(Clone, Debug)]
pub struct PublishFactoryRuntimeSettings {
  /// Reviewed request.
  pub intent: FactoryRuntimeSettingsIntent,
  /// Scoped replay key.
  pub idempotency_key: IdempotencyKey,
  /// Original first acceptance time.
  pub published_at: Timestamp,
}
/// Historical replay probe; creates no reservation.
#[derive(Clone, Debug)]
pub struct ReplayFactoryRuntimeSettings {
  /// Exact original client intent.
  pub intent: FactoryRuntimeSettingsIntent,
  /// Original scoped mutation identity.
  pub idempotency_key: IdempotencyKey,
}
/// Immutable result, independent of later changes to Runtime inventory or current heads.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRuntimeSettingsOutcome {
  /// Applied or original historical replay.
  pub disposition: MutationDisposition,
  /// Exact retained settings.
  pub settings: FactoryRuntimeSettings,
  /// Original acceptance time.
  pub published_at: Timestamp,
}
/// Distinguishes stale optimistic versions from conflicting replay identities.
#[derive(Debug, thiserror::Error)]
pub enum FactoryRuntimeSettingsPublicationError {
  /// Another configuration won the displayed revision.
  #[error("runtime settings precondition failed")]
  PreconditionFailed,
  /// Classified persistence/intent failure.
  #[error(transparent)]
  Store(#[from] StoreError),
}
/// Backend-neutral append-only settings, reusing management audit and scoped replay.
#[async_trait::async_trait]
pub trait FactoryRuntimeSettingsStore: Send + Sync {
  /// Current or exact historical settings; absence preserves installation behavior.
  async fn factory_runtime_settings(
    &self,
    project: ProjectId,
    runtime: &FactoryKey,
    version: Option<FactoryRuntimeSettingsVersion>,
  ) -> Result<Option<FactoryRuntimeSettings>, StoreError>;
  /// Exact original receipt before mutable installation qualification.
  async fn replay_factory_runtime_settings(
    &self,
    request: ManagementMutation<ReplayFactoryRuntimeSettings>,
  ) -> Result<Option<FactoryRuntimeSettingsOutcome>, StoreError>;
  /// Atomically publish version, replay result, audit and outbox facts.
  async fn publish_factory_runtime_settings(
    &self,
    request: ManagementMutation<PublishFactoryRuntimeSettings>,
  ) -> Result<FactoryRuntimeSettingsOutcome, FactoryRuntimeSettingsPublicationError>;
}
pub(crate) fn invalid_runtime_settings() -> StoreError {
  StoreError::invalid(
    StoreOperation::PublishFactoryRuntimeSettings,
    StoreInputError::InvalidFactoryRuntimeSettings,
  )
}
