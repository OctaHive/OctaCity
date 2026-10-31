//! Immutable ordinary Build context shared by Trigger and Factory admission.
use crate::{
  EffectiveProjectPolicy, EffectiveProjectPolicySource, EffectiveProjectPolicySourceError, JobSpecToolchainPolicy,
};
use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, ProjectId};
use octacity_server_store::{
  ConfigurationStore, PipelineStore, PublishedBuildConfiguration, PublishedPipeline, PublishedRepository, StoreError,
};
use std::sync::Arc;

/// Exact server-owned configuration and policy used to compile ordinary Jobs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildExecutionContext {
  /// Selected immutable Build Configuration.
  pub configuration: PublishedBuildConfiguration,
  /// Exact selected Repository, absent only for explicit source-free configuration.
  pub repository: Option<PublishedRepository>,
  /// Pipeline version selected by that configuration.
  pub pipeline: PublishedPipeline,
  /// Effective Project policy to freeze at Build acceptance.
  pub effective_policy: EffectiveProjectPolicy,
  /// Operator-approved source and Octa toolchain.
  pub job_spec_toolchain: JobSpecToolchainPolicy,
}

/// Secret-safe failure while resolving immutable execution context.
#[derive(Debug, thiserror::Error)]
pub enum BuildExecutionContextError {
  /// An authoritative version read failed.
  #[error("Build execution context is unavailable")]
  Store(#[source] StoreError),
  /// Effective Project policy could not be resolved.
  #[error("Build execution policy is unavailable")]
  Policy(#[source] EffectiveProjectPolicySourceError),
  /// Returned identities or Project ownership differ from the requested closure.
  #[error("Build execution context is inconsistent")]
  Invalid,
}

/// Resolves an exact Project-owned execution closure independently of Triggers.
#[async_trait::async_trait]
pub trait BuildExecutionContextSource: Send + Sync {
  /// Rejects missing immutable versions and cross-Project substitutions.
  async fn load(
    &self,
    project: ProjectId,
    id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<BuildExecutionContext, BuildExecutionContextError>;
}

/// Loads the exact immutable closure without creating or requiring a Trigger.
pub struct StoreBackedBuildExecutionContext<C, L, P> {
  configurations: Arc<C>,
  pipelines: Arc<L>,
  policies: Arc<P>,
  toolchain: JobSpecToolchainPolicy,
}
impl<C: ConfigurationStore, L: PipelineStore, P: EffectiveProjectPolicySource>
  StoreBackedBuildExecutionContext<C, L, P>
{
  /// Installs existing configuration, Pipeline and inherited-policy readers.
  pub fn new(configurations: Arc<C>, pipelines: Arc<L>, policies: Arc<P>, toolchain: JobSpecToolchainPolicy) -> Self {
    Self {
      configurations,
      pipelines,
      policies,
      toolchain,
    }
  }
  /// Reads only the requested version and rejects cross-Project substitution.
  pub async fn load(
    &self,
    project: ProjectId,
    id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<BuildExecutionContext, BuildExecutionContextError> {
    self.load_inner(Some(project), id, version).await
  }
  pub(crate) async fn load_for_configuration(
    &self,
    id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<BuildExecutionContext, BuildExecutionContextError> {
    self.load_inner(None, id, version).await
  }
  async fn load_inner(
    &self,
    expected_project: Option<ProjectId>,
    id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<BuildExecutionContext, BuildExecutionContextError> {
    let configuration = self
      .configurations
      .build_configuration_version(id, version)
      .await
      .map_err(BuildExecutionContextError::Store)?;
    let project = configuration.project_id;
    if expected_project.is_some_and(|expected| expected != project)
      || configuration.id != id
      || configuration.version != version
    {
      return Err(BuildExecutionContextError::Invalid);
    }
    let definition = &configuration.definition;
    definition.validate().map_err(|_| BuildExecutionContextError::Invalid)?;
    let repository = match (definition.repository_id, definition.repository_version) {
      (Some(id), Some(version)) => Some(
        self
          .configurations
          .repository_version(id, version)
          .await
          .map_err(BuildExecutionContextError::Store)?,
      ),
      (None, None) => None,
      _ => return Err(BuildExecutionContextError::Invalid),
    };
    let pipeline = self
      .pipelines
      .pipeline_version(definition.pipeline_id, definition.pipeline_version)
      .await
      .map_err(BuildExecutionContextError::Store)?;
    let effective_policy = self
      .policies
      .effective_policy(project)
      .await
      .map_err(BuildExecutionContextError::Policy)?;
    if repository.as_ref().is_some_and(|repository| {
      repository.project_id != project
        || Some(repository.id) != definition.repository_id
        || Some(repository.version) != definition.repository_version
    }) || pipeline.project_id != project
      || pipeline.id != definition.pipeline_id
      || pipeline.version != definition.pipeline_version
      || effective_policy
        .sources
        .last()
        .is_none_or(|source| source.project_id != project)
    {
      return Err(BuildExecutionContextError::Invalid);
    }
    Ok(BuildExecutionContext {
      configuration,
      repository,
      pipeline,
      effective_policy,
      job_spec_toolchain: self.toolchain.clone(),
    })
  }
}

#[async_trait::async_trait]
impl<C: ConfigurationStore, L: PipelineStore, P: EffectiveProjectPolicySource> BuildExecutionContextSource
  for StoreBackedBuildExecutionContext<C, L, P>
{
  async fn load(
    &self,
    project: ProjectId,
    id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<BuildExecutionContext, BuildExecutionContextError> {
    StoreBackedBuildExecutionContext::load(self, project, id, version).await
  }
}
