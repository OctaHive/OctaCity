//! Canonical Accepted Work projection from immutable configured gate records.
use crate::*;
use serde::Serialize;
use std::collections::BTreeMap;
/// Frozen Work, exact accepting policy and the complete immutable input ancestry.
///
/// The common journal retains all source bytes and the gate completion atomically.
/// This canonical projection adds no parallel authority or phase-specific rows;
/// restart reconstructs identical bytes from those retained records.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AcceptedWorkContract {
  schema_version: u16,
  work: WorkEnvelope,
  configuration: FactoryConfigurationRef,
  definition: FlowDefinitionRef,
  source_record: NodeAttemptId,
  policy_digest: FactoryDigest,
  records: Vec<FlowNodeRecord>,
  incoming: Vec<FlowIncomingData>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  caller_inputs: Vec<FlowNodeInput>,
}
impl AcceptedWorkContract {
  /// Freezes only an outcome explicitly accepted by the pinned declarative gate.
  pub fn freeze(
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    data: &FlowDataHistory,
    runtime: FlowRuntimeHistory<'_>,
    source_record: NodeAttemptId,
  ) -> Result<Self, FactoryError> {
    data.validate(work, admitted, runtime)?;
    Self::freeze_validated(work, admitted, data, runtime, source_record)
  }
  pub(crate) fn freeze_validated(
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    data: &FlowDataHistory,
    runtime: FlowRuntimeHistory<'_>,
    source_record: NodeAttemptId,
  ) -> Result<Self, FactoryError> {
    let source = data
      .records
      .iter()
      .find(|row| row.node_attempt_id() == source_record)
      .ok_or_else(invalid)?;
    let definition = admitted
      .closure()
      .definition(source.input().definition())
      .ok_or_else(invalid)?;
    let node = definition.node(source.input().node()).ok_or_else(invalid)?;
    let completion = runtime
      .completions
      .iter()
      .find(|row| row.node_attempt_id() == source_record)
      .ok_or_else(invalid)?;
    if !node.accepted_work_outcomes().contains(completion.outcome()) {
      return Err(invalid());
    }
    let policy_digest = FactoryDigest::sha256(
      "octacity.factory.accepted-work-policy.v1",
      &[&serde_json::to_vec(node.gate().ok_or_else(invalid)?).map_err(|_| invalid())?],
    );
    let mut pending = vec![source_record];
    let mut records = BTreeMap::new();
    let mut incoming = BTreeMap::new();
    let mut caller_inputs = BTreeMap::new();
    while let Some(id) = pending.pop() {
      if records.contains_key(&id) {
        continue;
      }
      let row = data
        .records
        .iter()
        .find(|row| row.node_attempt_id() == id)
        .ok_or_else(invalid)?;
      pending.extend(row.input().sources().iter().map(FlowRecordReference::node_attempt_id));
      if let Some(source) = row.subflow_source() {
        pending.push(source.node_attempt_id());
      }
      let mut caller = row.input().caller_input_digest();
      while let Some(digest) = caller {
        if caller_inputs.contains_key(&digest) {
          break;
        }
        let input = data
          .inputs
          .iter()
          .find(|input| input.digest().ok() == Some(digest))
          .ok_or_else(invalid)?;
        pending.extend(input.sources().iter().map(FlowRecordReference::node_attempt_id));
        if let Some(incoming_digest) = input.incoming_digest() {
          let envelope = data
            .incoming
            .iter()
            .find(|row| row.digest().ok() == Some(incoming_digest))
            .ok_or_else(invalid)?;
          incoming.insert(incoming_digest, envelope.clone());
        }
        caller = input.caller_input_digest();
        caller_inputs.insert(digest, input.clone());
      }
      if let Some(digest) = row.input().incoming_digest() {
        let envelope = data
          .incoming
          .iter()
          .find(|row| row.digest().ok() == Some(digest))
          .ok_or_else(invalid)?;
        incoming.insert(digest, envelope.clone());
      }
      records.insert(id, row.clone());
    }
    let contract = Self {
      schema_version: 1,
      work: work.clone(),
      configuration: work.configuration().clone(),
      definition: definition.reference(),
      source_record,
      policy_digest,
      records: records.into_values().collect(),
      incoming: incoming.into_values().collect(),
      caller_inputs: caller_inputs.into_values().collect(),
    };
    if serde_json::to_vec(&contract).map_err(|_| invalid())?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(contract)
  }
  /// Restores a persisted projection only against the authoritative Work and journal.
  pub fn restore(
    bytes: &[u8],
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    data: &FlowDataHistory,
    runtime: FlowRuntimeHistory<'_>,
    source_record: NodeAttemptId,
    digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let restored = Self::freeze(work, admitted, data, runtime, source_record)?;
    let wire: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if restored.digest()? != digest || wire != serde_json::to_value(&restored).map_err(|_| invalid())? {
      return Err(invalid());
    }
    Ok(restored)
  }
  /// Complete immutable Work, including acceptance inputs and exact base revision.
  #[must_use]
  pub const fn work(&self) -> &WorkEnvelope {
    &self.work
  }
  /// Accepting common journal record.
  #[must_use]
  pub const fn source_record(&self) -> NodeAttemptId {
    self.source_record
  }
  /// Complete deterministic policy identity, including operator parameters.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }
  /// Exact accepted source records and their complete provenance.
  #[must_use]
  pub fn records(&self) -> &[FlowNodeRecord] {
    &self.records
  }
  /// Content identity of the frozen contract.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    Ok(FactoryDigest::sha256(
      "octacity.factory.accepted-work-contract.v1",
      &[&serde_json::to_vec(self).map_err(|_| invalid())?],
    ))
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Accepted Work contract",
  }
}
