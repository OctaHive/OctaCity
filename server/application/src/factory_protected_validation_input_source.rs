//! Exact original validation inputs composed from retained candidate and private-test proofs.
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactByteReader, MAX_ARTIFACT_READ_BYTES};
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::{ArtifactRecordStore, BuildQueryStore, FactoryRunStore};
use std::sync::Arc;

/// Original immutable compiler selection; this source grants no fresh execution authority.
pub struct FactoryStoredProtectedValidationInputSource<R, A, Q, B, C, P> {
  runs: Arc<R>,
  candidates: Arc<C>,
  tests: Arc<FactoryStoredProtectedTestContextSource<R, A, Q, B, P>>,
  reader: Arc<FactoryArtifactReader<A, Q, B>>,
  capture_tool: ImmutableReference,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<R, A, Q, B, C, P> FactoryStoredProtectedValidationInputSource<R, A, Q, B, C, P> {
  /// Installs existing exact retained readers and the immutable capture implementation.
  pub fn new(
    runs: Arc<R>,
    candidates: Arc<C>,
    tests: Arc<FactoryStoredProtectedTestContextSource<R, A, Q, B, P>>,
    reader: Arc<FactoryArtifactReader<A, Q, B>>,
    capture_tool: ImmutableReference,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      runs,
      candidates,
      tests,
      reader,
      capture_tool,
      clock,
    }
  }
}
#[async_trait]
impl<
  R: FactoryRunStore,
  A: ArtifactRecordStore,
  Q: BuildQueryStore,
  B: ArtifactByteReader,
  C: FactoryRetainedCandidateContextSource,
  P: FactoryProtectedTestPlanSource,
> FactoryProtectedValidationInputSource for FactoryStoredProtectedValidationInputSource<R, A, Q, B, C, P>
{
  async fn materialization(
    &self,
    intent: &FlowBuildIntent,
  ) -> Result<FactoryProtectedValidationMaterialization, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let at = self.clock.now()?.max(intent.node().started_at());
    let snapshot = self.runs.factory_run_snapshot(intent.input().factory_run_id()).await?;
    crate::factory_stored_candidate_context::verify_retained_consumer(&snapshot, intent)?;
    if intent.node().node_kind() != FlowNodeKind::BuildCommand {
      return Err(invalid());
    }
    let completed = snapshot
      .flow
      .completions
      .iter()
      .any(|row| row.node_attempt_id() == intent.node().id());
    let until = if completed {
      at.unix_millis()
        .checked_add(1)
        .and_then(|value| Timestamp::from_unix_millis(value).ok())
        .ok_or_else(invalid)?
    } else {
      intent.node().deadline()
    };
    if at >= until {
      return Err(invalid());
    }
    let candidate = self
      .candidates
      .read_retained_candidate_context(intent, at, until)
      .await?;
    if candidate.input_digest() != intent.input().digest().map_err(|_| invalid())? {
      return Err(invalid());
    }
    let tests = self.tests.read(intent.input(), at, until).await?;
    let manifest_ref = FactoryArtifactReference::new(
      candidate.manifest().identity().artifact_id,
      FactoryDigest::from_bytes(candidate.manifest().identity().digest.as_bytes()),
      candidate.manifest().identity().size_bytes,
    )
    .map_err(|_| invalid())?;
    let mut producers = snapshot.flow.data.records.iter().filter_map(|record| {
      let output = record.build_provenance()?.outputs().iter().find(|output| {
        output.artifact == manifest_ref
          && output.name.as_str() == octacity_protocol::CHANGE_SET_MANIFEST_OUTPUT
          && output.producer.tool() == &self.capture_tool
      })?;
      Some((record, output))
    });
    let (producer, output) = producers.next().ok_or_else(invalid)?;
    if producers.next().is_some() {
      return Err(invalid());
    }
    let manifest = self
      .reader
      .read_record(ReadFactoryArtifact {
        project_id: intent.input().subject().project_id(),
        artifact: manifest_ref,
        maximum_bytes: MAX_ARTIFACT_READ_BYTES,
        observed_at: at,
        required_until: until,
      })
      .await?;
    let deliverables = if intent.profile().capture_deliverables.is_some() {
      let original = snapshot
        .flow
        .data
        .build_intents
        .iter()
        .find(|row| row.node().id() == producer.node_attempt_id())
        .ok_or_else(invalid)?;
      let execution = snapshot
        .flow
        .data
        .build_executions
        .iter()
        .find(|row| {
          row.node_attempt_id() == producer.node_attempt_id()
            && row.build_id() == output.producer.build_id()
            && row.attempt_id() == output.producer.attempt_id()
        })
        .ok_or_else(invalid)?;
      let retained = self
        .reader
        .read(ReadFactoryArtifact {
          project_id: intent.input().subject().project_id(),
          artifact: output.artifact.clone(),
          maximum_bytes: MAX_ARTIFACT_READ_BYTES,
          observed_at: at,
          required_until: until,
        })
        .await?;
      let document = FactoryNodeBuildOutputDocument {
        record: retained.record().clone(),
        bytes: retained.bytes().to_vec(),
        data: None,
        schema: output.schema.clone(),
        tool: self.capture_tool.clone(),
        plugin: output.producer.plugin().clone(),
      };
      Some(
        crate::factory_candidate_deliverables::restore_factory_node_candidate_deliverables(
          producer,
          original,
          execution,
          &document,
          &self.capture_tool,
        )
        .map_err(|_| invalid())?,
      )
    } else {
      None
    };
    let finished = self.clock.now()?.max(at);
    // Only active compilation carries the original Node execution deadline.
    // Completed observation instead rechecks exact original retention below.
    if (!completed && finished >= until) || !candidate.is_retained_at(finished) {
      return Err(invalid());
    }
    let current = self.runs.factory_run_snapshot(intent.input().factory_run_id()).await?;
    crate::factory_stored_candidate_context::verify_retained_consumer(&current, intent)?;
    factory_protected_validation_materialization(FactoryProtectedValidationRequest {
      deliverables: deliverables.as_ref(),
      intent,
      candidate: candidate.candidate().changeset(),
      candidate_bundle: candidate.bundle(),
      candidate_manifest: &manifest,
      tests: tests.bundle(),
      test_manifest: tests.manifest(),
      test_files: tests.files(),
      observed_at: finished,
    })
  }
}
