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
    let current = state
      .runtime_settings
      .versions
      .get(&(intent.project_id, intent.runtime.clone()))
      .and_then(|rows| rows.last_key_value().map(|(v, _)| *v));
    if current != intent.expected_version {
      return Err(FactoryRuntimeSettingsPublicationError::PreconditionFailed);
    }
    let settings = intent.settings()?;
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
