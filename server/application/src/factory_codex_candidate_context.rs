//! Candidate recovery selected by the existing exact installed Codex profile.
use crate::{
  ApplicationError, FactoryCandidateContext, FactoryCandidateContextSource, FactoryCodexBuildProfile,
  FactoryExecutionClock, FactoryNodeCandidateContextSource, FactoryRetainedCandidateContextSource,
  FactoryStoredCandidateContextSource, MAX_FACTORY_EXECUTOR_BINDINGS, OrdinaryBuildApplicationError,
};
use async_trait::async_trait;
use octacity_artifact_store::ArtifactByteReader;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{FactoryClaimOwnership, FactoryCredentialConsumer, FlowBuildIntent};
use octacity_server_store::{ArtifactRecordStore, BuildQueryStore, FactoryRunStore};
use std::sync::Arc;

/// Uses one installed profile selection for dispatch and readonly observation.
/// A capture pin comes only from that profile, never from a model or manifest.
pub struct FactoryCodexCandidateContextSource<R, A, Q, B> {
  runs: Arc<R>,
  source: Arc<FactoryCandidateContextSource<A, Q, B>>,
  profiles: Vec<FactoryCodexBuildProfile>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<R, A, Q, B> FactoryCodexCandidateContextSource<R, A, Q, B> {
  /// Installs the same profile catalog used by the compiler and output verifier.
  /// Native package/backend qualification remains the composition root's duty.
  pub fn new(
    runs: Arc<R>,
    source: Arc<FactoryCandidateContextSource<A, Q, B>>,
    profiles: Vec<FactoryCodexBuildProfile>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    if profiles.is_empty()
      || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || profiles
        .iter()
        .any(|profile| profile.consumer != FactoryCredentialConsumer::ReadOnlyReview || profile.tool_risk.is_some())
    {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(Self {
      runs,
      source,
      profiles,
      clock,
    })
  }
  fn selected_source(
    &self,
    intent: &FlowBuildIntent,
  ) -> Result<FactoryStoredCandidateContextSource<R, A, Q, B>, ApplicationError> {
    let mut profiles = self.profiles.iter().filter(|profile| profile.matches_intent(intent));
    let profile = profiles.next().ok_or_else(ApplicationError::invalid)?;
    if profiles.next().is_some() {
      return Err(ApplicationError::invalid());
    }
    let capture_tool = profile
      .candidate_capture_tool
      .clone()
      .ok_or_else(ApplicationError::invalid)?;
    Ok(FactoryStoredCandidateContextSource::new(
      self.runs.clone(),
      self.source.clone(),
      capture_tool,
      self.clock.clone(),
    ))
  }
}
#[async_trait]
impl<R: FactoryRunStore, A: ArtifactRecordStore, Q: BuildQueryStore, B: ArtifactByteReader>
  FactoryNodeCandidateContextSource for FactoryCodexCandidateContextSource<R, A, Q, B>
{
  async fn read_candidate_context(
    &self,
    intent: &FlowBuildIntent,
    ownership: &FactoryClaimOwnership,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryCandidateContext, ApplicationError> {
    self
      .selected_source(intent)?
      .read_candidate_context(intent, ownership, observed_at, required_until)
      .await
  }
}
#[async_trait]
impl<R: FactoryRunStore, A: ArtifactRecordStore, Q: BuildQueryStore, B: ArtifactByteReader>
  FactoryRetainedCandidateContextSource for FactoryCodexCandidateContextSource<R, A, Q, B>
{
  async fn read_retained_candidate_context(
    &self,
    intent: &FlowBuildIntent,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryCandidateContext, ApplicationError> {
    self
      .selected_source(intent)?
      .read_retained_candidate_context(intent, observed_at, required_until)
      .await
  }
}
