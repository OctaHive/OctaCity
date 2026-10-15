//! Trusted evidence publication over an explicitly selected ordinary validation completion.
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::ArtifactByteReader;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::{ArtifactRecordStore, BuildQueryStore, FactoryRunStore};
use serde::Serialize;
use std::sync::Arc;

/// Frozen operator contracts; input payloads cannot select a different verifier or schema.
#[derive(Clone, Serialize)]
pub struct FactoryProtectedValidationEvidenceProfile {
  /// Exact installed action selector, independent of logical Flow and node names.
  pub action: FactoryKey,
  /// Published complete-manifest outcome and schema.
  pub ready: FlowOutcomeDefinition,
  /// Exact report checked against the materialized candidate and protected tests.
  pub report: EvidenceRequirement,
  /// Exact independently verified qualification facts retained in the Build journal.
  pub facts: EvidenceRequirement,
}
impl FactoryProtectedValidationEvidenceProfile {
  /// Content identity binds the action, outcome and both immutable output requirements.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let bytes = serde_json::to_vec(self).map_err(|_| invalid_profile())?;
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid_profile());
    }
    Ok(FactoryDigest::sha256(
      "octacity.factory.protected-validation-evidence-profile.v1",
      &[&bytes],
    ))
  }
}
fn invalid_profile() -> FactoryError {
  FactoryError::InvalidReference {
    relationship: "protected validation evidence profile",
  }
}
/// Installed trusted action; the common Node owner persists its immutable result and replay.
pub struct FactoryProtectedValidationEvidenceAction<S, A, Q, B, V> {
  identity: ImmutableReference,
  store: Arc<S>,
  reader: Arc<FactoryArtifactReader<A, Q, B>>,
  inputs: Arc<V>,
  profile: FactoryProtectedValidationEvidenceProfile,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S, A, Q, B, V> FactoryProtectedValidationEvidenceAction<S, A, Q, B, V> {
  /// Binds authoritative history, exact retained reads and the original trusted materialization.
  pub fn new(
    identity: ImmutableReference,
    store: Arc<S>,
    reader: Arc<FactoryArtifactReader<A, Q, B>>,
    inputs: Arc<V>,
    profile: FactoryProtectedValidationEvidenceProfile,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      identity,
      store,
      reader,
      inputs,
      profile,
      clock,
    }
  }
}
#[async_trait]
impl<
  S: FactoryRunStore,
  A: ArtifactRecordStore,
  Q: BuildQueryStore,
  B: ArtifactByteReader,
  V: FactoryProtectedValidationInputSource,
> FactoryFlowActionPlugin for FactoryProtectedValidationEvidenceAction<S, A, Q, B, V>
{
  fn identity(&self) -> ImmutableReference {
    self.identity.clone()
  }
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    if binding.plugin() != &self.identity
      || binding.action() != &self.profile.action
      || binding.parameters() != &serde_json::json!({})
      || self.identity.digest() != self.profile.digest().map_err(|_| ApplicationError::invalid())?
      || !outcomes.contains(&self.profile.ready)
      || self.profile.report.kind() == self.profile.facts.kind()
      || self.profile.report.output_kind() != EvidenceOutputKind::Report
      || self.profile.facts.output_kind() != EvidenceOutputKind::Report
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
  async fn cancel(
    &self,
    _: FactoryFlowActionRequest<'_>,
    _: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    Ok(Some(BudgetUsage::default()))
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Selected {
      report: FactoryArtifactReference,
      facts: FactoryArtifactReference,
    }
    let invalid = ApplicationError::invalid;
    let snapshot = self.store.factory_run_snapshot(request.input.factory_run_id()).await?;
    let at = self.clock.now()?.max(request.observed_at);
    let [source] = request.input.sources() else {
      return Err(invalid());
    };
    if snapshot.work.id() != request.work_id
      || snapshot.work.subject() != request.subject
      || !snapshot.flow.data.inputs.contains(request.input)
      || at >= request.deadline
      || at >= request.authority_expires_at
    {
      return Err(invalid());
    }
    let selected: Selected = serde_json::from_value(request.input.payload().value().clone()).map_err(|_| invalid())?;
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|row| row.node_attempt_id() == source.node_attempt_id())
      .ok_or_else(invalid)?;
    if record.digest().map_err(|_| invalid())? != source.digest() {
      return Err(invalid());
    }
    let provenance = record.build_provenance().ok_or_else(invalid)?;
    let find = |requirement: &EvidenceRequirement, artifact: &FactoryArtifactReference| {
      provenance
        .outputs()
        .iter()
        .find(|output| {
          output.name == *requirement.kind()
            && output.artifact == *artifact
            && output.output_kind == requirement.output_kind()
            && output.schema == *requirement.schema()
            && output.producer.tool() == requirement.tool()
            && output.producer.plugin() == requirement.plugin()
        })
        .ok_or_else(invalid)
    };
    let report = find(&self.profile.report, &selected.report)?;
    let facts = find(&self.profile.facts, &selected.facts)?;
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
          && row.build_id() == report.producer.build_id()
          && row.attempt_id() == report.producer.attempt_id()
      })
      .ok_or_else(invalid)?;
    let completion = snapshot
      .flow
      .completions
      .iter()
      .find(|row| row.node_attempt_id() == record.node_attempt_id())
      .ok_or_else(invalid)?;
    if record.input() != intent.input()
      || provenance.profile() != intent.profile()
      || completion.outcome() != intent.result_outcome()
      || report.verified_at > at
      || facts.verified_at > at
    {
      return Err(invalid());
    }
    let retained_until = at
      .unix_millis()
      .checked_add(1)
      .and_then(|value| Timestamp::from_unix_millis(value).ok())
      .ok_or_else(invalid)?;
    let retained = self
      .reader
      .read(ReadFactoryArtifact {
        project_id: request.subject.project_id(),
        artifact: selected.report,
        maximum_bytes: request
          .budget
          .max_output_bytes()
          .min(octacity_artifact_store::MAX_ARTIFACT_READ_BYTES),
        observed_at: at,
        required_until: retained_until,
      })
      .await?;
    let materialized = self.inputs.materialization(intent).await?;
    let completed = FactoryNodeBuildCompletion {
      execution: execution.clone(),
      record: record.clone(),
      completion: completion.clone(),
      usage: completion.usage(),
    };
    let document = FactoryNodeBuildOutputDocument {
      record: retained.record().clone(),
      bytes: retained.bytes().to_vec(),
      data: report.data.clone(),
      schema: self.profile.report.schema().clone(),
      tool: self.profile.report.tool().clone(),
      plugin: self.profile.report.plugin().clone(),
    };
    let id = EvidenceManifestId::from_uuid(uuid::Uuid::new_v5(
      &request.input.flow_run_id().as_uuid(),
      &request.operation_id.as_bytes(),
    ))
    .map_err(|_| invalid())?;
    let manifest = construct_factory_protected_validation_evidence_manifest(
      id,
      &materialized,
      &completed,
      &document,
      &self.profile.report,
      &self.profile.facts,
      request.started_at,
    )?;
    let schema = request
      .schemas
      .iter()
      .find(|schema| schema.reference() == self.profile.ready.schema())
      .ok_or_else(invalid)?;
    let payload =
      FlowPayload::new(schema, serde_json::to_value(&manifest).map_err(|_| invalid())?).map_err(|_| invalid())?;
    let output_bytes = serde_json::to_vec(&payload).map_err(|_| invalid())?.len() as u64;
    let finished = self.clock.now()?.max(at);
    if output_bytes > request.budget.max_output_bytes()
      || finished >= request.deadline
      || finished >= request.authority_expires_at
      || manifest.items().iter().any(|item| finished >= item.fresh_until())
    {
      return Err(invalid());
    }
    Ok(FactoryFlowActionObservation {
      discovery: Some(FlowDiscoveryFacts::evidence(&manifest).map_err(|_| invalid())?),
      outcome: self.profile.ready.key().clone(),
      payload,
      usage: BudgetUsage {
        output_bytes,
        elapsed_millis: u64::try_from(
          finished
            .unix_millis()
            .checked_sub(request.started_at.unix_millis())
            .ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?,
        ..Default::default()
      },
      outbox: vec![],
    })
  }
}
