//! Retained private test selection over the common journal and ordinary Artifacts.
use crate::*;
use octacity_artifact_store::{ArtifactByteReader, MAX_ARTIFACT_READ_BYTES};
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::{ArtifactRecord, ArtifactRecordStore, BuildQueryStore, FactoryRunStore};
use std::sync::Arc;

/// Verified inventory and private file publications selected by one frozen context.
/// File bytes remain private signed Job inputs and are not rendered for a model.
pub struct FactoryProtectedTestContext {
  bundle: ProtectedTestBundle,
  manifest: ArtifactRecord,
  files: Vec<ArtifactRecord>,
}
impl FactoryProtectedTestContext {
  /// Original immutable coverage, subject and retention proof.
  pub const fn bundle(&self) -> &ProtectedTestBundle {
    &self.bundle
  }
  /// Exact independently retained inventory publication.
  pub const fn manifest(&self) -> &ArtifactRecord {
    &self.manifest
  }
  /// Exact file publications; no file bodies are fetched by this source.
  pub fn files(&self) -> &[ArtifactRecord] {
    &self.files
  }
}
/// Restores only explicitly selected private metadata from an installed capture contract.
/// No latest pointer, process cache, logical node name or model producer assertion is used.
pub struct FactoryStoredProtectedTestContextSource<R, A, Q, B, P> {
  runs: Arc<R>,
  reader: Arc<FactoryArtifactReader<A, Q, B>>,
  plans: Arc<P>,
  inventory: EvidenceRequirement,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<
  R: FactoryRunStore,
  A: ArtifactRecordStore,
  Q: BuildQueryStore,
  B: ArtifactByteReader,
  P: FactoryProtectedTestPlanSource,
> FactoryStoredProtectedTestContextSource<R, A, Q, B, P>
{
  /// Installs authoritative history, bounded Artifact reads and the frozen authoring policy.
  pub fn new(
    runs: Arc<R>,
    reader: Arc<FactoryArtifactReader<A, Q, B>>,
    plans: Arc<P>,
    inventory: EvidenceRequirement,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      runs,
      reader,
      plans,
      inventory,
      clock,
    }
  }
  /// Recovers the exact selected inventory while its original retention still covers the window.
  /// This read grants no execution authority and never renews an expired authoring attempt.
  pub async fn read(
    &self,
    input: &FlowNodeInput,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryProtectedTestContext, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let at = self.clock.now()?.max(observed_at);
    let snapshot = self.runs.factory_run_snapshot(input.factory_run_id()).await?;
    if self.inventory.output_kind() != EvidenceOutputKind::Artifact
      || required_until <= observed_at
      || snapshot.work.subject() != input.subject()
      || snapshot.work.id() != input.work_id()
      || snapshot.run.state() == FactoryRunState::Cancelled
    {
      return Err(invalid());
    }
    let context = input.context().ok_or_else(invalid)?;
    let selected = |artifact: &FactoryArtifactReference| {
      context
        .entries()
        .iter()
        .any(|entry| matches!(entry.source(), FactoryContextReference::Artifact(reference) if reference == artifact))
    };
    let mut producers = snapshot.flow.data.records.iter().filter_map(|record| {
      let proof = record.build_provenance()?.outputs().iter().find(|output| {
        output.name == *self.inventory.kind()
          && output.output_kind == self.inventory.output_kind()
          && output.schema == *self.inventory.schema()
          && output.producer.tool() == self.inventory.tool()
          && output.producer.plugin() == self.inventory.plugin()
          && selected(&output.artifact)
      })?;
      Some((record, proof))
    });
    let (record, proof) = producers.next().ok_or_else(invalid)?;
    if producers.next().is_some() {
      return Err(invalid());
    }
    let intent = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .find(|row| row.node().id() == record.node_attempt_id())
      .ok_or_else(invalid)?;
    let execution = snapshot
      .flow
      .data
      .build_executions
      .iter()
      .find(|row| {
        row.node_attempt_id() == record.node_attempt_id()
          && row.build_id() == proof.producer.build_id()
          && row.attempt_id() == proof.producer.attempt_id()
      })
      .ok_or_else(invalid)?;
    if !snapshot
      .flow
      .completions
      .iter()
      .any(|row| row.node_attempt_id() == record.node_attempt_id())
    {
      return Err(invalid());
    }
    let plan = self.plans.frozen_plan(intent).await?;
    if plan.subject().exact().map_err(|_| invalid())? != input.exact_resource().map_err(|_| invalid())?
      || required_until > plan.retain_until()
    {
      return Err(invalid());
    }
    let read = |artifact: FactoryArtifactReference| ReadFactoryArtifact {
      project_id: input.subject().project_id(),
      artifact,
      maximum_bytes: MAX_ARTIFACT_READ_BYTES,
      observed_at: at,
      required_until: plan.retain_until(),
    };
    let retained = self.reader.read(read(proof.artifact.clone())).await?;
    let document = FactoryNodeBuildOutputDocument {
      record: retained.record().clone(),
      bytes: retained.bytes().to_vec(),
      data: None,
      schema: self.inventory.schema().clone(),
      tool: self.inventory.tool().clone(),
      plugin: self.inventory.plugin().clone(),
    };
    let bundle =
      restore_factory_protected_test_bundle(record, intent, execution, &document, &plan, &self.inventory, at)?;
    let mut files = Vec::with_capacity(bundle.files().len());
    for file in bundle.files() {
      files.push(self.reader.read_record(read(file.artifact.clone())).await?);
    }
    let finished = self.clock.now()?.max(at);
    // The requested window establishes minimum retention. It does not impose
    // an I/O deadline or renew the original immutable private-test lifetime.
    if finished >= plan.retain_until() {
      return Err(invalid());
    }
    factory_protected_test_materialization(input, &bundle, retained.record(), &files, finished)?;
    Ok(FactoryProtectedTestContext {
      bundle,
      manifest: document.record,
      files,
    })
  }
}
