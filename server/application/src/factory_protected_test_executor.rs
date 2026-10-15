//! Profile-selected protected authoring through existing ordinary Build ports.
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_store::{BuildQueryStore, FactoryRunStore};
use std::{collections::BTreeSet, sync::Arc};

/// Selects one exact installed author policy before delegating bundle verification.
/// Ordinary Build creation, cancellation and retained byte checks remain in the
/// existing command pipeline; profile selection cannot come from output content.
pub struct FactoryProtectedTestAuthoringExecutor<B, O, S> {
  builds: Arc<B>,
  outputs: Arc<O>,
  runs: Arc<S>,
  profiles: Vec<FactoryProtectedTestAuthoringProfile>,
}
impl<B, O, S> FactoryProtectedTestAuthoringExecutor<B, O, S> {
  /// Rejects empty, ambiguous or mixed technical executor installations.
  pub fn new(
    builds: Arc<B>,
    outputs: Arc<O>,
    runs: Arc<S>,
    profiles: Vec<FactoryProtectedTestAuthoringProfile>,
  ) -> Result<Self, ApplicationError> {
    let first = profiles.first().ok_or_else(ApplicationError::invalid)?;
    let mut seen = BTreeSet::new();
    if profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || profiles.iter().any(|profile| {
        profile.command().executor != first.command().executor || !seen.insert(profile.command().task_digest())
      })
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      builds,
      outputs,
      runs,
      profiles,
    })
  }
}
#[async_trait]
impl<B, O, S> FactoryNodeExecutor for FactoryProtectedTestAuthoringExecutor<B, O, S>
where
  B: OrdinaryBuildCancellationApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
  S: FactoryRunStore,
{
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let intent = request.build.ok_or_else(ApplicationError::invalid)?;
    let profile = self
      .profiles
      .iter()
      .find(|profile| profile.command().matches_intent(intent))
      .ok_or_else(ApplicationError::invalid)?;
    let plans = Arc::new(FactoryStoredProtectedTestPlanSource::new(
      self.runs.clone(),
      profile.command().identity.clone(),
      profile.policy().clone(),
    ));
    FactoryProtectedTestAuthoringAdapter::new(
      self.builds.clone(),
      self.outputs.clone(),
      plans,
      profile.inventory().clone(),
    )
    .observe(request)
    .await
  }
  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    at: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    FactoryNodeExecutor::cancel(
      &FactoryNodeBuildAdapter::new(self.builds.clone(), self.outputs.clone()),
      request,
      at,
    )
    .await
  }
}
