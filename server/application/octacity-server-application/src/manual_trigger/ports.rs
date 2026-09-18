use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{ImmutableRevision, ProjectId};
use octacity_server_store::{
  ConfigurationStore, PipelineStore, TriggerDefinitionRef, TriggerDefinitionStore, TriggerKind, TriggerTarget,
};

use crate::EffectiveProjectPolicy;

use super::model::{
  EffectiveProjectPolicySourceError, JobSpecToolchainPolicy, ManualTriggerContext, ManualTriggerContextError,
  RevisionResolutionError, RevisionResolutionRequest,
};

/// Loads immutable server-owned context for a manual Trigger command.
#[async_trait]
pub trait ManualTriggerContextProvider: Send + Sync {
  /// Validates the exact enabled manual Trigger and loads its immutable context.
  async fn load(
    &self,
    trigger: TriggerDefinitionRef,
    target: TriggerTarget,
  ) -> Result<ManualTriggerContext, ManualTriggerContextError>;
}

/// Supplies already-resolved effective Project policy to application use cases.
#[async_trait]
pub trait EffectiveProjectPolicySource: Send + Sync {
  /// Loads and resolves the immutable policy lineage ending at `project_id`.
  async fn effective_policy(
    &self,
    project_id: ProjectId,
  ) -> Result<EffectiveProjectPolicy, EffectiveProjectPolicySourceError>;
}

/// Default context provider backed by the backend-neutral store query ports.
pub struct StoreBackedManualTriggerContext<S, P> {
  store: Arc<S>,
  policies: Arc<P>,
  toolchain: JobSpecToolchainPolicy,
}

impl<S, P> StoreBackedManualTriggerContext<S, P> {
  /// Creates a context provider from immutable store queries and policy resolution.
  pub fn new(store: Arc<S>, policies: Arc<P>, toolchain: JobSpecToolchainPolicy) -> Self {
    Self {
      store,
      policies,
      toolchain,
    }
  }
}

#[async_trait]
impl<S, P> ManualTriggerContextProvider for StoreBackedManualTriggerContext<S, P>
where
  S: ConfigurationStore + PipelineStore + TriggerDefinitionStore + 'static,
  P: EffectiveProjectPolicySource + 'static,
{
  async fn load(
    &self,
    trigger: TriggerDefinitionRef,
    target: TriggerTarget,
  ) -> Result<ManualTriggerContext, ManualTriggerContextError> {
    self
      .store
      .require_enabled_trigger(trigger, TriggerKind::Manual, target)
      .await
      .map_err(ManualTriggerContextError::Store)?;
    let configuration = self
      .store
      .build_configuration_version(target.configuration_id, target.configuration_version)
      .await
      .map_err(ManualTriggerContextError::Store)?;
    let repository = self
      .store
      .repository_version(
        configuration.definition.repository_id,
        configuration.definition.repository_version,
      )
      .await
      .map_err(ManualTriggerContextError::Store)?;
    let pipeline = self
      .store
      .pipeline_version(
        configuration.definition.pipeline_id,
        configuration.definition.pipeline_version,
      )
      .await
      .map_err(ManualTriggerContextError::Store)?;
    let effective_policy = self
      .policies
      .effective_policy(configuration.project_id)
      .await
      .map_err(ManualTriggerContextError::Policy)?;
    Ok(ManualTriggerContext {
      configuration,
      repository,
      pipeline,
      effective_policy,
      job_spec_toolchain: self.toolchain.clone(),
    })
  }
}

/// Resolves or verifies one allowed source expression without materializing a workspace.
#[async_trait]
pub trait RevisionResolver: Send + Sync {
  /// Returns the exact immutable provider-native revision selected for the Build.
  async fn resolve(&self, request: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError>;
}
