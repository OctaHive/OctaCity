//! Candidate recovery from the capture pin of an already qualified Codex profile.
use crate::{
  ApplicationError, FactoryCandidateContextSource, FactoryExecutionClock, FactoryNodeCandidateContextSource,
  FactoryRetainedCandidateContextSource, FactoryStoredCandidateContextSource,
};
use octacity_artifact_store::ArtifactByteReader;
use octacity_server_factory::{FlowBuildIntent, ImmutableReference};
use octacity_server_store::{ArtifactRecordStore, BuildQueryStore, FactoryRunStore};
use std::sync::Arc;

/// A candidate reader for both fenced dispatch and retained observation.
pub trait FactoryCodexCandidateReader:
  FactoryNodeCandidateContextSource + FactoryRetainedCandidateContextSource
{
}

impl<T: FactoryNodeCandidateContextSource + FactoryRetainedCandidateContextSource> FactoryCodexCandidateReader for T {}

/// Binds an installed capture pin to the common candidate reader.
#[async_trait::async_trait]
pub trait FactoryCodexCandidateReaderFactory: Send + Sync {
  /// Returns the verified producer's tool for the candidate frozen in this intent.
  async fn retained_capture_tool(&self, intent: &FlowBuildIntent) -> Result<ImmutableReference, ApplicationError>;

  /// The caller supplies the pin from the profile already qualified against the frozen intent.
  fn for_capture(&self, capture_tool: &ImmutableReference) -> Box<dyn FactoryCodexCandidateReader + '_>;
}

/// Reuses the retained candidate reader without resolving a second profile.
pub struct FactoryCodexCandidateContextSource<R, A, Q, B> {
  runs: Arc<R>,
  source: Arc<FactoryCandidateContextSource<A, Q, B>>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<R, A, Q, B> FactoryCodexCandidateContextSource<R, A, Q, B> {
  /// Installs authoritative journals, the existing Artifact reader and trusted time.
  pub fn new(
    runs: Arc<R>,
    source: Arc<FactoryCandidateContextSource<A, Q, B>>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self { runs, source, clock }
  }
}

#[async_trait::async_trait]
impl<R: FactoryRunStore, A: ArtifactRecordStore, Q: BuildQueryStore, B: ArtifactByteReader>
  FactoryCodexCandidateReaderFactory for FactoryCodexCandidateContextSource<R, A, Q, B>
{
  async fn retained_capture_tool(&self, intent: &FlowBuildIntent) -> Result<ImmutableReference, ApplicationError> {
    use crate::factory::artifacts::factory_stored_candidate_context::{
      selected_candidate_capture, verify_retained_consumer,
    };
    let snapshot = self.runs.factory_run_snapshot(intent.input().factory_run_id()).await?;
    verify_retained_consumer(&snapshot, intent)?;
    let (_, manifest) = selected_candidate_capture(&snapshot, intent)?;
    Ok(manifest.producer.tool().clone())
  }

  fn for_capture(&self, capture_tool: &ImmutableReference) -> Box<dyn FactoryCodexCandidateReader + '_> {
    Box::new(FactoryStoredCandidateContextSource::new(
      self.runs.clone(),
      self.source.clone(),
      capture_tool.clone(),
      self.clock.clone(),
    ))
  }
}

pub(super) fn classify_selection(error: ApplicationError) -> crate::OrdinaryBuildApplicationError {
  if error.classification() == crate::ApplicationFailure::Unavailable {
    crate::OrdinaryBuildApplicationError::Unavailable
  } else {
    crate::OrdinaryBuildApplicationError::Invalid
  }
}
