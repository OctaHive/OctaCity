use crate::{
  AdmittedFlow, ContextManifest, ExactSubject, FactoryConfigurationRef, FactoryContextReference, FactoryDigest,
  FactoryError, FactoryKey, FactoryRunId, FlowAdmissionLimits, FlowDataSchema, FlowDefinitionRef, FlowNodeRecord,
  FlowPayload, FlowRun, FlowRunId, FlowRuntimeHistory, MAX_FLOW_DATA_BYTES, NodeAttemptId, RetrievalReceipt,
  WorkEnvelope, WorkEnvelopeId, WorkflowCycle, WorkflowCycleId, validate_flow_runtime_history,
};
use serde::{Deserialize, Serialize};

/// Exact append-only result selected as a source, without copying its producing input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowRecordReference {
  node_attempt_id: NodeAttemptId,
  digest: FactoryDigest,
}
impl FlowRecordReference {
  pub(super) fn from_record(record: &FlowNodeRecord) -> Result<Self, FactoryError> {
    Ok(Self {
      node_attempt_id: record.node_attempt_id(),
      digest: record.digest()?,
    })
  }
  /// Returns the globally unique producing Node Attempt.
  #[must_use]
  pub const fn node_attempt_id(&self) -> NodeAttemptId {
    self.node_attempt_id
  }
  /// Returns the complete immutable result identity, including its input and provenance.
  #[must_use]
  pub const fn digest(&self) -> FactoryDigest {
    self.digest
  }
}

/// Trusted owner inputs used to prepare one configured node's frozen input.
pub struct FlowInputPreparation<'a> {
  /// Immutable admitted Work, read from authoritative persistence.
  pub work: &'a WorkEnvelope,
  /// Exact selected definition closure and hard limits.
  pub admitted: &'a AdmittedFlow,
  /// Persisted owning Flow Run.
  pub flow: &'a FlowRun,
  /// Persisted owning Workflow Cycle.
  pub cycle: &'a WorkflowCycle,
  /// Operator-defined node key in the exact definition.
  pub node: FactoryKey,
  /// Exact configured target schema.
  pub schema: &'a FlowDataSchema,
  /// Authoritative append-only execution rows from the owning store snapshot.
  pub history: FlowRuntimeHistory<'a>,
  /// Retained result records from the same snapshot, verified against completions.
  pub records: &'a [FlowNodeRecord],
  /// Frozen node inputs, including the exact input of a nested Flow's calling attempt.
  pub inputs: &'a [FlowNodeInput],
  /// Optional immutable incoming data retained by the Flow owner.
  pub incoming: Option<&'a crate::FlowIncomingData>,
}

/// Common immutable input for an arbitrary configured logical node.
///
/// Only explicitly projected payload fields are exposed. Work identity and exact
/// subject are retained without forwarding all Work Artifacts or parent transcripts.
/// The owner prepares this value before creating an attempt using its digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowNodeInput {
  schema_version: u16,
  work_id: WorkEnvelopeId,
  work_digest: FactoryDigest,
  subject: ExactSubject,
  configuration: FactoryConfigurationRef,
  root_definition: FlowDefinitionRef,
  admission_limits: FlowAdmissionLimits,
  factory_run_id: FactoryRunId,
  definition: FlowDefinitionRef,
  flow_run_id: FlowRunId,
  cycle_id: WorkflowCycleId,
  node: FactoryKey,
  generation: u32,
  payload: FlowPayload,
  #[serde(skip_serializing_if = "Option::is_none")]
  context: Option<ContextManifest>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  retrieval: Vec<RetrievalReceipt>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  sources: Vec<FlowRecordReference>,
  #[serde(skip_serializing_if = "Option::is_none")]
  incoming_digest: Option<FactoryDigest>,
  #[serde(skip_serializing_if = "Option::is_none")]
  caller_input_digest: Option<FactoryDigest>,
}
impl FlowNodeInput {
  /// Recomputes operator-owned projection at publication, retaining only the selected context.
  /// Callers provide the current authoritative source history; historical reads
  /// use the source attempts frozen in this input instead of later replacements.
  pub fn verify_preparation(&self, context: FlowInputPreparation<'_>) -> Result<(), FactoryError> {
    let binding = context
      .admitted
      .closure()
      .definition(context.flow.definition())
      .and_then(|definition| definition.node(&context.node))
      .and_then(crate::FlowNodeDefinition::input_binding)
      .ok_or_else(invalid)?;
    let super::input_binding::ProjectedInput {
      payload,
      sources,
      incoming_digest,
      caller_input_digest,
    } = binding.project(&context)?;
    let mut expected = Self::new(
      context.work,
      context.admitted,
      context.flow,
      context.cycle,
      context.node,
      payload,
    )?
    .with_sources(sources)?;
    expected.incoming_digest = incoming_digest;
    expected.caller_input_digest = caller_input_digest;
    // Execution generation is checked against full attempt history by the store
    // and journal; historical data projection uses only its frozen source snapshot.
    expected.generation = self.generation;
    if let Some(manifest) = &self.context {
      expected = expected.with_context(manifest.clone(), self.retrieval.clone())?;
    }
    if expected != *self {
      return Err(invalid());
    }
    Ok(())
  }
  /// Prepares projected data using the containing node's exact frozen input binding.
  pub fn prepare(context: FlowInputPreparation<'_>) -> Result<Self, FactoryError> {
    validate_flow_runtime_history(context.admitted, context.history)?;
    if !context.history.flow_runs.contains(context.flow) || !context.history.cycles.contains(context.cycle) {
      return Err(invalid());
    }
    let binding = context
      .admitted
      .closure()
      .definition(context.flow.definition())
      .and_then(|definition| definition.node(&context.node))
      .and_then(crate::FlowNodeDefinition::input_binding)
      .ok_or_else(invalid)?;
    let super::input_binding::ProjectedInput {
      payload,
      sources,
      incoming_digest,
      caller_input_digest,
    } = binding.project(&context)?;
    let mut input = Self::new(
      context.work,
      context.admitted,
      context.flow,
      context.cycle,
      context.node,
      payload,
    )?
    .with_sources(sources)?;
    input.incoming_digest = incoming_digest;
    input.caller_input_digest = caller_input_digest;
    input.generation = input.preparation_generation(context.history)?;
    if input.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(input)
  }
  /// Freezes projected schema-validated data under exact retained execution identities.
  pub fn new(
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    flow: &FlowRun,
    cycle: &WorkflowCycle,
    node: FactoryKey,
    payload: FlowPayload,
  ) -> Result<Self, FactoryError> {
    let declared = admitted
      .closure()
      .definition(flow.definition())
      .and_then(|d| d.node(&node))
      .ok_or_else(invalid)?;
    if flow.factory_run_id() != admitted.root_run().factory_run_id()
      || cycle.flow_run_id() != flow.id()
      || declared.input_schema() != Some(payload.schema())
    {
      return Err(invalid());
    }
    let input = Self {
      schema_version: 1,
      work_id: work.id(),
      work_digest: work_digest(work)?,
      subject: work.subject().clone(),
      configuration: work.configuration().clone(),
      root_definition: admitted.closure().root(),
      admission_limits: admitted.limits().clone(),
      factory_run_id: admitted.root_run().factory_run_id(),
      definition: flow.definition(),
      flow_run_id: flow.id(),
      cycle_id: cycle.id(),
      node,
      generation: 0,
      payload,
      context: None,
      retrieval: vec![],
      sources: vec![],
      incoming_digest: None,
      caller_input_digest: None,
    };
    if input.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(input)
  }
  /// Freezes only the selected context and receipts under the exact Work base.
  ///
  /// The Flow owner selects permitted context before persisting the input digest;
  /// the resulting manifest is not a provider-selected authority grant.
  pub fn with_context(
    mut self,
    context: ContextManifest,
    mut retrieval: Vec<RetrievalReceipt>,
  ) -> Result<Self, FactoryError> {
    if context.subject().exact() != &self.subject
      || retrieval.iter().any(|receipt| receipt.subject() != context.subject())
    {
      return Err(invalid());
    }
    retrieval.sort_by_key(RetrievalReceipt::id);
    if retrieval.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
      return Err(invalid());
    }
    for entry in context.entries() {
      if let FactoryContextReference::RepositoryFragment(reference) = entry.source()
        && !retrieval
          .iter()
          .any(|receipt| receipt.contains_reference(reference).unwrap_or(false))
      {
        return Err(invalid());
      }
    }
    if retrieval.iter().any(|receipt| !context.entries().iter().any(|entry| {
      matches!(entry.source(), FactoryContextReference::RepositoryFragment(reference) if reference.retrieval_receipt_id() == receipt.id())
    })) { return Err(invalid()); }
    self.context = Some(context);
    self.retrieval = retrieval;
    if self.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(self)
  }
  /// Returns the exact explicitly selected execution context, if the node uses one.
  #[must_use]
  pub const fn context(&self) -> Option<&ContextManifest> {
    self.context.as_ref()
  }
  fn with_sources(mut self, mut sources: Vec<FlowRecordReference>) -> Result<Self, FactoryError> {
    sources.sort_by_key(FlowRecordReference::node_attempt_id);
    if sources
      .windows(2)
      .any(|pair| pair[0].node_attempt_id == pair[1].node_attempt_id)
    {
      return Err(invalid());
    }
    self.sources = sources;
    if self.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(self)
  }
  /// Returns exact source result references; producing inputs are not forwarded.
  #[must_use]
  pub fn sources(&self) -> &[FlowRecordReference] {
    &self.sources
  }
  /// Returns the exact incoming-data identity selected during preparation.
  #[must_use]
  pub const fn incoming_digest(&self) -> Option<FactoryDigest> {
    self.incoming_digest
  }
  /// Exact parent input selected by a configured caller projection.
  #[must_use]
  pub const fn caller_input_digest(&self) -> Option<FactoryDigest> {
    self.caller_input_digest
  }
  pub(super) fn verify_work(&self, work: &WorkEnvelope, admitted: &AdmittedFlow) -> Result<(), FactoryError> {
    self.verify_admission(admitted)?;
    if self.work_id != work.id()
      || self.work_digest != work_digest(work)?
      || &self.subject != work.subject()
      || &self.configuration != work.configuration()
    {
      return Err(invalid());
    }
    Ok(())
  }
  pub(super) fn verify_admission(&self, admitted: &AdmittedFlow) -> Result<(), FactoryError> {
    if self.root_definition != admitted.closure().root()
      || &self.admission_limits != admitted.limits()
      || self.factory_run_id != admitted.root_run().factory_run_id()
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Restores bytes against the owner-retained digest and exact Work, admission and schema.
  ///
  /// The expected digest comes from authoritative persistence, never from provider bytes.
  pub fn restore(
    bytes: &[u8],
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    flow: &FlowRun,
    cycle: &WorkflowCycle,
    schema: &FlowDataSchema,
    expected_digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let wire: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let payload_bytes = serde_json::to_vec(wire.get("payload").ok_or_else(invalid)?).map_err(|_| invalid())?;
    let node = serde_json::from_value(wire.get("node").ok_or_else(invalid)?.clone()).map_err(|_| invalid())?;
    let mut input = Self::new(
      work,
      admitted,
      flow,
      cycle,
      node,
      FlowPayload::restore(&payload_bytes, schema)?,
    )?;
    if let Some(context) = wire.get("context") {
      let context = serde_json::from_value(context.clone()).map_err(|_| invalid())?;
      let retrieval = wire
        .get("retrieval")
        .map(|value| serde_json::from_value(value.clone()).map_err(|_| invalid()))
        .transpose()?
        .unwrap_or_default();
      input = input.with_context(context, retrieval)?;
    }
    if let Some(sources) = wire.get("sources") {
      input = input.with_sources(serde_json::from_value(sources.clone()).map_err(|_| invalid())?)?;
    }
    if let Some(digest) = wire.get("incoming_digest") {
      input.incoming_digest = Some(serde_json::from_value(digest.clone()).map_err(|_| invalid())?);
    }
    if let Some(digest) = wire.get("caller_input_digest") {
      input.caller_input_digest = Some(serde_json::from_value(digest.clone()).map_err(|_| invalid())?);
    }
    input.generation =
      serde_json::from_value(wire.get("generation").ok_or_else(invalid)?.clone()).map_err(|_| invalid())?;
    if wire != serde_json::to_value(&input).map_err(|_| invalid())? || input.digest()? != expected_digest {
      return Err(invalid());
    }
    Ok(input)
  }
  /// Returns the stable identity consumed by the persisted Node Attempt.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    Ok(FactoryDigest::sha256(
      "octacity.factory.flow-node-input.v1",
      &[&self.bytes()?],
    ))
  }
  fn bytes(&self) -> Result<Vec<u8>, FactoryError> {
    serde_json::to_vec(self).map_err(|_| invalid())
  }
  /// Returns the owning immutable Work identity.
  #[must_use]
  pub const fn work_id(&self) -> WorkEnvelopeId {
    self.work_id
  }
  /// Returns the owning admitted Factory Run.
  #[must_use]
  pub const fn factory_run_id(&self) -> FactoryRunId {
    self.factory_run_id
  }
  /// Returns the exact Project, Repository and base revision.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }
  /// Returns the admitted Factory Configuration version and content identity.
  #[must_use]
  pub const fn configuration(&self) -> &FactoryConfigurationRef {
    &self.configuration
  }
  /// Returns only declared projected data, without acceptance authority.
  #[must_use]
  pub const fn payload(&self) -> &FlowPayload {
    &self.payload
  }
  /// Returns the owning exact Flow Definition.
  #[must_use]
  pub const fn definition(&self) -> FlowDefinitionRef {
    self.definition
  }
  /// Returns the owning Flow Run.
  #[must_use]
  pub const fn flow_run_id(&self) -> FlowRunId {
    self.flow_run_id
  }
  /// Returns the exact Workflow Cycle.
  #[must_use]
  pub const fn cycle_id(&self) -> WorkflowCycleId {
    self.cycle_id
  }
  /// Returns the operator-defined logical node key.
  #[must_use]
  pub const fn node(&self) -> &FactoryKey {
    &self.node
  }
  /// Zero-based execution of this logical node within its owning workflow cycle.
  /// Repeats receive a new frozen identity even when their selected payload is identical.
  #[must_use]
  pub const fn generation(&self) -> u32 {
    self.generation
  }
  fn preparation_generation(&self, history: FlowRuntimeHistory<'_>) -> Result<u32, FactoryError> {
    u32::try_from(
      history
        .attempts
        .iter()
        .filter(|row| {
          row.flow_run_id() == self.flow_run_id
            && row.workflow_cycle_id() == self.cycle_id
            && row.node_key() == &self.node
        })
        .count(),
    )
    .map_err(|_| invalid())
  }
}
pub(super) fn work_digest(work: &WorkEnvelope) -> Result<FactoryDigest, FactoryError> {
  Ok(FactoryDigest::sha256(
    "octacity.factory.work-envelope.v1",
    &[&serde_json::to_vec(work).map_err(|_| invalid())?],
  ))
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow node input",
  }
}
