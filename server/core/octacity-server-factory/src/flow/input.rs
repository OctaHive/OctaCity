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

/// Explicit node resource selection; absence retains the historical Work-backed wire form.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowNodeCode {
  /// Work correlation is retained, with no repository attachment or code authority.
  None,
  /// One exact selected resource; its permissions and scope remain in the pinned definition.
  Source {
    /// Explicit declaration key, never inferred from Work correlation.
    source: FactoryKey,
    /// Reviewed access request, still narrowed by independent execution policy.
    access: crate::MountMode,
    /// Exact node visibility retained for Context and downstream signed materialization.
    scope: crate::FlowCodeScope,
    /// Exact base or independently accepted candidate bound to this resource.
    subject: Box<crate::FactoryTaskSubject>,
    /// Independently retained producing result, present only for accepted candidate selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted_result: Option<FlowRecordReference>,
  },
}

/// Common immutable input for an arbitrary configured logical node.
///
/// Only explicitly projected payload fields are exposed. Work correlation and explicit
/// code resources are retained without forwarding all Work Artifacts or parent transcripts.
/// The owner prepares this value before creating an attempt using its digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowNodeInput {
  schema_version: u16,
  work_id: WorkEnvelopeId,
  work_digest: FactoryDigest,
  subject: crate::WorkSubject,
  #[serde(skip_serializing_if = "Option::is_none")]
  code: Option<Box<FlowNodeCode>>,
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
  #[serde(skip_serializing_if = "Option::is_none")]
  profile_override: Option<crate::FlowProfileAdmissionOverride>,
}
impl FlowNodeInput {
  /// Verifies the exact execution owner before a capability consumes frozen data.
  pub fn verify_attempt(&self, attempt: &crate::NodeAttempt) -> Result<(), FactoryError> {
    attempt.validate_execution_window()?;
    if attempt.factory_run_id() != self.factory_run_id
      || attempt.flow_run_id() != self.flow_run_id
      || attempt.workflow_cycle_id() != self.cycle_id
      || attempt.node_key() != &self.node
      || attempt.input_digest() != self.digest()?
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Creates the next execution generation from the same frozen data and Context.
  /// Retry eligibility and predecessor stop are checked by the owner and journal.
  pub fn retry(&self) -> Result<Self, FactoryError> {
    let mut next = self.clone();
    next.generation = self.generation.checked_add(1).ok_or_else(invalid)?;
    Ok(next)
  }
  /// Recomputes operator-owned projection at publication, retaining only the selected context.
  /// Callers provide the current authoritative source history; historical reads
  /// use the source attempts frozen in this input instead of later replacements.
  pub fn verify_preparation(&self, context: FlowInputPreparation<'_>) -> Result<(), FactoryError> {
    self.verify_admission(context.admitted)?;
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
    let candidate = Self::selected_candidate(&context)?;
    let mut expected = Self::new_with_candidate(
      context.work,
      context.admitted,
      context.flow,
      context.cycle,
      context.node,
      payload,
      candidate,
    )?
    .with_sources(sources)?
    .with_admission_profile(context.work, context.admitted, context.history)?;
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
    let candidate = Self::selected_candidate(&context)?;
    let mut input = Self::new_with_candidate(
      context.work,
      context.admitted,
      context.flow,
      context.cycle,
      context.node,
      payload,
      candidate,
    )?
    .with_sources(sources)?
    .with_admission_profile(context.work, context.admitted, context.history)?;
    input.incoming_digest = incoming_digest;
    input.caller_input_digest = caller_input_digest;
    input.generation = input.preparation_generation(context.history)?;
    if input.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(input)
  }
  fn selected_candidate(
    context: &FlowInputPreparation<'_>,
  ) -> Result<Option<(crate::CandidateSubject, FlowRecordReference)>, FactoryError> {
    let definition = context
      .admitted
      .closure()
      .definition(context.flow.definition())
      .ok_or_else(invalid)?;
    let Some(crate::FlowCodeBinding::Source {
      source,
      revision: crate::FlowCodeRevision::AcceptedCandidate { node, outcome },
      ..
    }) = definition.node(&context.node).and_then(crate::FlowNodeDefinition::code)
    else {
      return Ok(None);
    };
    let record = super::input_binding::select_record(context, node, outcome)?;
    let candidate = record
      .discovery()
      .and_then(crate::FlowDiscoveryFacts::published_candidate)
      .ok_or_else(invalid)?;
    let declaration = definition
      .code_sources()
      .and_then(|sources| sources.get(source))
      .ok_or_else(invalid)?;
    if candidate.exact() != record.input().exact_resource()?
      || candidate.exact().project_id() != context.work.subject().project_id()
      || candidate.exact().repository_id() != declaration.repository.id
    {
      return Err(invalid());
    }
    Ok(Some((candidate.clone(), FlowRecordReference::from_record(record)?)))
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
    Self::new_with_candidate(work, admitted, flow, cycle, node, payload, None)
  }
  fn new_with_candidate(
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    flow: &FlowRun,
    cycle: &WorkflowCycle,
    node: FactoryKey,
    payload: FlowPayload,
    candidate: Option<(crate::CandidateSubject, FlowRecordReference)>,
  ) -> Result<Self, FactoryError> {
    let declared = admitted
      .closure()
      .definition(flow.definition())
      .and_then(|d| d.node(&node))
      .ok_or_else(invalid)?;
    work
      .profile_overrides()
      .validate(admitted.closure(), work.subject().project_id())?;
    if flow.factory_run_id() != admitted.root_run().factory_run_id()
      || cycle.flow_run_id() != flow.id()
      || declared.input_schema() != Some(payload.schema())
    {
      return Err(invalid());
    }
    let code = match declared.code() {
      None => {
        work.subject().repository()?;
        None
      }
      Some(crate::FlowCodeBinding::None) => Some(FlowNodeCode::None),
      Some(crate::FlowCodeBinding::Source {
        source,
        revision: crate::FlowCodeRevision::AdmittedBase,
        access,
        scope,
      }) => {
        super::code::validate_pins(
          admitted.closure(),
          admitted.code_pins(),
          Some(work.subject().project_id()),
        )?;
        let pin = admitted
          .code_pins()
          .iter()
          .find(|pin| pin.definition == flow.definition() && &pin.source == source)
          .ok_or_else(invalid)?;
        Some(FlowNodeCode::Source {
          source: source.clone(),
          access: *access,
          scope: scope.clone(),
          accepted_result: None,
          subject: Box::new(crate::FactoryTaskSubject::Exact(pin.subject.clone())),
        })
      }
      Some(crate::FlowCodeBinding::Source {
        source,
        revision: crate::FlowCodeRevision::ExactRevision { revision },
        access,
        scope,
      }) => {
        super::code::validate_pins(
          admitted.closure(),
          admitted.code_pins(),
          Some(work.subject().project_id()),
        )?;
        let declaration = admitted
          .closure()
          .definition(flow.definition())
          .and_then(|definition| definition.code_sources())
          .and_then(|sources| sources.get(source))
          .ok_or_else(invalid)?;
        if declaration.repository.project_id != work.subject().project_id() {
          return Err(invalid());
        }
        Some(FlowNodeCode::Source {
          source: source.clone(),
          access: *access,
          scope: scope.clone(),
          accepted_result: None,
          subject: Box::new(crate::FactoryTaskSubject::Exact(ExactSubject::new(
            declaration.repository.project_id,
            declaration.repository.id,
            revision.clone(),
          ))),
        })
      }
      Some(crate::FlowCodeBinding::Source {
        source,
        revision: crate::FlowCodeRevision::AcceptedCandidate { .. },
        access,
        scope,
      }) => {
        let (candidate, reference) = candidate.ok_or_else(invalid)?;
        super::code::validate_pins(
          admitted.closure(),
          admitted.code_pins(),
          Some(work.subject().project_id()),
        )?;
        let declaration = admitted
          .closure()
          .definition(flow.definition())
          .and_then(|definition| definition.code_sources())
          .and_then(|sources| sources.get(source))
          .ok_or_else(invalid)?;
        if candidate.exact().project_id() != work.subject().project_id()
          || candidate.exact().repository_id() != declaration.repository.id
        {
          return Err(invalid());
        }
        Some(FlowNodeCode::Source {
          source: source.clone(),
          access: *access,
          scope: scope.clone(),
          subject: Box::new(crate::FactoryTaskSubject::Candidate(candidate)),
          accepted_result: Some(reference),
        })
      }
    };
    let profile_override = work
      .profile_overrides()
      .entries()
      .iter()
      .find(|entry| {
        flow.parent().is_none()
          && entry.stage.root == admitted.closure().root()
          && entry.stage.calls.is_empty()
          && entry.stage.node == node
      })
      .cloned();
    let input = Self {
      schema_version: if code.is_some() { 2 } else { 1 },
      work_id: work.id(),
      work_digest: work_digest(work)?,
      subject: work.subject().clone(),
      code: code.map(Box::new),
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
      profile_override,
    };
    let input = input.with_sources(vec![])?;
    if input.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(input)
  }
  /// Freezes selected context and receipts under the selected task subject.
  /// Explicit code bindings retain their full subject; historical inputs retain Work-base compatibility.
  ///
  /// The Flow owner selects permitted context before persisting the input digest;
  /// the resulting manifest is not a provider-selected authority grant.
  pub fn with_context(
    mut self,
    context: ContextManifest,
    mut retrieval: Vec<RetrievalReceipt>,
  ) -> Result<Self, FactoryError> {
    let subject_matches = if self.code.is_some() {
      context.subject() == &self.task_subject()?
    } else {
      context.subject().exact()? == self.exact_resource()?
    };
    if !subject_matches || retrieval.iter().any(|receipt| receipt.subject() != context.subject()) {
      return Err(invalid());
    }
    if let Some(FlowNodeCode::Source { scope, .. }) = self.code.as_deref() {
      for entry in context.entries() {
        let range = match entry.source() {
          FactoryContextReference::RepositoryRange(range) => range,
          FactoryContextReference::RepositoryFragment(fragment) => fragment.range(),
          _ => continue,
        };
        let requested = crate::FlowCodeScope::Selected {
          entries: [crate::FlowCodeSelection::Range {
            path: range.path().clone(),
            lines: crate::FactoryRepositoryLineRange::new(range.first_line(), range.last_line())?,
          }]
          .into(),
        };
        if !requested.is_within(scope) {
          return Err(invalid());
        }
      }
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
    if let Some(FlowNodeCode::Source {
      accepted_result: Some(reference),
      ..
    }) = self.code.as_deref()
      && !sources.contains(reference)
    {
      sources.push(reference.clone());
    }
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
  /// Exact original Work admission choice for this stage occurrence, if supplied.
  #[must_use]
  pub const fn profile_override(&self) -> Option<&crate::FlowProfileAdmissionOverride> {
    self.profile_override.as_ref()
  }
  /// Freezes an admission choice using authoritative parent-call rows, without selecting automatically.
  /// The complete journal checks those rows and this binding before accepting the input.
  pub fn with_admission_profile(
    mut self,
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    history: FlowRuntimeHistory<'_>,
  ) -> Result<Self, FactoryError> {
    // Context and projected source digests may still be under preparation here.
    // Their complete verification remains at the ordinary publication boundary.
    if self.work_digest != work_digest(work)? {
      return Err(invalid());
    }
    self.profile_override = self.expected_admission_profile(work, admitted, history)?.cloned();
    Ok(self)
  }
  fn expected_admission_profile<'a>(
    &self,
    work: &'a WorkEnvelope,
    admitted: &AdmittedFlow,
    history: FlowRuntimeHistory<'_>,
  ) -> Result<Option<&'a crate::FlowProfileAdmissionOverride>, FactoryError> {
    if work.profile_overrides().is_empty() {
      return Ok(None);
    }
    let address =
      super::profile_override::FlowStageAddress::for_occurrence(admitted, history, self.flow_run_id, &self.node)?;
    Ok(
      work
        .profile_overrides()
        .entries()
        .iter()
        .find(|entry| entry.stage == address),
    )
  }
  pub(super) fn verify_admission_profile(
    &self,
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    history: FlowRuntimeHistory<'_>,
  ) -> Result<(), FactoryError> {
    if self.profile_override.as_ref() != self.expected_admission_profile(work, admitted, history)? {
      return Err(invalid());
    }
    Ok(())
  }
  pub(crate) fn verify_work(&self, work: &WorkEnvelope, admitted: &AdmittedFlow) -> Result<(), FactoryError> {
    self.verify_admission(admitted)?;
    if self.work_id != work.id()
      || self.work_digest != work_digest(work)?
      || &self.subject != work.subject()
      || &self.configuration != work.configuration()
      || self
        .profile_override
        .as_ref()
        .is_some_and(|entry| !work.profile_overrides().entries().contains(entry))
    {
      return Err(invalid());
    }
    Ok(())
  }
  pub(crate) fn verify_admission(&self, admitted: &AdmittedFlow) -> Result<(), FactoryError> {
    if let Some(entry) = &self.profile_override {
      let definition = entry.stage.definition(admitted.closure())?;
      if definition != self.definition || entry.stage.node != self.node {
        return Err(invalid());
      }
    }
    if self.root_definition != admitted.closure().root()
      || &self.admission_limits != admitted.limits()
      || self.factory_run_id != admitted.root_run().factory_run_id()
    {
      return Err(invalid());
    }
    if let Some(binding) = admitted
      .closure()
      .definition(self.definition)
      .and_then(|definition| definition.node(&self.node))
      .and_then(crate::FlowNodeDefinition::context_binding)
    {
      binding.verify(self)?;
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
    let candidate = wire
      .get("code")
      .map(|value| serde_json::from_value::<FlowNodeCode>(value.clone()).map_err(|_| invalid()))
      .transpose()?;
    let candidate = match candidate {
      Some(FlowNodeCode::Source {
        subject,
        accepted_result: Some(reference),
        ..
      }) => match *subject {
        crate::FactoryTaskSubject::Candidate(candidate) => Some((candidate, reference)),
        _ => None,
      },
      _ => None,
    };
    let mut input = Self::new_with_candidate(
      work,
      admitted,
      flow,
      cycle,
      node,
      FlowPayload::restore(&payload_bytes, schema)?,
      candidate,
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
    if let Some(entry) = wire.get("profile_override") {
      input.profile_override = Some(serde_json::from_value(entry.clone()).map_err(|_| invalid())?);
    }
    input.generation =
      serde_json::from_value(wire.get("generation").ok_or_else(invalid)?.clone()).map_err(|_| invalid())?;
    if wire != serde_json::to_value(&input).map_err(|_| invalid())? || input.digest()? != expected_digest {
      return Err(invalid());
    }
    input.verify_work(work, admitted)?;
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
  /// Work correlation does not select a repository or grant code access.
  #[must_use]
  pub const fn subject(&self) -> &crate::WorkSubject {
    &self.subject
  }
  /// Frozen explicit code choice; absence denotes the historical Work-backed encoding.
  #[must_use]
  pub fn code(&self) -> Option<&FlowNodeCode> {
    self.code.as_deref()
  }
  /// Retained producing record proving an explicitly selected candidate resource.
  /// This identity grants no payload access; the same record may also be explicitly projected as data.
  #[must_use]
  pub fn accepted_resource_record(&self) -> Option<&FlowRecordReference> {
    match self.code.as_deref() {
      Some(FlowNodeCode::Source { accepted_result, .. }) => accepted_result.as_ref(),
      _ => None,
    }
  }
  /// Exact task Context identity; Work correlation is explicitly non-repository for no-code steps.
  pub fn task_subject(&self) -> Result<crate::FactoryTaskSubject, FactoryError> {
    match self.code.as_deref() {
      None => Ok(crate::FactoryTaskSubject::Exact(self.subject.repository()?.clone())),
      Some(FlowNodeCode::None) => Ok(crate::FactoryTaskSubject::WorkInputV1(self.subject.clone())),
      Some(FlowNodeCode::Source { subject, .. }) => Ok(subject.as_ref().clone()),
    }
  }
  /// Exact attached repository resource; explicitly no-code steps never inherit Work's repository.
  pub fn exact_resource(&self) -> Result<&ExactSubject, FactoryError> {
    match self.code.as_deref() {
      None => self.subject.repository(),
      Some(FlowNodeCode::None) => Err(invalid()),
      Some(FlowNodeCode::Source { subject, .. }) => subject.exact(),
    }
  }
  /// Verifies candidate identity against the frozen resource, without granting code access.
  /// Explicit candidate bindings require the full revision and ChangeSet identity;
  /// historical and explicitly base-bound inputs retain their base-subject semantics.
  #[must_use]
  pub fn matches_candidate_resource(&self, candidate: &crate::CandidateSubject) -> bool {
    if let Some(FlowNodeCode::Source { subject, .. }) = self.code.as_deref()
      && let crate::FactoryTaskSubject::Candidate(selected) = subject.as_ref()
    {
      selected == candidate
    } else {
      self
        .exact_resource()
        .is_ok_and(|resource| resource == candidate.exact())
    }
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
