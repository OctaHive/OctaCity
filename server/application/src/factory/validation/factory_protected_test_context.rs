//! Retained private test selection over the common journal and ordinary Artifacts.
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactByteReader, MAX_ARTIFACT_READ_BYTES};
use octacity_protocol::ProtectedInputV3;
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
  job_inputs: Vec<ProtectedInputV3>,
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
/// Private signed-Job inputs for one exact observation of the original Work revision.
pub struct FactoryProtectedTestObservationInputs {
  input_digest: FactoryDigest,
  inputs: Vec<ProtectedInputV3>,
  retained_until: Timestamp,
  reserved_inputs: u16,
  classes: Vec<FactoryKey>,
}

impl FactoryProtectedTestObservationInputs {
  #[cfg(test)]
  pub(crate) fn new(
    input: &FlowNodeInput,
    bundle: &ProtectedTestBundle,
    manifest: &ArtifactRecord,
    files: &[ArtifactRecord],
    at: Timestamp,
  ) -> Result<Self, ApplicationError> {
    let job_inputs = factory_protected_test_materialization(input, bundle, manifest, files, at)?;
    Self::from_verified(input, bundle, job_inputs)
  }

  fn from_verified(
    input: &FlowNodeInput,
    bundle: &ProtectedTestBundle,
    job_inputs: Vec<ProtectedInputV3>,
  ) -> Result<Self, ApplicationError> {
    if !matches!(
      input.context().map(|context| context.subject()),
      Some(FactoryTaskSubject::Exact(_))
    ) {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      input_digest: input.digest().map_err(|_| ApplicationError::invalid())?,
      inputs: job_inputs,
      retained_until: bundle.retained_until(),
      reserved_inputs: bundle.plan().reserved_inputs(),
      classes: bundle.plan().classes().to_vec(),
    })
  }
  pub(crate) fn classes(&self) -> &[FactoryKey] {
    &self.classes
  }
  pub(crate) fn matches(&self, input: &FlowNodeInput) -> Result<bool, ApplicationError> {
    Ok(self.input_digest == input.digest().map_err(|_| ApplicationError::invalid())?)
  }
  pub(crate) fn inputs(&self) -> &[ProtectedInputV3] {
    &self.inputs
  }
  pub(crate) fn retained_until(&self) -> Timestamp {
    self.retained_until
  }
  pub(crate) fn reserved_inputs(&self) -> u16 {
    self.reserved_inputs
  }
}

/// Restores the selected private inputs for a qualified command observing the Work base.
#[async_trait]
pub trait FactoryProtectedTestObservationInputSource: Send + Sync {
  /// Resolves the exact persisted bundle and returns its read-only signed-Job inputs.
  async fn observation_inputs(
    &self,
    intent: &FlowBuildIntent,
  ) -> Result<FactoryProtectedTestObservationInputs, ApplicationError>;
}

/// Restores only explicitly selected private metadata from an installed capture contract.
/// No latest pointer, process cache, logical node name or model producer assertion is used.
pub struct FactoryStoredProtectedTestContextSource<R, A, Q, B, P> {
  runs: Arc<R>,
  reader: Arc<FactoryArtifactReader<A, Q, B>>,
  plans: Arc<P>,
  inventory: EvidenceRequirement,
  accepts_supplement: bool,
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
      accepts_supplement: false,
      clock,
    }
  }
  /// Enables supplemental inputs only for verifier programs installed with that capability.
  #[must_use]
  pub fn with_verification_supplement(mut self, supported: bool) -> Self {
    self.accepts_supplement = supported;
    self
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
      || plan.supplement().is_some() && !self.accepts_supplement
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
    let job_inputs = factory_protected_test_materialization(input, &bundle, retained.record(), &files, finished)?;
    Ok(FactoryProtectedTestContext {
      bundle,
      manifest: document.record,
      files,
      job_inputs,
    })
  }
}

#[async_trait]
impl<
  R: FactoryRunStore,
  A: ArtifactRecordStore,
  Q: BuildQueryStore,
  B: ArtifactByteReader,
  P: FactoryProtectedTestPlanSource,
> FactoryProtectedTestObservationInputSource for FactoryStoredProtectedTestContextSource<R, A, Q, B, P>
{
  async fn observation_inputs(
    &self,
    intent: &FlowBuildIntent,
  ) -> Result<FactoryProtectedTestObservationInputs, ApplicationError> {
    use crate::factory::artifacts::factory_stored_candidate_context::verify_retained_consumer;
    let input = intent.input();
    let invalid = ApplicationError::invalid;
    if intent.node().node_kind() != FlowNodeKind::BuildCommand
      || intent.profile().capture_deliverables.is_some()
      || !super::factory_protected_validation::readonly_test_mounts(intent)
      || !matches!(
        input.context().map(|context| context.subject()),
        Some(FactoryTaskSubject::Exact(_))
      )
    {
      return Err(invalid());
    }
    let at = self.clock.now()?.max(intent.node().started_at());
    let snapshot = self.runs.factory_run_snapshot(input.factory_run_id()).await?;
    verify_retained_consumer(&snapshot, intent)?;
    let (until, completed) = observation_window(&snapshot, intent, at)?;
    let selected = self.read(input, at, until).await?;
    let finished = self.clock.now()?.max(at);
    let current = self.runs.factory_run_snapshot(input.factory_run_id()).await?;
    verify_retained_consumer(&current, intent)?;
    if (!completed && finished >= until) || finished >= selected.bundle.retained_until() {
      return Err(invalid());
    }
    FactoryProtectedTestObservationInputs::from_verified(input, &selected.bundle, selected.job_inputs)
  }
}

pub(super) fn observation_window(
  snapshot: &octacity_server_store::FactoryRunSnapshot,
  intent: &FlowBuildIntent,
  at: Timestamp,
) -> Result<(Timestamp, bool), ApplicationError> {
  let completed = snapshot
    .flow
    .completions
    .iter()
    .any(|row| row.node_attempt_id() == intent.node().id());
  let until = if completed {
    at.unix_millis()
      .checked_add(1)
      .and_then(|value| Timestamp::from_unix_millis(value).ok())
      .ok_or_else(ApplicationError::invalid)?
  } else {
    intent.node().deadline()
  };
  if at >= until {
    return Err(ApplicationError::invalid());
  }
  Ok((until, completed))
}
