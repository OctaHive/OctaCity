use super::predicate::validate_pointer;
use crate::{
  FactoryError, FactoryKey, FlowDataSchema, FlowPayload, MAX_FLOW_DATA_BYTES, MAX_FLOW_SCHEMA_DEPTH,
  MAX_FLOW_SCHEMA_ENTRIES,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Hard encoded ceiling for one admitted data mapping.
pub const MAX_FLOW_MAPPING_BYTES: usize = 16 * 1024;
const MAX_MAPPING_NODES: usize = 128;

/// Explicit data selection independent of logical stage names.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "mapping", deny_unknown_fields)]
pub enum FlowValueMapping {
  /// Copies one explicitly selected input value.
  Pointer {
    /// RFC 6901 pointer into the projected input.
    path: String,
  },
  /// Uses an operator-owned configuration value.
  Constant {
    /// Frozen value, validated by the target schema.
    value: Value,
  },
  /// Constructs a closed object from explicitly selected fields.
  Object {
    /// Target names and selections; no undeclared input fields propagate.
    fields: BTreeMap<FactoryKey, FlowValueMapping>,
  },
}

/// Frozen mapping reused for node inputs and declared output contracts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "MappingWire")]
pub struct FlowDataMapping {
  value: FlowValueMapping,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MappingWire {
  value: FlowValueMapping,
}
impl TryFrom<MappingWire> for FlowDataMapping {
  type Error = FactoryError;
  fn try_from(wire: MappingWire) -> Result<Self, Self::Error> {
    Self::new(wire.value)
  }
}
impl FlowDataMapping {
  /// Validates explicit selections before the containing configuration is admitted.
  pub fn new(value: FlowValueMapping) -> Result<Self, FactoryError> {
    let mut count = 0;
    value.validate(0, &mut count)?;
    let mapping = Self { value };
    if serde_json::to_vec(&mapping).map_err(|_| invalid())?.len() > MAX_FLOW_MAPPING_BYTES {
      return Err(invalid());
    }
    Ok(mapping)
  }
  /// Projects only configured fields and validates the complete target contract.
  ///
  /// Missing data fails closed. Projection grants no acceptance or execution authority.
  pub fn project(&self, input: &FlowPayload, schema: &FlowDataSchema) -> Result<FlowPayload, FactoryError> {
    self.project_value(input.value(), schema)
  }
  pub(super) fn project_value(&self, input: &Value, schema: &FlowDataSchema) -> Result<FlowPayload, FactoryError> {
    let mut remaining = MAX_FLOW_DATA_BYTES;
    FlowPayload::new(schema, self.value.project(input, &mut remaining)?)
  }
}
impl FlowValueMapping {
  fn validate(&self, depth: usize, count: &mut usize) -> Result<(), FactoryError> {
    *count += 1;
    if depth > MAX_FLOW_SCHEMA_DEPTH || *count > MAX_MAPPING_NODES {
      return Err(invalid());
    }
    match self {
      Self::Pointer { path } => validate_pointer(path)?,
      Self::Constant { .. } => (),
      Self::Object { fields } => {
        if fields.len() > MAX_FLOW_SCHEMA_ENTRIES {
          return Err(invalid());
        }
        for value in fields.values() {
          value.validate(depth + 1, count)?;
        }
      }
    }
    Ok(())
  }
  fn project(&self, root: &Value, remaining: &mut usize) -> Result<Value, FactoryError> {
    match self {
      Self::Pointer { path } => copy_bounded(root.pointer(path).ok_or_else(invalid)?, remaining),
      Self::Constant { value } => copy_bounded(value, remaining),
      Self::Object { fields } => {
        spend(remaining, 2)?;
        fields
          .iter()
          .map(|(name, selection)| {
            spend(
              remaining,
              serde_json::to_vec(name.as_str()).map_err(|_| invalid())?.len() + 2,
            )?;
            Ok((name.as_str().to_owned(), selection.project(root, remaining)?))
          })
          .collect::<Result<serde_json::Map<_, _>, _>>()
          .map(Value::Object)
      }
    }
  }
}
pub(super) fn copy_bounded(value: &Value, remaining: &mut usize) -> Result<Value, FactoryError> {
  // Charge selected bytes before cloning: repeated selections cannot amplify allocation.
  spend(remaining, serde_json::to_vec(value).map_err(|_| invalid())?.len())?;
  Ok(value.clone())
}
fn spend(remaining: &mut usize, bytes: usize) -> Result<(), FactoryError> {
  *remaining = remaining.checked_sub(bytes).ok_or_else(invalid)?;
  Ok(())
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow data mapping",
  }
}
