use serde::{Deserialize, Serialize};
use serde_json::Value;

/// REST representation of one immutable ordered Job event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobEventResource {
  /// Positive sequence within one Job stream.
  pub sequence: u64,
  /// Stable provider-neutral event classification.
  pub kind: String,
  /// Source-observed Unix time in milliseconds.
  pub occurred_at_unix_ms: i64,
  /// Bounded canonical event payload.
  pub payload: Value,
}

/// Ordered Job-event page with the cursor to use for the next read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobEventPage {
  /// Contiguous events after the caller-supplied cursor.
  pub items: Vec<JobEventResource>,
  /// Last returned sequence, or the current durable cursor when empty.
  pub cursor: u64,
}
