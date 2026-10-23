//! Pre-dispatch selection facts share the exact frozen input and ordinary Flow journal.
use crate::{AdmittedFlow, FactoryDigest, FactoryError, FactoryKey, FlowNodeInput, FlowProfileSelectionPolicy};
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

/// Immutable selection for one frozen node occurrence, before an execution attempt exists.
/// Its input digest binds the Run, nested Flow, cycle, generation, schemas and candidate closure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowProfileSelectionRecord {
  input_digest: FactoryDigest,
  candidate: Option<FactoryKey>,
  started_at: Timestamp,
  deadline: Timestamp,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionWire {
  input_digest: FactoryDigest,
  candidate: Option<FactoryKey>,
  started_at: Timestamp,
  deadline: Timestamp,
}
impl FlowProfileSelectionRecord {
  /// Freezes the manual policy's choice or pending state without executing a capability.
  pub fn manual(input: &FlowNodeInput, admitted: &AdmittedFlow, at: Timestamp) -> Result<Self, FactoryError> {
    input.verify_admission(admitted)?;
    let node = admitted
      .closure()
      .definition(input.definition())
      .and_then(|definition| definition.node(input.node()))
      .ok_or_else(invalid)?;
    let selection = node.profile_selection().ok_or_else(invalid)?;
    let FlowProfileSelectionPolicy::Manual { candidate } = &selection.policy else {
      return Err(invalid());
    };
    let deadline = at
      .unix_millis()
      .checked_add(i64::try_from(node.budget().max_elapsed_millis()).map_err(|_| invalid())?)
      .and_then(|value| Timestamp::from_unix_millis(value).ok())
      .ok_or_else(invalid)?;
    Ok(Self {
      input_digest: input.digest()?,
      candidate: candidate.clone(),
      started_at: at,
      deadline,
    })
  }
  /// Exact frozen input identifying this selection's immutable stage occurrence.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }
  /// Selected candidate, or `None` while explicit manual input is required.
  #[must_use]
  pub const fn candidate(&self) -> Option<&FactoryKey> {
    self.candidate.as_ref()
  }
  /// Original selection start; polling and owner takeover do not renew it.
  #[must_use]
  pub const fn started_at(&self) -> Timestamp {
    self.started_at
  }
  /// Original bounded stage deadline, shared by subsequent execution.
  #[must_use]
  pub const fn deadline(&self) -> Timestamp {
    self.deadline
  }
  /// Canonical retained fact identity, independent of observer ownership.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
    Ok(FactoryDigest::sha256(
      "octacity.factory.profile-selection.v1",
      &[&bytes],
    ))
  }
  /// Restores exact content against its frozen input and admitted selection policy.
  pub fn restore(
    bytes: &[u8],
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    let wire: SelectionWire = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let value = Self::manual(input, admitted, wire.started_at)?;
    if wire.input_digest != value.input_digest
      || wire.candidate != value.candidate
      || wire.deadline != value.deadline
      || value.digest()? != digest
    {
      return Err(invalid());
    }
    Ok(value)
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow profile selection record",
  }
}
