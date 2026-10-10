use crate::{
  AdmittedFlow, FactoryDigest, FactoryError, FactoryRunId, FlowDataSchema, FlowDefinitionRef, FlowPayload,
  MAX_FLOW_DATA_BYTES, WorkEnvelope,
};
use serde::Serialize;

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
}
impl FlowIncomingData {
  /// Freezes bounded incoming data under the exact Work and admitted definition.
  pub fn new(work: &WorkEnvelope, admitted: &AdmittedFlow, payload: FlowPayload) -> Result<Self, FactoryError> {
    let data = Self {
      work_digest: super::input::work_digest(work)?,
      factory_run_id: admitted.root_run().factory_run_id(),
      root_definition: admitted.closure().root(),
      payload,
    };
    if data.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(data)
  }
  pub(super) fn verify(&self, work: &WorkEnvelope, admitted: &AdmittedFlow) -> Result<(), FactoryError> {
    if self.work_digest != super::input::work_digest(work)?
      || self.factory_run_id != admitted.root_run().factory_run_id()
      || self.root_definition != admitted.closure().root()
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
    let data = Self::new(work, admitted, payload)?;
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
