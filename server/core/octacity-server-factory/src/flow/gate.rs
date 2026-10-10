use crate::{FactoryError, ImmutableReference};
use serde::{Deserialize, Serialize};

/// Maximum encoded operator parameters for one deterministic policy binding.
pub const MAX_FLOW_GATE_PARAMETER_BYTES: usize = 8 * 1024;

/// Exact deterministic policy and bounded parameters of an ordinary gate node.
///
/// Policy adapters own parameter schemas and finite result semantics. The generic
/// interpreter knows only node identity, declared outcomes and transitions. A
/// binding grants no side-effect permission, readiness or execution authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "GateBindingWire")]
pub struct FlowGateBinding {
  policy: ImmutableReference,
  parameters: serde_json::Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GateBindingWire {
  policy: ImmutableReference,
  parameters: serde_json::Value,
}
impl TryFrom<GateBindingWire> for FlowGateBinding {
  type Error = FactoryError;
  fn try_from(wire: GateBindingWire) -> Result<Self, Self::Error> {
    Self::new(wire.policy, wire.parameters)
  }
}
impl FlowGateBinding {
  /// Freezes one installed policy's exact identity and operator-owned parameters.
  pub fn new(policy: ImmutableReference, parameters: serde_json::Value) -> Result<Self, FactoryError> {
    if !parameters.is_object()
      || serde_json::to_vec(&parameters).map_or(true, |bytes| bytes.len() > MAX_FLOW_GATE_PARAMETER_BYTES)
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "Flow gate parameters",
      });
    }
    Ok(Self { policy, parameters })
  }
  /// Returns the policy's exact version and content identity, without mutable aliases.
  #[must_use]
  pub const fn policy(&self) -> &ImmutableReference {
    &self.policy
  }
  /// Returns frozen parameters for validation by the selected policy adapter.
  #[must_use]
  pub const fn parameters(&self) -> &serde_json::Value {
    &self.parameters
  }
}
