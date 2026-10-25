//! Memory adapter for the same operation contract as persistent Runtime settings.
use crate::{
  testing::{InMemoryFactoryConfigurationStore, recorded_management_audit},
  *,
};
use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_factory::*;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct RuntimeSettingsMemoryState {
  versions: BTreeMap<(ProjectId, FactoryKey), BTreeMap<FactoryRuntimeSettingsVersion, FactoryRuntimeSettings>>,
  mutations: BTreeMap<ManagementIdempotencyKey, (FactoryRuntimeSettingsIntent, FactoryRuntimeSettingsOutcome)>,
}
impl RuntimeSettingsMemoryState {
  pub(super) fn current(&self, project: ProjectId) -> impl Iterator<Item = &FactoryRuntimeSettings> {
    self
      .versions
      .iter()
      .filter(move |((owner, _), _)| *owner == project)
      .filter_map(|(_, versions)| versions.last_key_value().map(|(_, settings)| settings))
  }
  fn replay(
    &self,
    key: &ManagementIdempotencyKey,
    intent: &FactoryRuntimeSettingsIntent,
  ) -> Result<Option<FactoryRuntimeSettingsOutcome>, StoreError> {
    let Some((prior, outcome)) = self.mutations.get(key) else {
      return Ok(None);
    };
    if prior != intent {
      return Err(StoreError::Conflict {
        entity: EntityKind::Project,
      });
    }
    let mut replay = outcome.clone();
    replay.disposition = MutationDisposition::Replayed;
    Ok(Some(replay))
  }
}
#[async_trait::async_trait]
impl FactoryRuntimeSettingsStore for InMemoryFactoryConfigurationStore {
  async fn factory_runtime_mcp_configurations(
    &self,
    project: ProjectId,
    references: &[ImmutableReference],
  ) -> Result<BTreeMap<ImmutableReference, FactoryMcpConfiguration>, StoreError> {
    let state = self.lock()?;
    if !state.projects.contains(&project) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    resolve_factory_runtime_mcp_configurations(
      references,
      state
        .runtime_settings
        .versions
        .iter()
        .filter(|((owner, _), _)| *owner == project)
        .flat_map(|(_, versions)| versions.values()),
    )
  }
  async fn factory_runtime_settings(
    &self,
    project: ProjectId,
    runtime: &FactoryKey,
    version: Option<FactoryRuntimeSettingsVersion>,
  ) -> Result<Option<FactoryRuntimeSettings>, StoreError> {
    let state = self.lock()?;
    if !state.projects.contains(&project) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    Ok(
      state
        .runtime_settings
        .versions
        .get(&(project, runtime.clone()))
        .and_then(|versions| match version {
          Some(v) => versions.get(&v),
          None => versions.last_key_value().map(|(_, s)| s),
        })
        .cloned(),
    )
  }
  async fn replay_factory_runtime_settings(
    &self,
    request: ManagementMutation<ReplayFactoryRuntimeSettings>,
  ) -> Result<Option<FactoryRuntimeSettingsOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    self
      .lock()?
      .runtime_settings
      .replay(&audit.scoped_idempotency_key(&request.idempotency_key), &request.intent)
  }
  async fn publish_factory_runtime_settings(
    &self,
    request: ManagementMutation<PublishFactoryRuntimeSettings>,
  ) -> Result<FactoryRuntimeSettingsOutcome, FactoryRuntimeSettingsPublicationError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    if let Some(prior) = self.lock()?.runtime_settings.replay(&key, &request.intent)? {
      return Ok(prior);
    }
    let external = match &self.capabilities {
      Some(source) => Some(source.availability(request.intent.project_id).await?),
      None => None,
    };
    let mut state = self.lock()?;
    if let Some(prior) = state.runtime_settings.replay(&key, &request.intent)? {
      return Ok(prior);
    }
    let intent = &request.intent;
    if !state.projects.contains(&intent.project_id) {
      return Err(
        StoreError::NotFound {
          entity: EntityKind::Project,
        }
        .into(),
      );
    }
    let current_version = state
      .runtime_settings
      .versions
      .get(&(intent.project_id, intent.runtime.clone()))
      .and_then(|versions| versions.last_key_value().map(|(version, _)| *version));
    if current_version != intent.expected_version {
      return Err(FactoryRuntimeSettingsPublicationError::PreconditionFailed);
    }
    let settings = intent.settings()?;
    let installed = external.unwrap_or_else(|| {
      state.choices.get(&intent.project_id).cloned().map_or(
        FactoryConfigurationAvailability::Disabled,
        FactoryConfigurationAvailability::Available,
      )
    });
    let current = state
      .runtime_settings
      .current(intent.project_id)
      .filter(|settings| settings.runtime() != &intent.runtime)
      .cloned();
    configured_factory_catalog(installed, current.chain([settings.clone()])).map_err(|_| invalid_runtime_settings())?;
    let outcome = FactoryRuntimeSettingsOutcome {
      disposition: MutationDisposition::Applied,
      settings: settings.clone(),
      published_at: request.published_at,
    };
    state
      .runtime_settings
      .versions
      .entry((intent.project_id, intent.runtime.clone()))
      .or_default()
      .insert(settings.version(), settings);
    state
      .runtime_settings
      .mutations
      .insert(key.clone(), (request.intent.clone(), outcome.clone()));
    state.evidence.insert(format!(
      "publish-factory-runtime-settings:{}:{}",
      key.security_scope().as_str(),
      key.caller_key()
    ));
    state.audit.insert(recorded_management_audit(
      &audit,
      StoreOperation::PublishFactoryRuntimeSettings,
      EntityKind::Project,
      intent.project_id,
    ));
    Ok(outcome)
  }
}
