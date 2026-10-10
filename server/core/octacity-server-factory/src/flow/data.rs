use crate::{FactoryDigest, FactoryError, FactoryKey, ImmutableReference};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Hard retained payload byte ceiling, including schema metadata.
pub const MAX_FLOW_DATA_BYTES: usize = 1_048_576;
/// Hard encoded configured schema ceiling.
pub const MAX_FLOW_SCHEMA_BYTES: usize = 16 * 1024;
/// Hard configured shape nesting ceiling.
pub const MAX_FLOW_SCHEMA_DEPTH: usize = 16;
/// Hard number of fields or finite enum alternatives at one shape node.
pub const MAX_FLOW_SCHEMA_ENTRIES: usize = 64;
/// Hard configured array-length ceiling.
pub const MAX_FLOW_SCHEMA_ITEMS: u16 = 128;

/// Configured shape of a node value; field names carry no runtime stage semantics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type", deny_unknown_fields)]
pub enum FlowValueSchema {
  /// Closed object: every undeclared field is rejected.
  Object {
    /// Declared field shapes and presence requirements.
    fields: BTreeMap<FactoryKey, FlowFieldSchema>,
  },
  /// Bounded UTF-8 text.
  String {
    /// Inclusive lower byte bound.
    min_bytes: usize,
    /// Inclusive upper byte bound.
    max_bytes: usize,
  },
  /// Exact signed integer within configured bounds.
  Integer {
    /// Inclusive lower bound.
    minimum: i64,
    /// Inclusive upper bound.
    maximum: i64,
  },
  /// Boolean observation; schema validation does not grant acceptance.
  Boolean,
  /// Bounded ordered sequence of values sharing one configured shape.
  Array {
    /// Shape of every item.
    item: Box<FlowValueSchema>,
    /// Inclusive lower length bound.
    min_items: u16,
    /// Inclusive upper length bound.
    max_items: u16,
  },
  /// Non-empty finite set of scalar values, including explicit null when declared.
  Enum {
    /// Exact allowed values; object/array alternatives are rejected.
    values: Vec<Value>,
  },
}

/// Presence and shape of one configured object field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowFieldSchema {
  required: bool,
  value: FlowValueSchema,
}
impl FlowFieldSchema {
  /// Declares a field that must be present.
  #[must_use]
  pub const fn required(value: FlowValueSchema) -> Self {
    Self { required: true, value }
  }
  /// Declares a field that may be absent, but is validated when supplied.
  #[must_use]
  pub const fn optional(value: FlowValueSchema) -> Self {
    Self { required: false, value }
  }
}

/// Immutable configured data contract shared by all logical nodes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "DataSchemaWire")]
pub struct FlowDataSchema {
  reference: ImmutableReference,
  shape: FlowValueSchema,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DataSchemaWire {
  reference: ImmutableReference,
  shape: FlowValueSchema,
}
impl TryFrom<DataSchemaWire> for FlowDataSchema {
  type Error = FactoryError;
  fn try_from(wire: DataSchemaWire) -> Result<Self, Self::Error> {
    let schema = Self::new(
      wire.reference.identity().clone(),
      wire.reference.version().clone(),
      wire.shape,
    )?;
    if schema.reference != wire.reference {
      return Err(invalid());
    }
    Ok(schema)
  }
}
impl FlowDataSchema {
  /// Content-addresses the complete configured shape under an operator-owned name/version.
  pub fn new(identity: FactoryKey, version: FactoryKey, shape: FlowValueSchema) -> Result<Self, FactoryError> {
    shape.validate(0)?;
    let bytes = serde_json::to_vec(&(&identity, &version, &shape)).map_err(|_| invalid())?;
    if bytes.len() > MAX_FLOW_SCHEMA_BYTES {
      return Err(invalid());
    }
    let reference = ImmutableReference::new(
      identity,
      version,
      FactoryDigest::sha256("octacity.factory.flow-data-schema.v1", &[&bytes]),
    );
    Ok(Self { reference, shape })
  }
  /// Returns the exact contract name, version and content identity.
  #[must_use]
  pub const fn reference(&self) -> &ImmutableReference {
    &self.reference
  }
}

/// Schema-validated observations; this value cannot assert trusted acceptance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowPayload {
  schema: ImmutableReference,
  value: Value,
}
impl FlowPayload {
  /// Validates observations using the exact configured contract before use.
  pub fn new(schema: &FlowDataSchema, value: Value) -> Result<Self, FactoryError> {
    if !schema.shape.accepts(&value) {
      return Err(invalid());
    }
    let payload = Self {
      schema: schema.reference.clone(),
      value,
    };
    if serde_json::to_vec(&payload).map_err(|_| invalid())?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(payload)
  }
  /// Restores bounded bytes using an exact trusted schema, without accepting a replacement.
  pub fn restore(bytes: &[u8], schema: &FlowDataSchema) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
      schema: ImmutableReference,
      value: Value,
    }
    let wire: Wire = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if &wire.schema != schema.reference() {
      return Err(invalid());
    }
    Self::new(schema, wire.value)
  }
  /// Returns the exact schema consumed by validation.
  #[must_use]
  pub const fn schema(&self) -> &ImmutableReference {
    &self.schema
  }
  /// Returns observations, without lifecycle or evidence authority.
  #[must_use]
  pub const fn value(&self) -> &Value {
    &self.value
  }
}
impl FlowValueSchema {
  fn validate(&self, depth: usize) -> Result<(), FactoryError> {
    if depth > MAX_FLOW_SCHEMA_DEPTH {
      return Err(invalid());
    }
    match self {
      Self::Boolean => (),
      Self::Integer { minimum, maximum } if minimum <= maximum => (),
      Self::String { min_bytes, max_bytes } if min_bytes <= max_bytes && *max_bytes <= MAX_FLOW_DATA_BYTES => (),
      Self::Array {
        item,
        min_items,
        max_items,
      } if min_items <= max_items && *max_items <= MAX_FLOW_SCHEMA_ITEMS => item.validate(depth + 1)?,
      Self::Object { fields } if fields.len() <= MAX_FLOW_SCHEMA_ENTRIES => {
        for field in fields.values() {
          field.value.validate(depth + 1)?;
        }
      }
      Self::Enum { values } if !values.is_empty() && values.len() <= MAX_FLOW_SCHEMA_ENTRIES => {
        if values
          .iter()
          .enumerate()
          .any(|(index, value)| value.is_object() || value.is_array() || values[..index].contains(value))
        {
          return Err(invalid());
        }
      }
      _ => return Err(invalid()),
    }
    Ok(())
  }
  fn accepts(&self, value: &Value) -> bool {
    match self {
      Self::Boolean => value.is_boolean(),
      Self::Integer { minimum, maximum } => value.as_i64().is_some_and(|n| n >= *minimum && n <= *maximum),
      Self::String { min_bytes, max_bytes } => value
        .as_str()
        .is_some_and(|s| s.len() >= *min_bytes && s.len() <= *max_bytes),
      Self::Array {
        item,
        min_items,
        max_items,
      } => value.as_array().is_some_and(|values| {
        values.len() >= usize::from(*min_items)
          && values.len() <= usize::from(*max_items)
          && values.iter().all(|value| item.accepts(value))
      }),
      Self::Enum { values } => values.contains(value),
      Self::Object { fields } => value.as_object().is_some_and(|object| {
        object.keys().all(|name| fields.keys().any(|key| key.as_str() == name))
          && fields.iter().all(|(key, field)| {
            object
              .get(key.as_str())
              .map_or(!field.required, |value| field.value.accepts(value))
          })
      }),
    }
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow data contract",
  }
}
