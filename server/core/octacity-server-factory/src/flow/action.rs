use serde::{Deserialize, Serialize};

use crate::{FactoryError, FactoryKey, ImmutableReference};

/// Maximum serialized parameter bytes in one operator-published action binding.
pub const MAX_FLOW_ACTION_PARAMETER_BYTES: usize = 8 * 1024;

/// Immutable plugin extension of the existing `trusted_action` primitive.
///
/// Action names and parameter schemas belong to the exact plugin version.
/// Parameters carry configuration, never credentials or routing authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ActionBindingWire")]
pub struct FlowActionBinding {
  plugin: ImmutableReference,
  action: FactoryKey,
  parameters: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionBindingWire {
  plugin: ImmutableReference,
  action: FactoryKey,
  parameters: serde_json::Value,
}

impl TryFrom<ActionBindingWire> for FlowActionBinding {
  type Error = FactoryError;
  fn try_from(wire: ActionBindingWire) -> Result<Self, Self::Error> {
    Self::new(wire.plugin, wire.action, wire.parameters)
  }
}

impl FlowActionBinding {
  /// Freezes bounded structured parameters for one exact plugin action.
  pub fn new(
    plugin: ImmutableReference,
    action: FactoryKey,
    parameters: serde_json::Value,
  ) -> Result<Self, FactoryError> {
    if !parameters.is_object()
      || serde_json::to_vec(&parameters).map_or(true, |bytes| bytes.len() > MAX_FLOW_ACTION_PARAMETER_BYTES)
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "Flow action parameters",
      });
    }
    Ok(Self {
      plugin,
      action,
      parameters,
    })
  }

  /// Returns the exact plugin version and content identity.
  #[must_use]
  pub const fn plugin(&self) -> &ImmutableReference {
    &self.plugin
  }
  /// Returns the plugin-defined action name.
  #[must_use]
  pub const fn action(&self) -> &FactoryKey {
    &self.action
  }
  /// Returns the operator-published parameters; plugin validation precedes effects.
  #[must_use]
  pub const fn parameters(&self) -> &serde_json::Value {
    &self.parameters
  }
}
