use crate::{
  AdmittedFlow, FactoryDigest, FactoryError, FactoryRunId, FlowDataSchema, FlowDefinitionRef, FlowNodeRecord,
  FlowPayload, FlowRecordReference, FlowRunId, MAX_FLOW_DATA_BYTES, WorkEnvelope,
};
use serde::{Deserialize, Serialize};

/// Producer-independent selection identity. The containing Incoming digest also
/// binds this selection to the Work's exact resolved code and destination Run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIncomingIssueSnapshot {
  /// Canonical private selection retained by the managed input owner.
  pub digest: FactoryDigest,
  /// Canonical Project.
  pub project_id: octacity_server_domain::ProjectId,
  /// Canonical Issue; this is not a producing Run identity.
  pub issue_id: octacity_server_domain::IssueId,
  /// Reviewed Issue revision.
  pub version: octacity_server_domain::IssueVersion,
  /// Assessed requirements applicability.
  pub applicability_version: octacity_server_domain::IssueVersion,
  /// Exact Project catalog selected by the input contract.
  pub definition_version: u64,
}

/// Immutable producer provenance retained separately from projected result data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIncomingPredecessor {
  run_id: FactoryRunId,
  flow_run_id: FlowRunId,
  definition: FlowDefinitionRef,
  record: FlowRecordReference,
}
impl FlowIncomingPredecessor {
  /// Returns the producing Factory Run.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }
  /// Returns the exact root or nested producing Flow execution.
  #[must_use]
  pub const fn flow_run_id(&self) -> FlowRunId {
    self.flow_run_id
  }
  /// Returns the producing definition and version.
  #[must_use]
  pub const fn definition(&self) -> FlowDefinitionRef {
    self.definition
  }
  /// Returns the exact producing attempt and retained record digest.
  #[must_use]
  pub const fn record(&self) -> &FlowRecordReference {
    &self.record
  }
}

/// Immutable externally supplied data selected by configured input mappings.
///
/// Its schema is operator-owned. Payload fields remain observations and cannot
/// select nodes, permissions or transitions. Persistence owns the expected digest
/// used to restore it, independently of the bytes supplied by a provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowIncomingData {
  work_digest: FactoryDigest,
  factory_run_id: FactoryRunId,
  root_definition: FlowDefinitionRef,
  payload: FlowPayload,
  #[serde(skip_serializing_if = "Option::is_none")]
  predecessor: Option<FlowIncomingPredecessor>,
  #[serde(skip_serializing_if = "Option::is_none")]
  issue_snapshot: Option<FlowIncomingIssueSnapshot>,
}
impl FlowIncomingData {
  /// Freezes bounded incoming data under the exact Work and admitted definition.
  pub fn new(work: &WorkEnvelope, admitted: &AdmittedFlow, payload: FlowPayload) -> Result<Self, FactoryError> {
    let data = Self {
      work_digest: super::input::work_digest(work)?,
      factory_run_id: admitted.root_run().factory_run_id(),
      root_definition: admitted.closure().root(),
      payload,
      predecessor: None,
      issue_snapshot: None,
    };
    data.validate_size()?;
    Ok(data)
  }
  /// Freezes canonical Issue data after the Issue owner resolves its exact selection.
  pub fn from_issue_snapshot(
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    payload: FlowPayload,
    snapshot: FlowIncomingIssueSnapshot,
  ) -> Result<Self, FactoryError> {
    let mut data = Self::new(work, admitted, payload)?;
    data.issue_snapshot = Some(snapshot);
    data.verify(work, admitted)?;
    data.validate_size()?;
    Ok(data)
  }
  /// Freezes a result after the admission owner verifies producer acceptance and visibility.
  pub fn from_predecessor(
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    record: &FlowNodeRecord,
  ) -> Result<Self, FactoryError> {
    let input = record.input();
    if input.subject() != work.subject() || input.work_id() == work.id() {
      return Err(invalid());
    }
    let mut data = Self::new(work, admitted, record.payload().clone())?;
    data.predecessor = Some(FlowIncomingPredecessor {
      run_id: input.factory_run_id(),
      flow_run_id: input.flow_run_id(),
      definition: input.definition(),
      record: FlowRecordReference::from_record(record)?,
    });
    data.verify(work, admitted)?;
    data.validate_size()?;
    Ok(data)
  }
  fn validate_size(&self) -> Result<(), FactoryError> {
    if self.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(())
  }
  /// Checks the immutable Work and destination Run identities before using the payload.
  pub fn verify(&self, work: &WorkEnvelope, admitted: &AdmittedFlow) -> Result<(), FactoryError> {
    if self.work_digest != super::input::work_digest(work)?
      || self.factory_run_id != admitted.root_run().factory_run_id()
      || self.root_definition != admitted.closure().root()
      || self
        .issue_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.project_id != work.subject().project_id() || self.predecessor.is_some())
      || self
        .predecessor
        .as_ref()
        .is_some_and(|source| source.run_id == self.factory_run_id)
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Restores only exact retained bytes under an independently persisted identity.
  pub fn restore(
    bytes: &[u8],
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    schema: &FlowDataSchema,
    expected: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let wire: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let payload = FlowPayload::restore(
      &serde_json::to_vec(wire.get("payload").ok_or_else(invalid)?).map_err(|_| invalid())?,
      schema,
    )?;
    let mut data = Self::new(work, admitted, payload)?;
    data.predecessor = wire
      .get("predecessor")
      .map(|source| serde_json::from_value(source.clone()))
      .transpose()
      .map_err(|_| invalid())?;
    data.issue_snapshot = wire
      .get("issue_snapshot")
      .map(|snapshot| serde_json::from_value(snapshot.clone()))
      .transpose()
      .map_err(|_| invalid())?;
    data.verify(work, admitted)?;
    if wire != serde_json::to_value(&data).map_err(|_| invalid())? || data.digest()? != expected {
      return Err(invalid());
    }
    Ok(data)
  }
  /// Returns the schema-validated incoming observations.
  #[must_use]
  pub const fn payload(&self) -> &FlowPayload {
    &self.payload
  }
  /// Returns retained producer provenance when this input comes from another Run.
  #[must_use]
  pub const fn predecessor(&self) -> Option<&FlowIncomingPredecessor> {
    self.predecessor.as_ref()
  }
  /// Returns the canonical selection identity without exposing unprojected private values.
  #[must_use]
  pub const fn issue_snapshot(&self) -> Option<&FlowIncomingIssueSnapshot> {
    self.issue_snapshot.as_ref()
  }
  /// Returns the identity retained in each prepared node input using this source.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    Ok(FactoryDigest::sha256(
      "octacity.factory.flow-incoming-data.v1",
      &[&self.bytes()?],
    ))
  }
  fn bytes(&self) -> Result<Vec<u8>, FactoryError> {
    serde_json::to_vec(self).map_err(|_| invalid())
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow incoming data",
  }
}
