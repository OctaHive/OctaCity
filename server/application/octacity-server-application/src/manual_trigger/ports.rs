use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{ImmutableRevision, ProjectId};
use octacity_server_store::{
  ConfigurationStore, PipelineStore, ProjectPolicyStore, StoreError, TriggerDefinitionRef, TriggerDefinitionStore,
  TriggerKind, TriggerTarget,
};

use crate::{EffectiveProjectPolicy, ProjectPolicyLayer, resolve_project_policy};

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

/// Effective-policy source backed by immutable store documents.
pub struct StoreBackedEffectiveProjectPolicySource<S> {
  store: Arc<S>,
}

impl<S> StoreBackedEffectiveProjectPolicySource<S> {
  /// Creates the source from a backend-neutral policy lineage port.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> EffectiveProjectPolicySource for StoreBackedEffectiveProjectPolicySource<S>
where
  S: ProjectPolicyStore + 'static,
{
  async fn effective_policy(
    &self,
    project_id: ProjectId,
  ) -> Result<EffectiveProjectPolicy, EffectiveProjectPolicySourceError> {
    let documents = self
      .store
      .project_policy_lineage(project_id)
      .await
      .map_err(map_policy_store_error)?;
    let layers = documents
      .into_iter()
      .map(|document| {
        let mut policy = document.policy;
        let object = policy
          .as_object_mut()
          .ok_or(EffectiveProjectPolicySourceError::Invalid)?;
        object.insert("project_id".to_owned(), serde_json::json!(document.project_id));
        object.insert("parent_id".to_owned(), serde_json::json!(document.parent_id));
        object.insert("version".to_owned(), serde_json::json!(document.version));
        serde_json::from_value::<ProjectPolicyLayer>(policy).map_err(|_| EffectiveProjectPolicySourceError::Invalid)
      })
      .collect::<Result<Vec<_>, _>>()?;
    resolve_project_policy(&layers).map_err(|_| EffectiveProjectPolicySourceError::Invalid)
  }
}

fn map_policy_store_error(error: StoreError) -> EffectiveProjectPolicySourceError {
  match error {
    StoreError::NotFound { .. } => EffectiveProjectPolicySourceError::NotFound,
    StoreError::Unavailable => EffectiveProjectPolicySourceError::Unavailable,
    _ => EffectiveProjectPolicySourceError::Invalid,
  }
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

/// Resolver for the pre-VCS vertical slice that accepts only an exact immutable
/// revision already permitted by Repository policy.
pub struct ExactRevisionResolver;

#[async_trait]
impl RevisionResolver for ExactRevisionResolver {
  async fn resolve(&self, request: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
    match request.selection {
      super::model::ManualSourceSelection::ExactRevision(revision) => Ok(revision),
      super::model::ManualSourceSelection::DefaultReference | super::model::ManualSourceSelection::Reference(_) => {
        Err(RevisionResolutionError::Unavailable)
      }
    }
  }
}
