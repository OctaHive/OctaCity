//! Exact schema bounds at the HTTP boundary. Domain serialization and hashes
//! remain numeric; only declared schema slots are converted, never user values.
use serde_json::Value;

const MAX_EXACT_JSON_INTEGER: i64 = 9_007_199_254_740_991;

pub(super) fn shape(value: &mut Value) {
  match value["type"].as_str() {
    Some("integer") => {
      for field in ["minimum", "maximum"] {
        if let Some(number) = value[field].as_i64()
          && !(-MAX_EXACT_JSON_INTEGER..=MAX_EXACT_JSON_INTEGER).contains(&number)
        {
          value[field] = Value::String(number.to_string());
        }
      }
    }
    Some("object") => {
      if let Some(fields) = value["fields"].as_object_mut() {
        for field in fields.values_mut() {
          shape(&mut field["value"]);
        }
      }
    }
    Some("array") => shape(&mut value["item"]),
    Some("externally_tagged") => {
      if let Some(variants) = value["variants"].as_object_mut() {
        for variant in variants.values_mut().filter(|variant| !variant.is_null()) {
          shape(variant);
        }
      }
    }
    _ => {}
  }
}

pub(super) fn schema(mut value: Value) -> Value {
  shape(&mut value["shape"]);
  value
}

/// Both immutable Flow configurations and editable drafts own this registry.
pub(super) fn flow(mut value: Value) -> Value {
  if let Some(schemas) = value["data_schemas"].as_array_mut() {
    for schema in schemas {
      shape(&mut schema["shape"]);
    }
  }
  value
}

pub(super) fn configuration(mut value: Value) -> Value {
  value["flow"] = flow(value["flow"].take());
  value
}

pub(super) fn preset(mut value: Value) -> Value {
  if !value["flow"].is_null() {
    value["flow"] = flow(value["flow"].take());
  }
  if !value["configuration"].is_null() {
    value["configuration"] = configuration(value["configuration"].take());
  }
  if let Some(children) = value["dependencies"].as_array_mut() {
    for child in children {
      child["definition"] = flow(child["definition"].take());
    }
  }
  value
}

/// Runtime capability DTOs expose parameter/environment schemas alongside safe metadata.
pub(super) fn profile_capabilities(mut value: Value) -> Value {
  fn entry(value: &mut Value) {
    if let Some(parameters) = value.get_mut("parameters")
      && let Some(value) = parameters.get_mut("shape")
    {
      shape(value);
    }
    if let Some(environment) = value.get_mut("public_environment").and_then(Value::as_object_mut) {
      for schema in environment.values_mut() {
        if let Some(value) = schema.get_mut("shape") {
          shape(value);
        }
      }
    }
  }
  if let Some(models) = value.pointer_mut("/model_catalog/models").and_then(Value::as_array_mut) {
    for model in models {
      entry(model);
    }
  }
  entry(&mut value);
  if let Some(items) = value.get_mut("items").and_then(Value::as_array_mut) {
    for item in items {
      entry(item);
    }
  }
  value
}

/// Installed Source choices expose the native action's declared receipt schema.
pub(super) fn source_choices(mut value: Value) -> Value {
  if let Some(items) = value.get_mut("items").and_then(Value::as_array_mut) {
    for item in items {
      if let Some(output) = item.pointer_mut("/reporting/action/output/shape") {
        shape(output);
      }
    }
  }
  value
}

/// Shared action pages and exact observations expose only admitted response schemas.
pub(super) fn actions(mut value: Value) -> Value {
  fn description(value: &mut Value) {
    if let Some(schemas) = value.pointer_mut("/kind/schemas").and_then(Value::as_array_mut) {
      for schema in schemas {
        if let Some(shape_value) = schema.get_mut("shape") {
          shape(shape_value);
        }
      }
    }
  }
  if let Some(items) = value.get_mut("items").and_then(Value::as_array_mut) {
    for item in items {
      if let Some(action) = item.get_mut("action") {
        description(action);
      }
    }
  }
  if let Some(action) = value.get_mut("description") {
    description(action);
  }
  value
}
