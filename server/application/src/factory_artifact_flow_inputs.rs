//! Context collection for deployments that install retained Artifact inputs.
use crate::{
  ApplicationError, FactoryArtifactReader, FactoryExecutionClock, FactoryFlowInputPreparer, FactoryNodeTarget,
  ReadFactoryArtifact, prepare_factory_flow_input,
};
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactByteReader, MAX_ARTIFACT_READ_BYTES};
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::{ArtifactRecordStore, BuildQueryStore, FactoryRunSnapshot};
use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

/// Resolves explicit Artifact projections; repository retrieval is a separately installed capability.
/// This owner adds no task, predecessor or stage context implicitly.
pub struct FactoryArtifactFlowInputs<A, S, B> {
  reader: Arc<FactoryArtifactReader<A, S, B>>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<A, S, B> FactoryArtifactFlowInputs<A, S, B> {
  /// Installs qualified ordinary Artifact reads and trusted time.
  pub fn new(reader: Arc<FactoryArtifactReader<A, S, B>>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self { reader, clock }
  }
}
#[async_trait]
impl<A: ArtifactRecordStore, S: BuildQueryStore, B: ArtifactByteReader> FactoryFlowInputPreparer
  for FactoryArtifactFlowInputs<A, S, B>
{
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    let input = prepare_factory_flow_input(snapshot, target)?;
    let node = snapshot
      .admitted_flow
      .closure()
      .definition(input.definition())
      .and_then(|definition| definition.node(input.node()))
      .ok_or_else(ApplicationError::invalid)?;
    let Some(binding) = node.context_binding() else {
      if node.build().is_none() {
        return Ok(input);
      }
      let subject = FactoryTaskSubject::Exact(snapshot.work.subject().clone());
      let policy = FactoryDigest::sha256("octacity.factory.artifact-context-policy.v1", &[b"null"]);
      return freeze_context(input, subject, policy, vec![]);
    };
    let subject = match binding
      .candidate_revision(input.payload())
      .map_err(|_| ApplicationError::invalid())?
    {
      None => FactoryTaskSubject::Exact(snapshot.work.subject().clone()),
      Some(revision) => FactoryTaskSubject::Candidate(selected_candidate(snapshot, &input, &revision)?),
    };
    let selected = binding
      .selected_sources(input.payload())
      .map_err(|_| ApplicationError::invalid())?;
    let policy_bytes = serde_json::to_vec(binding).map_err(|_| ApplicationError::invalid())?;
    let policy = FactoryDigest::sha256("octacity.factory.artifact-context-policy.v1", &[&policy_bytes]);
    let observed_at = self.clock.now()?;
    let required_until = Timestamp::from_unix_millis(
      observed_at
        .unix_millis()
        .checked_add(i64::try_from(node.budget().max_elapsed_millis()).map_err(|_| ApplicationError::invalid())?)
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let mut remaining = MAX_CONTEXT_MANIFEST_BYTES;
    let mut entries = vec![];
    for (alias, (kind, source)) in selected {
      let FactoryContextReference::Artifact(artifact) = &source else {
        return Err(ApplicationError::invalid());
      };
      let retained = self
        .reader
        .read_input(ReadFactoryArtifact {
          project_id: snapshot.work.subject().project_id(),
          artifact: artifact.clone(),
          maximum_bytes: remaining.min(MAX_ARTIFACT_READ_BYTES),
          observed_at: self.clock.now()?,
          required_until,
        })
        .await
        .map_err(|error| ApplicationError::context_input(alias.clone(), &error))?;
      remaining = remaining
        .checked_sub(artifact.encoded_size())
        .ok_or_else(ApplicationError::invalid)?;
      entries.push(
        ContextManifestEntry::new(
          kind,
          alias,
          subject.clone(),
          source.clone(),
          artifact.content_digest(),
          artifact.encoded_size(),
          FactorySafeText::new("Explicit immutable context selection").map_err(|_| ApplicationError::invalid())?,
          retained.provenance_digest(),
        )
        .map_err(|_| ApplicationError::invalid())?,
      );
    }
    freeze_context(input, subject, policy, entries)
  }
}

fn freeze_context(
  input: FlowNodeInput,
  subject: FactoryTaskSubject,
  policy: FactoryDigest,
  entries: Vec<ContextManifestEntry>,
) -> Result<FlowNodeInput, ApplicationError> {
  let bytes = serde_json::to_vec(&(
    input.digest().map_err(|_| ApplicationError::invalid())?,
    &subject,
    policy,
    &entries,
  ))
  .map_err(|_| ApplicationError::invalid())?;
  let id = ContextManifestId::from_uuid(uuid::Uuid::new_v5(&input.flow_run_id().as_uuid(), &bytes))
    .map_err(|_| ApplicationError::invalid())?;
  input
    .with_context(
      ContextManifest::new(id, subject, policy, entries).map_err(|_| ApplicationError::invalid())?,
      vec![],
    )
    .map_err(|_| ApplicationError::invalid())
}

/// Follow only content-bound selected data references. Control predecessors and
/// unrelated journal publications never grant candidate provenance.
fn selected_candidate(
  snapshot: &FactoryRunSnapshot,
  input: &FlowNodeInput,
  revision: &octacity_server_domain::ImmutableRevision,
) -> Result<CandidateSubject, ApplicationError> {
  let records = snapshot
    .flow
    .data
    .records
    .iter()
    .map(|record| (record.node_attempt_id(), record))
    .collect::<BTreeMap<_, _>>();
  let inputs = snapshot
    .flow
    .data
    .inputs
    .iter()
    .map(|input| Ok((input.digest().map_err(|_| ApplicationError::invalid())?, input)))
    .collect::<Result<BTreeMap<_, _>, ApplicationError>>()?;
  let mut pending = vec![input];
  let mut seen_inputs = BTreeSet::new();
  let mut seen_records = BTreeSet::new();
  let mut selected = None;
  while let Some(input) = pending.pop() {
    if !seen_inputs.insert(input.digest().map_err(|_| ApplicationError::invalid())?) {
      continue;
    }
    if let Some(caller) = input.caller_input_digest() {
      pending.push(inputs.get(&caller).copied().ok_or_else(ApplicationError::invalid)?);
    }
    for source in input.sources() {
      let record = records
        .get(&source.node_attempt_id())
        .ok_or_else(ApplicationError::invalid)?;
      if record.digest().map_err(|_| ApplicationError::invalid())? != source.digest() {
        return Err(ApplicationError::invalid());
      }
      if !seen_records.insert(source.node_attempt_id()) {
        continue;
      }
      if let Some(candidate) = record
        .discovery()
        .and_then(FlowDiscoveryFacts::published_candidate)
        .filter(|candidate| candidate.exact() == snapshot.work.subject() && candidate.candidate_revision() == revision)
      {
        if selected.as_ref().is_some_and(|other| other != candidate) {
          return Err(ApplicationError::invalid());
        }
        selected = Some(candidate.clone());
      }
      pending.push(record.input());
    }
  }
  selected.ok_or_else(ApplicationError::invalid)
}
