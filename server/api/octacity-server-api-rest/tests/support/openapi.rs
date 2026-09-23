use serde_json::Value;

pub fn assert_component_exists(document: &Value, schema: &str) {
  assert!(
    document["components"]["schemas"].get(schema).is_some(),
    "missing component schema {schema}"
  );
}

pub fn assert_required_header(operation: &Value, name: &str, expected: bool) {
  let actual = operation["parameters"]
    .as_array()
    .into_iter()
    .flatten()
    .any(|parameter| parameter["in"] == "header" && parameter["name"] == name && parameter["required"] == true);
  assert_eq!(actual, expected, "header drift for {name}");
}

pub fn assert_json_matches_component(document: &Value, component: &str, value: &Value) {
  let schema = &document["components"]["schemas"][component];
  if let Err(error) = validate(document, schema, value, component) {
    panic!("OpenAPI schema drift for {component}: {error}");
  }
}

fn validate(document: &Value, schema: &Value, value: &Value, path: &str) -> Result<(), String> {
  if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
    let name = reference
      .strip_prefix("#/components/schemas/")
      .ok_or_else(|| format!("{path}: unsupported reference {reference}"))?;
    return validate(document, &document["components"]["schemas"][name], value, path);
  }
  if let Some(constant) = schema.get("const")
    && value != constant
  {
    return Err(format!("{path}: expected constant {constant}, got {value}"));
  }
  if let Some(variants) = schema
    .get("anyOf")
    .or_else(|| schema.get("oneOf"))
    .and_then(Value::as_array)
  {
    if variants
      .iter()
      .any(|variant| validate(document, variant, value, path).is_ok())
    {
      return Ok(());
    }
    return Err(format!("{path}: value matches no documented variant"));
  }
  if let Some(values) = schema.get("enum").and_then(Value::as_array)
    && !values.contains(value)
  {
    return Err(format!("{path}: {value} is outside the documented enum"));
  }
  match schema.get("type").and_then(Value::as_str) {
    None => Ok(()),
    Some("object") => validate_object(document, schema, value, path),
    Some("array") => validate_array(document, schema, value, path),
    Some("string") => validate_string(schema, value, path),
    Some("integer") => validate_integer(schema, value, path),
    Some("boolean") if value.is_boolean() => Ok(()),
    Some("null") if value.is_null() => Ok(()),
    Some(expected) => Err(format!("{path}: expected {expected}, got {value}")),
  }
}

fn validate_object(document: &Value, schema: &Value, value: &Value, path: &str) -> Result<(), String> {
  let object = value
    .as_object()
    .ok_or_else(|| format!("{path}: expected object, got {value}"))?;
  if let Some(required) = schema.get("required").and_then(Value::as_array) {
    for name in required.iter().filter_map(Value::as_str) {
      if !object.contains_key(name) {
        return Err(format!("{path}: missing required property {name}"));
      }
    }
  }
  let properties = schema.get("properties").and_then(Value::as_object);
  for (name, child) in object {
    if let Some(child_schema) = properties.and_then(|properties| properties.get(name)) {
      validate(document, child_schema, child, &format!("{path}.{name}"))?;
      continue;
    }
    match schema.get("additionalProperties") {
      Some(Value::Bool(false)) => return Err(format!("{path}: undocumented property {name}")),
      Some(additional) if additional.is_object() => {
        validate(document, additional, child, &format!("{path}.{name}"))?;
      }
      _ => {}
    }
  }
  Ok(())
}

fn validate_array(document: &Value, schema: &Value, value: &Value, path: &str) -> Result<(), String> {
  let items = value
    .as_array()
    .ok_or_else(|| format!("{path}: expected array, got {value}"))?;
  if let Some(item_schema) = schema.get("items") {
    for (index, item) in items.iter().enumerate() {
      validate(document, item_schema, item, &format!("{path}[{index}]"))?;
    }
  }
  Ok(())
}

fn validate_string(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
  let value = value
    .as_str()
    .ok_or_else(|| format!("{path}: expected string, got {value}"))?;
  let length = value.chars().count() as u64;
  if schema
    .get("minLength")
    .and_then(Value::as_u64)
    .is_some_and(|minimum| length < minimum)
  {
    return Err(format!("{path}: string is shorter than minLength"));
  }
  if schema
    .get("maxLength")
    .and_then(Value::as_u64)
    .is_some_and(|maximum| length > maximum)
  {
    return Err(format!("{path}: string is longer than maxLength"));
  }
  Ok(())
}

fn validate_integer(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
  let integer = value
    .as_i64()
    .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
    .ok_or_else(|| format!("{path}: expected integer, got {value}"))?;
  if schema
    .get("minimum")
    .and_then(Value::as_i64)
    .is_some_and(|minimum| integer < minimum)
  {
    return Err(format!("{path}: integer is below minimum"));
  }
  if schema
    .get("maximum")
    .and_then(Value::as_i64)
    .is_some_and(|maximum| integer > maximum)
  {
    return Err(format!("{path}: integer is above maximum"));
  }
  Ok(())
}
