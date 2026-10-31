//! Codex Structured Outputs projection of an immutable Flow schema.
//!
//! Provider-only nullable optionals are converted back before the unchanged
//! domain validator runs. Raw retained records and Flow schema digests stay exact.
use crate::CodexCompilationError;
use octacity_server_factory::FlowValueSchema;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn json_schema(shape: &FlowValueSchema) -> Result<Value, CodexCompilationError> {
  Ok(match shape {
    FlowValueSchema::Reference { .. } => return Err(CodexCompilationError::NodeBindings),
    FlowValueSchema::Object { fields } => {
      let properties = fields
        .iter()
        .map(|(name, field)| {
          let schema = json_schema(field.value())?;
          // Native Structured Outputs requires every property to be present.
          // Null represents an absent optional field at this transport boundary.
          let schema = if field.is_required() {
            schema
          } else {
            json!({"anyOf":[schema, {"type":"null"}]})
          };
          Ok((name.as_str().to_owned(), schema))
        })
        .collect::<Result<BTreeMap<_, _>, CodexCompilationError>>()?;
      let required = fields.keys().map(|name| name.as_str()).collect::<Vec<_>>();
      json!({"type":"object", "properties":properties, "required":required, "additionalProperties":false})
    }
    // JSON Schema counts Unicode code points; the authoritative Flow payload
    // validator still enforces UTF-8 byte bounds after verified result decoding.
    FlowValueSchema::String { min_bytes, max_bytes } => {
      json!({"type":"string", "minLength":min_bytes.div_ceil(4), "maxLength":max_bytes})
    }
    FlowValueSchema::Integer { minimum, maximum } => json!({"type":"integer", "minimum":minimum, "maximum":maximum}),
    FlowValueSchema::Boolean => json!({"type":"boolean"}),
    FlowValueSchema::Array {
      item,
      min_items,
      max_items,
    } => json!({"type":"array", "items":json_schema(item)?, "minItems":min_items, "maxItems":max_items}),
    FlowValueSchema::Enum { values } => json!({"enum":values}),
    FlowValueSchema::ExternallyTagged { variants } => {
      let alternatives = variants.iter().map(|(name, shape)| {
        Ok(match shape {
          None => json!({"type":"string", "const":name}),
          Some(shape) => json!({"type":"object", "properties":{name:json_schema(shape)?}, "required":[name], "additionalProperties":false}),
        })
      }).collect::<Result<Vec<_>, CodexCompilationError>>()?;
      json!({"oneOf":alternatives})
    }
  })
}

/// Restores absent optional fields without accepting unknown fields, altering
/// required values, or discarding null explicitly declared by a finite enum.
pub(crate) fn restore_optional_fields(shape: &FlowValueSchema, value: &mut Value) {
  match (shape, value) {
    (FlowValueSchema::Object { fields }, Value::Object(object)) => {
      for (name, field) in fields {
        let nullable = matches!(field.value(), FlowValueSchema::Enum { values } if values.contains(&Value::Null));
        if !field.is_required() && !nullable && object.get(name.as_str()).is_some_and(Value::is_null) {
          object.remove(name.as_str());
        } else if let Some(value) = object.get_mut(name.as_str()) {
          restore_optional_fields(field.value(), value);
        }
      }
    }
    (FlowValueSchema::Array { item, .. }, Value::Array(values)) => {
      for value in values {
        restore_optional_fields(item, value);
      }
    }
    (FlowValueSchema::ExternallyTagged { variants }, Value::Object(object)) => {
      for (name, value) in object {
        if let Some(Some(shape)) = variants.get(name) {
          restore_optional_fields(shape, value);
        }
      }
    }
    _ => {}
  }
}
