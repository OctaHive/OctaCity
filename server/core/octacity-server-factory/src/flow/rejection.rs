use crate::{FactorySafeText, FlowDataSchema, FlowValueSchema, MAX_FLOW_DATA_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Safe explanation of rejected data. Contains schema locations and counts,
/// never model text, input documents, credentials or execution authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowResultRejection {
  /// The decoded result does not match its admitted shape.
  Schema,
  /// A decoded string violates the admitted UTF-8 byte interval.
  StringBytes {
    /// Diagnostic location; `/` denotes the root result.
    path: FactorySafeText,
    /// Inclusive minimum from the admitted schema.
    minimum: usize,
    /// Inclusive maximum from the admitted schema.
    maximum: usize,
    /// Observed UTF-8 bytes, without their contents.
    actual: usize,
  },
}

impl FlowResultRejection {
  pub(super) fn is_valid(&self) -> bool {
    match self {
      Self::Schema => true,
      Self::StringBytes {
        minimum,
        maximum,
        actual,
        ..
      } => minimum <= maximum && *maximum <= MAX_FLOW_DATA_BYTES && (actual < minimum || actual > maximum),
    }
  }
}

impl FlowDataSchema {
  /// Explains a rejection using metadata only. The unchanged payload validator
  /// remains the acceptance boundary; valid values have no rejection.
  pub fn rejection(&self, value: &Value) -> Option<FlowResultRejection> {
    if self.shape().accepts(value) {
      return None;
    }
    Some(string_rejection(self.shape(), value, "").unwrap_or(FlowResultRejection::Schema))
  }
}

// Run only on rejected data, leaving successful validation allocation-free.
fn string_rejection(shape: &FlowValueSchema, value: &Value, path: &str) -> Option<FlowResultRejection> {
  match (shape, value) {
    (FlowValueSchema::String { min_bytes, max_bytes }, Value::String(text))
      if text.len() < *min_bytes || text.len() > *max_bytes =>
    {
      Some(FlowResultRejection::StringBytes {
        path: FactorySafeText::new(if path.is_empty() { "/" } else { path }).ok()?,
        minimum: *min_bytes,
        maximum: *max_bytes,
        actual: text.len(),
      })
    }
    (FlowValueSchema::Object { fields }, Value::Object(object)) => fields.iter().find_map(|(key, field)| {
      string_rejection(
        field.value(),
        object.get(key.as_str())?,
        &format!("{path}/{}", key.as_str()),
      )
    }),
    (FlowValueSchema::Array { item, .. }, Value::Array(items)) => items
      .iter()
      .enumerate()
      .find_map(|(index, value)| string_rejection(item, value, &format!("{path}/{index}"))),
    (FlowValueSchema::ExternallyTagged { variants }, Value::Object(object)) => {
      object.iter().find_map(|(key, value)| {
        string_rejection(
          variants.get(key)?.as_ref()?,
          value,
          &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
        )
      })
    }
    _ => None,
  }
}
