use super::predicate::validate_pointer;
use crate::{
  FactoryError, FactoryKey, FlowDataSchema, FlowPayload, FlowValueSchema, MAX_FLOW_DATA_BYTES, MAX_FLOW_SCHEMA_DEPTH,
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
  /// Constructs a bounded ordered list from explicit selections.
  Array {
    /// Declared items; unselected data never propagates.
    items: Vec<FlowValueMapping>,
  },
  /// Selects one explicit projection according to declared input presence.
  /// A failed selected projection never falls back to the other branch.
  IfPresent {
    /// Validated RFC 6901 presence test; no discovery or authority is implied.
    path: String,
    /// Projection used when the selected field exists, including explicit null.
    then: Box<FlowValueMapping>,
    /// Projection used only when the selected field is absent.
    otherwise: Box<FlowValueMapping>,
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
  /// Returns the validated expression for callers checking a narrower runtime contract.
  pub const fn value(&self) -> &FlowValueMapping {
    &self.value
  }

  /// Checks every source pointer that can populate one projected field.
  /// Literals and paths without an explicit source fail closed.
  pub fn selects_only(&self, path: &str, check: &mut impl FnMut(&str) -> bool) -> bool {
    self.value.selects_only(path, check)
  }

  /// Projects only configured fields and validates the complete target contract.
  ///
  /// Missing data fails closed. Projection grants no acceptance or execution authority.
  pub fn project(&self, input: &FlowPayload, schema: &FlowDataSchema) -> Result<FlowPayload, FactoryError> {
    self.project_value(input.value(), schema)
  }
  /// Projects a trusted JSON source through the bounded mapping and exact target schema.
  /// Callers establish source provenance; this method grants no execution authority.
  pub fn project_value(&self, input: &Value, schema: &FlowDataSchema) -> Result<FlowPayload, FactoryError> {
    FlowPayload::new(schema, self.project_raw(input)?)
  }
  /// Checks that declared input paths and projected value types fit a consumer.
  /// Optional source values still have to be present when their projection runs.
  pub fn fits(&self, input: &FlowValueSchema, target: &FlowValueSchema) -> bool {
    self.value.fits(input, target)
  }

  pub(super) fn fits_pointers(
    &self,
    target: &FlowValueSchema,
    mut check: impl FnMut(&str, &FlowValueSchema, &[&str]) -> bool,
  ) -> bool {
    self.value.fits_checked(target, &mut Vec::new(), &mut check)
  }

  /// Whether this mapping can be checked without observing an admitted input.
  pub(crate) fn is_input_independent(&self) -> bool {
    self.value.is_input_independent()
  }
  /// Evaluates a bounded projection for a caller that validates the result against its own contract.
  pub fn project_raw(&self, input: &Value) -> Result<Value, FactoryError> {
    let mut remaining = MAX_FLOW_DATA_BYTES;
    self.value.project(input, &mut remaining)
  }
}
impl FlowValueMapping {
  fn selects_only(&self, path: &str, check: &mut impl FnMut(&str) -> bool) -> bool {
    match self {
      Self::Pointer { path: source } => check(&format!("{source}{path}")),
      Self::IfPresent { then, otherwise, .. } => then.selects_only(path, check) && otherwise.selects_only(path, check),
      Self::Object { fields } => projection_segment(path).is_some_and(|(key, tail)| {
        fields
          .iter()
          .find(|(name, _)| name.as_str() == key)
          .is_some_and(|(_, value)| value.selects_only(tail, check))
      }),
      Self::Array { items } => projection_segment(path).is_some_and(|(index, tail)| {
        index
          .parse::<usize>()
          .ok()
          .and_then(|index| items.get(index))
          .is_some_and(|value| value.selects_only(tail, check))
      }),
      Self::Constant { .. } => false,
    }
  }

  fn fits(&self, input: &FlowValueSchema, target: &FlowValueSchema) -> bool {
    self.fits_checked(target, &mut Vec::new(), &mut |path, target, _| {
      schema_pointer(input, path, "", |_| true).is_some_and(|shape| target.covers(shape))
    })
  }

  fn fits_checked<'a>(
    &'a self,
    target: &FlowValueSchema,
    present: &mut Vec<&'a str>,
    check: &mut impl FnMut(&str, &FlowValueSchema, &[&str]) -> bool,
  ) -> bool {
    match self {
      Self::Pointer { path } => check(path, target, present),
      Self::Constant { value } => target.accepts(value),
      Self::Object { fields } => {
        let FlowValueSchema::Object { fields: expected } = target else {
          return false;
        };
        fields.iter().all(|(name, mapping)| {
          expected
            .get(name)
            .is_some_and(|field| mapping.fits_checked(field.value(), present, check))
        }) && expected
          .iter()
          .all(|(name, field)| !field.is_required() || fields.contains_key(name))
      }
      Self::Array { items } => {
        let FlowValueSchema::Array {
          item,
          min_items,
          max_items,
        } = target
        else {
          return false;
        };
        (usize::from(*min_items)..=usize::from(*max_items)).contains(&items.len())
          && items.iter().all(|mapping| mapping.fits_checked(item, present, check))
      }
      Self::IfPresent { path, then, otherwise } => {
        present.push(path);
        let valid = then.fits_checked(target, present, check);
        present.pop();
        valid && otherwise.fits_checked(target, present, check)
      }
    }
  }

  fn is_input_independent(&self) -> bool {
    match self {
      Self::Constant { .. } => true,
      Self::Object { fields } => fields.values().all(Self::is_input_independent),
      Self::Array { items } => items.iter().all(Self::is_input_independent),
      Self::Pointer { .. } | Self::IfPresent { .. } => false,
    }
  }
  fn validate(&self, depth: usize, count: &mut usize) -> Result<(), FactoryError> {
    *count += 1;
    if depth > MAX_FLOW_SCHEMA_DEPTH || *count > MAX_MAPPING_NODES {
      return Err(invalid());
    }
    match self {
      Self::Pointer { path } => validate_pointer(path)?,
      Self::Constant { .. } => (),
      Self::Array { items } => {
        if items.len() > usize::from(crate::MAX_FLOW_SCHEMA_ITEMS) {
          return Err(invalid());
        }
        for item in items {
          item.validate(depth + 1, count)?;
        }
      }
      Self::IfPresent { path, then, otherwise } => {
        validate_pointer(path)?;
        then.validate(depth + 1, count)?;
        otherwise.validate(depth + 1, count)?;
      }
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
      Self::Array { items } => {
        spend(remaining, 2 + items.len().saturating_sub(1))?;
        items
          .iter()
          .map(|item| item.project(root, remaining))
          .collect::<Result<Vec<_>, _>>()
          .map(Value::Array)
      }
      Self::IfPresent { path, then, otherwise } => {
        if root.pointer(path).is_some() { then } else { otherwise }.project(root, remaining)
      }
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

pub(super) fn projection_segment(path: &str) -> Option<(String, &str)> {
  let path = path.strip_prefix('/')?;
  let (head, tail) = path.find('/').map_or((path, ""), |index| path.split_at(index));
  Some((head.replace("~1", "/").replace("~0", "~"), tail))
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

// A shared typed pointer walk. Ordinary mappings preserve their existing presence
// contract; Incoming qualification additionally requires a dominating presence guard.
pub(super) fn schema_pointer<'a>(
  mut shape: &'a FlowValueSchema,
  path: &str,
  prefix: &str,
  mut optional: impl FnMut(&str) -> bool,
) -> Option<&'a FlowValueSchema> {
  let mut prefix = prefix.to_owned();
  for segment in path.split('/').skip(1) {
    prefix.push('/');
    prefix.push_str(segment);
    let key = segment.replace("~1", "/").replace("~0", "~");
    shape = match shape {
      FlowValueSchema::Object { fields } => {
        let field = fields.iter().find(|(name, _)| name.as_str() == key)?.1;
        if !field.is_required() && !optional(&prefix) {
          return None;
        }
        field.value()
      }
      FlowValueSchema::Array {
        item,
        min_items,
        max_items,
      } => {
        let index = key.parse::<u16>().ok()?;
        if index >= *max_items || index.to_string() != key || (index >= *min_items && !optional(&prefix)) {
          return None;
        }
        item
      }
      FlowValueSchema::ExternallyTagged { variants } => {
        if variants.len() != 1 && !optional(&prefix) {
          return None;
        }
        variants.get(&key)?.as_deref()?
      }
      _ => return None,
    };
  }
  Some(shape)
}
