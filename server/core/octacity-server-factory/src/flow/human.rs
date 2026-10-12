use crate::{FactoryError, FactoryKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Technical operator request purpose. It grants no publication or effect authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowHumanOperation {
  /// A bounded escalation disposition.
  Escalation,
  /// Confirmation before the configured delivery route.
  Delivery,
  /// Confirmation before the configured promotion route.
  Promotion,
}
impl FlowHumanOperation {
  /// Checks the code-owned request vocabulary independently of logical outcomes.
  #[must_use]
  pub const fn permits(self, disposition: FlowHumanDisposition) -> bool {
    match self {
      Self::Escalation => !matches!(disposition, FlowHumanDisposition::Confirm),
      Self::Delivery | Self::Promotion => matches!(disposition, FlowHumanDisposition::Confirm),
    }
  }
}

/// Closed operator choices; the immutable binding selects logical outcome keys.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowHumanDisposition {
  /// Record that an escalation was observed.
  Acknowledge,
  /// Permit the configured continuation.
  Resume,
  /// Select the configured refusal route.
  Abort,
  /// Confirm a delivery or promotion request.
  Confirm,
}

/// Pinned mapping from a closed management purpose to declared HumanGate outcomes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "BindingWire")]
pub struct FlowHumanGateBinding {
  operation: FlowHumanOperation,
  outcomes: BTreeMap<FlowHumanDisposition, FactoryKey>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingWire {
  operation: FlowHumanOperation,
  outcomes: BTreeMap<FlowHumanDisposition, FactoryKey>,
}
impl TryFrom<BindingWire> for FlowHumanGateBinding {
  type Error = FactoryError;
  fn try_from(wire: BindingWire) -> Result<Self, Self::Error> {
    Self::new(wire.operation, wire.outcomes)
  }
}
impl FlowHumanGateBinding {
  /// Freezes one non-empty mapping containing only choices valid for this purpose.
  pub fn new(
    operation: FlowHumanOperation,
    outcomes: BTreeMap<FlowHumanDisposition, FactoryKey>,
  ) -> Result<Self, FactoryError> {
    if outcomes.is_empty() || outcomes.keys().any(|choice| !operation.permits(*choice)) {
      return Err(invalid());
    }
    Ok(Self { operation, outcomes })
  }
  /// Returns the exact closed management purpose.
  #[must_use]
  pub const fn operation(&self) -> FlowHumanOperation {
    self.operation
  }
  /// Selects a declared logical outcome; a missing choice grants no continuation.
  #[must_use]
  pub fn outcome(&self, disposition: FlowHumanDisposition) -> Option<&FactoryKey> {
    self.outcomes.get(&disposition)
  }
  /// Enumerates only the immutable choices explicitly enabled by this node.
  pub fn dispositions(&self) -> impl Iterator<Item = FlowHumanDisposition> + '_ {
    self.outcomes.keys().copied()
  }
  /// Exact shape produced by a bounded management request for one configured choice.
  pub fn response_shape(&self, disposition: FlowHumanDisposition) -> Result<crate::FlowValueSchema, FactoryError> {
    if self.outcome(disposition).is_none() {
      return Err(invalid());
    }
    Ok(crate::FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          FactoryKey::new("operation").expect("technical field"),
          crate::FlowFieldSchema::required(crate::FlowValueSchema::Enum {
            values: vec![serde_json::json!(self.operation)],
          }),
        ),
        (
          FactoryKey::new("disposition").expect("technical field"),
          crate::FlowFieldSchema::required(crate::FlowValueSchema::Enum {
            values: vec![serde_json::json!(disposition)],
          }),
        ),
        (
          FactoryKey::new("reason").expect("technical field"),
          crate::FlowFieldSchema::required(crate::FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: crate::MAX_FACTORY_TEXT_BYTES,
          }),
        ),
      ]),
    })
  }
  /// Renders closed typed observations. Outcome keys and lifecycle data are absent.
  pub fn response(
    &self,
    disposition: FlowHumanDisposition,
    reason: &crate::FactoryText,
  ) -> Result<serde_json::Value, FactoryError> {
    if self.outcome(disposition).is_none() {
      return Err(invalid());
    }
    Ok(serde_json::json!({"operation": self.operation, "disposition": disposition, "reason": reason}))
  }
  pub(super) fn validate_node(&self, node: &crate::FlowNodeDefinition) -> Result<(), FactoryError> {
    if node.kind() != crate::FlowNodeKind::HumanGate || self.outcomes.values().any(|key| node.outcome(key).is_none()) {
      return Err(invalid());
    }
    Ok(())
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "HumanGate operator requests",
  }
}
