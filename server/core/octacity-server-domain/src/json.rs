use serde_json::{Map, Value};
use thiserror::Error;

/// Maximum nesting accepted by server-owned canonical JSON values.
pub const MAX_CANONICAL_JSON_DEPTH: usize = 128;

/// Canonicalizes JSON object keys while enforcing a process-safe nesting bound.
pub fn canonicalize_json(value: Value) -> Result<Value, JsonCanonicalizationError> {
  canonicalize_at(value, 0)
}

fn canonicalize_at(value: Value, depth: usize) -> Result<Value, JsonCanonicalizationError> {
  if depth > MAX_CANONICAL_JSON_DEPTH {
    return Err(JsonCanonicalizationError::TooDeep);
  }
  match value {
    Value::Array(values) => values
      .into_iter()
      .map(|value| canonicalize_at(value, depth + 1))
      .collect::<Result<Vec<_>, _>>()
      .map(Value::Array),
    Value::Object(values) => {
      let mut entries: Vec<_> = values.into_iter().collect();
      entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
      entries
        .into_iter()
        .map(|(key, value)| canonicalize_at(value, depth + 1).map(|value| (key, value)))
        .collect::<Result<Map<_, _>, _>>()
        .map(Value::Object)
    }
    scalar => Ok(scalar),
  }
}

/// Failure to construct bounded canonical JSON.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum JsonCanonicalizationError {
  /// The value exceeds the server-wide nesting bound.
  #[error("JSON value exceeds the maximum nesting depth")]
  TooDeep,
}
