use crate::{
  BudgetUsage, FactoryClaimOwnership, FactoryDigest, FactoryError, FactoryKey, FlowDataSchema, FlowDefinition,
  FlowNodeInput, FlowPayload, ImmutableReference, MAX_FLOW_DATA_BYTES, NodeAttempt, NodeAttemptCompletion,
  NodeAttemptCompletionInput, NodeAttemptId, NodeExecutionIdentity,
};
use octacity_server_domain::Timestamp;
use serde::Serialize;

/// Verified observation fields supplied by the selected trusted execution adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowRecordObservation {
  /// One finite outcome mapped by configured deterministic policy.
  pub outcome: FactoryKey,
  /// Schema-validated projected result bytes.
  pub payload: FlowPayload,
  /// Exact execution identity already selected by the persisted attempt.
  pub producer: NodeExecutionIdentity,
  /// Trusted observation time, bounded by the attempt's original deadline.
  pub observed_at: Timestamp,
}

/// Common immutable result record, without phase-specific data types or readiness authority.
///
/// Providers cannot deserialize a record. The owner constructs it from verified
/// outputs and commits its fenced completion and retained bytes atomically.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowNodeRecord {
  schema_version: u16,
  input: FlowNodeInput,
  node_attempt_id: NodeAttemptId,
  outcome: FactoryKey,
  payload: FlowPayload,
  producer: NodeExecutionIdentity,
  observed_at: Timestamp,
  #[serde(skip_serializing_if = "Option::is_none")]
  build: Option<crate::FlowBuildProvenance>,
  #[serde(skip_serializing_if = "Option::is_none")]
  subflow: Option<crate::FlowRecordReference>,
}
impl FlowNodeRecord {
  /// Bounded owner metadata for explicit projections, without producing input,
  /// task context or provider-authored payload fields. Verified report facts
  /// remain under their exact Build output provenance.
  pub fn metadata(&self) -> Result<serde_json::Value, FactoryError> {
    #[derive(Serialize)]
    struct BuildMetadata<'a> {
      profile: &'a crate::FlowBuildProfile,
      outputs: std::collections::BTreeMap<&'a FactoryKey, &'a crate::FlowVerifiedBuildOutput>,
    }
    #[derive(Serialize)]
    struct Metadata<'a> {
      input_digest: FactoryDigest,
      observed_at: Timestamp,
      outcome: &'a FactoryKey,
      schema: &'a ImmutableReference,
      build: Option<BuildMetadata<'a>>,
    }
    let build = self.build.as_ref().map(|build| BuildMetadata {
      profile: build.profile(),
      outputs: build.outputs().iter().map(|output| (&output.name, output)).collect(),
    });
    serde_json::to_value(Metadata {
      input_digest: self.input.digest()?,
      observed_at: self.observed_at,
      outcome: &self.outcome,
      schema: self.payload.schema(),
      build,
    })
    .map_err(|_| invalid())
  }
  /// Binds one retained result to its exact frozen input and persisted node attempt.
  pub fn new(
    input: &FlowNodeInput,
    attempt: &NodeAttempt,
    definition: &FlowDefinition,
    observation: FlowRecordObservation,
  ) -> Result<Self, FactoryError> {
    let record = Self {
      schema_version: 1,
      input: input.clone(),
      node_attempt_id: attempt.id(),
      outcome: observation.outcome,
      payload: observation.payload,
      producer: observation.producer,
      observed_at: observation.observed_at,
      build: None,
      subflow: None,
    };
    record.validate_binding(attempt, definition)?;
    if record.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(record)
  }
  pub(crate) fn for_subflow(
    input: &FlowNodeInput,
    attempt: &NodeAttempt,
    definition: &FlowDefinition,
    child: &crate::FlowRun,
    terminal: crate::FactoryKey,
    result: &Self,
  ) -> Result<Self, FactoryError> {
    if child.parent() != Some(crate::FlowRunParent::new(attempt.flow_run_id(), attempt.id()))
      || result.input().flow_run_id() != child.id()
      || definition
        .node(attempt.node_key())
        .and_then(crate::FlowNodeDefinition::subflow_definition)
        != Some(child.definition())
    {
      return Err(invalid());
    }
    let mut record = Self::new(
      input,
      attempt,
      definition,
      FlowRecordObservation {
        outcome: terminal,
        payload: result.payload().clone(),
        producer: attempt.execution().clone(),
        observed_at: result.observed_at(),
      },
    )?;
    record.subflow = Some(crate::FlowRecordReference::from_record(result)?);
    if record.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(record)
  }
  /// Exact retained child result returned by a subflow call, without an implicit context grant.
  #[must_use]
  pub const fn subflow_source(&self) -> Option<&crate::FlowRecordReference> {
    self.subflow.as_ref()
  }
  /// Binds independently verified ordinary Build outputs before fenced completion.
  pub fn with_build_provenance(
    mut self,
    provenance: crate::FlowBuildProvenance,
    definition: &FlowDefinition,
  ) -> Result<Self, FactoryError> {
    provenance.validate(&self.input, &self.payload, definition)?;
    self.build = Some(provenance);
    if self.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(self)
  }
  /// Returns the trusted retained Build provenance, if this node executed a Build.
  #[must_use]
  pub const fn build_provenance(&self) -> Option<&crate::FlowBuildProvenance> {
    self.build.as_ref()
  }
  fn validate_binding(&self, attempt: &NodeAttempt, definition: &FlowDefinition) -> Result<(), FactoryError> {
    let declared = definition.node(self.input.node()).ok_or_else(invalid)?;
    if definition.reference() != self.input.definition()
      || attempt.factory_run_id() != self.input.factory_run_id()
      || attempt.flow_run_id() != self.input.flow_run_id()
      || attempt.workflow_cycle_id() != self.input.cycle_id()
      || attempt.node_key() != self.input.node()
      || attempt.node_kind() != declared.kind()
      || attempt.input_digest() != self.input.digest()?
      || &self.producer != attempt.execution()
      || attempt.stage_projection_id().is_some()
      || declared
        .build()
        .is_some_and(|binding| binding.result_outcome != self.outcome)
      || declared
        .outcome(&self.outcome)
        .is_none_or(|outcome| outcome.schema() != self.payload.schema())
      || self.observed_at < attempt.claim().claimed_at()
      || self.observed_at > attempt.deadline()
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Produces an ordinary fenced completion; the owner still verifies current stored ownership and readiness.
  pub fn completion(
    &self,
    attempt: &NodeAttempt,
    definition: &FlowDefinition,
    ownership: FactoryClaimOwnership,
    usage: BudgetUsage,
    at: Timestamp,
  ) -> Result<NodeAttemptCompletion, FactoryError> {
    if attempt.id() != self.node_attempt_id || at < self.observed_at {
      return Err(invalid());
    }
    attempt.verify_observer(&ownership, at)?;
    self.validate_binding(attempt, definition)?;
    if matches!(
      attempt.node_kind(),
      crate::FlowNodeKind::BuildCommand | crate::FlowNodeKind::Reasoning
    ) && self.build.is_none()
    {
      return Err(invalid());
    }
    if let Some(provenance) = &self.build {
      provenance.validate(&self.input, &self.payload, definition)?;
    }
    NodeAttemptCompletion::new(
      attempt,
      definition,
      NodeAttemptCompletionInput {
        outcome: self.outcome.clone(),
        output_schema: self.payload.schema().clone(),
        output_digest: self.digest()?,
        ownership,
        usage,
        observed_at: at,
      },
    )
  }
  /// Restores retained bytes against exact trusted inputs and the committed completion's digest.
  pub fn restore(
    bytes: &[u8],
    input: &FlowNodeInput,
    attempt: &NodeAttempt,
    definition: &FlowDefinition,
    schema: &FlowDataSchema,
    completion: &NodeAttemptCompletion,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let wire: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let field = |name| wire.get(name).cloned().ok_or_else(invalid);
    let mut record = Self::new(
      input,
      attempt,
      definition,
      FlowRecordObservation {
        outcome: serde_json::from_value(field("outcome")?).map_err(|_| invalid())?,
        payload: FlowPayload::restore(&serde_json::to_vec(&field("payload")?).map_err(|_| invalid())?, schema)?,
        producer: serde_json::from_value(field("producer")?).map_err(|_| invalid())?,
        observed_at: serde_json::from_value(field("observed_at")?).map_err(|_| invalid())?,
      },
    )?;
    if let Some(provenance) = wire.get("build") {
      record = record.with_build_provenance(
        serde_json::from_value(provenance.clone()).map_err(|_| invalid())?,
        definition,
      )?;
    }
    if let Some(source) = wire.get("subflow") {
      record.subflow = Some(serde_json::from_value(source.clone()).map_err(|_| invalid())?);
    }
    let expected = record.completion(
      attempt,
      definition,
      FactoryClaimOwnership::new(completion.owner().clone(), completion.claim()),
      completion.usage(),
      completion.observed_at(),
    )?;
    if wire != serde_json::to_value(&record).map_err(|_| invalid())? || &expected != completion {
      return Err(invalid());
    }
    Ok(record)
  }
  /// Returns the retained result identity consumed by completion and successor inputs.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    Ok(FactoryDigest::sha256(
      "octacity.factory.flow-node-record.v1",
      &[&self.bytes()?],
    ))
  }
  fn bytes(&self) -> Result<Vec<u8>, FactoryError> {
    serde_json::to_vec(self).map_err(|_| invalid())
  }
  /// Returns the exact frozen producing input.
  #[must_use]
  pub const fn input(&self) -> &FlowNodeInput {
    &self.input
  }
  /// Returns the persisted producing attempt.
  #[must_use]
  pub const fn node_attempt_id(&self) -> NodeAttemptId {
    self.node_attempt_id
  }
  /// Returns the trusted observation time bound into the complete record digest.
  #[must_use]
  pub const fn observed_at(&self) -> Timestamp {
    self.observed_at
  }
  /// Returns projected result data with its exact schema.
  #[must_use]
  pub const fn payload(&self) -> &FlowPayload {
    &self.payload
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow node record",
  }
}
