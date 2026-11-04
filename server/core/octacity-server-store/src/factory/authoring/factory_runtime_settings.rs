//! Operation-shaped Runtime configuration journal over existing management mutation authority.
use crate::*;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
  /// Resolves selected exact pins together, returning each matching transport once without current-head substitution.
  async fn factory_runtime_mcp_configurations(
    &self,
    project: ProjectId,
    references: &[ImmutableReference],
  ) -> Result<BTreeMap<ImmutableReference, FactoryMcpConfiguration>, StoreError>;
  /// Resolves only an exact retained Project transport pin; never substitutes current settings.
  async fn factory_runtime_mcp_configuration(
    &self,
    project: ProjectId,
    reference: &ImmutableReference,
  ) -> Result<Option<FactoryMcpConfiguration>, StoreError> {
    Ok(
      self
        .factory_runtime_mcp_configurations(project, std::slice::from_ref(reference))
        .await?
        .remove(reference),
    )
  }
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

/// Shared exact selection over verified settings; adapters load each candidate document at most once per batch.
pub fn resolve_factory_runtime_mcp_configurations<'a>(
  references: &[ImmutableReference],
  settings: impl IntoIterator<Item = &'a FactoryRuntimeSettings>,
) -> Result<BTreeMap<ImmutableReference, FactoryMcpConfiguration>, StoreError> {
  let mut configurations = BTreeMap::new();
  for settings in settings {
    for reference in references
      .iter()
      .filter(|reference| factory_runtime_mcp_revision(reference) == Some(settings.version()))
    {
      if configurations.contains_key(reference) {
        continue;
      }
      if let Some(configuration) = settings
        .mcp_configuration(reference.identity())
        .map_err(|_| StoreError::Unavailable)?
        && configuration.reference() == reference
      {
        configurations.insert(reference.clone(), configuration);
      }
    }
  }
  Ok(configurations)
}

/// Settings revision encoded by the shared transport pin. Installed references may use other schemes.
pub fn factory_runtime_mcp_revision(reference: &ImmutableReference) -> Option<FactoryRuntimeSettingsVersion> {
  let value = reference.version().as_str().strip_prefix('v')?.parse::<u64>().ok()?;
  if value > i64::MAX as u64 || reference.version().as_str() != format!("v{value}") {
    return None;
  }
  FactoryRuntimeSettingsVersion::new(value).ok()
}
pub(crate) fn invalid_runtime_settings() -> StoreError {
  StoreError::invalid(
    StoreOperation::PublishFactoryRuntimeSettings,
    StoreInputError::InvalidFactoryRuntimeSettings,
  )
}

/// Shared pure projection, also checked under the publication owner's Project lock.
/// Reading maps malformed retained state to unavailability; publication rejects an oversized candidate.
pub fn configured_factory_catalog(
  availability: FactoryConfigurationAvailability,
  settings: impl IntoIterator<Item = FactoryRuntimeSettings>,
) -> Result<FactoryConfigurationAvailability, FactoryError> {
  let FactoryConfigurationAvailability::Available(mut choices) = availability else {
    return Ok(availability);
  };
  for value in settings {
    choices = choices.with_runtime_settings(&value)?;
  }
  Ok(FactoryConfigurationAvailability::Available(choices))
}
