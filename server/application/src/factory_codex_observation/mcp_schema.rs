//! Shared closed conclusion envelope; native provider values remain opaque JSON.
use super::mcp::MAX_FACTORY_MCP_CAPTURE_CALLS;
use octacity_server_factory::{FactoryKey, FlowFieldSchema, FlowValueSchema, MAX_FLOW_POINTER_BYTES};
use serde_json::json;

fn text(max_bytes: usize) -> FlowValueSchema {
  FlowValueSchema::String {
    min_bytes: 1,
    max_bytes,
  }
}
fn integer() -> FlowValueSchema {
  FlowValueSchema::Integer {
    minimum: 0,
    maximum: i64::MAX,
  }
}
fn choices(values: &[&str]) -> FlowValueSchema {
  FlowValueSchema::Enum {
    values: values.iter().map(|value| json!(value)).collect(),
  }
}
fn object<const N: usize>(fields: [(&str, FlowValueSchema); N]) -> FlowValueSchema {
  FlowValueSchema::Object {
    fields: fields
      .into_iter()
      .map(|(key, value)| {
        (
          FactoryKey::new(key).expect("static field"),
          FlowFieldSchema::required(value),
        )
      })
      .collect(),
  }
}
fn optional(mut schema: FlowValueSchema, name: &str, shape: FlowValueSchema) -> FlowValueSchema {
  let FlowValueSchema::Object { fields } = &mut schema else {
    unreachable!()
  };
  fields.insert(
    FactoryKey::new(name).expect("static field"),
    FlowFieldSchema::optional(shape),
  );
  schema
}
fn array(shape: FlowValueSchema) -> FlowValueSchema {
  FlowValueSchema::Array {
    item: Box::new(shape),
    min_items: 0,
    max_items: MAX_FACTORY_MCP_CAPTURE_CALLS
      .try_into()
      .expect("bounded MCP capture limit"),
  }
}

/// Closed ordinary stage schema with an operator-selected assessment and native-value claims.
/// It contains no common external-provider query or candidate schema.
pub fn factory_mcp_conclusion_shape(assessment: FlowValueSchema) -> FlowValueSchema {
  let pin = object([("identity", text(128)), ("version", text(128)), ("digest", text(64))]);
  let artifact = object([
    ("artifact_id", text(36)),
    ("content_digest", text(64)),
    ("encoded_size", integer()),
  ]);
  let reference = optional(
    object([("trace", artifact), ("sequence", integer()), ("call_id", text(256))]),
    "result_digest",
    text(64),
  );
  let claim = optional(
    object([
      ("reference", reference.clone()),
      ("connection", text(64)),
      ("tool", text(256)),
      ("arguments_json", text(64 * 1024)),
      (
        "pointer",
        FlowValueSchema::String {
          min_bytes: 0,
          max_bytes: MAX_FLOW_POINTER_BYTES,
        },
      ),
      ("value_json", text(64 * 1024)),
    ]),
    "json_text",
    integer(),
  );
  let observation = optional(
    object([
      ("reference", reference.clone()),
      ("connection", text(64)),
      ("configuration", pin.clone()),
      ("tool", text(256)),
      ("arguments_digest", text(64)),
      ("started_sequence", integer()),
      ("observed_at", integer()),
      ("status", choices(&["completed", "failed", "interrupted", "malformed"])),
      ("coverage", choices(&["unknown"])),
    ]),
    "resource",
    pin,
  );
  let verification = object([
    ("status", choices(&["observed", "needs_review"])),
    ("coverage", choices(&["unknown"])),
    (
      "global_absence_proven",
      FlowValueSchema::Enum {
        values: vec![json!(false)],
      },
    ),
    (
      "duplicate_proven",
      FlowValueSchema::Enum {
        values: vec![json!(false)],
      },
    ),
    ("invocation", text(64)),
    ("node_attempt_id", text(36)),
    (
      "producer",
      object([("build_id", text(36)), ("attempt_id", text(36)), ("job_id", text(36))]),
    ),
    ("trace_artifact_id", text(36)),
    ("trace_digest", text(64)),
    ("provenance_artifact_id", text(36)),
    ("provenance_digest", text(64)),
    ("capture_status", choices(&["complete", "cancelled", "failed"])),
    ("observations", array(observation)),
  ]);
  object([
    ("assessment", assessment),
    ("evidence", array(reference)),
    ("claims", array(claim)),
    ("verification", verification),
  ])
}

/// Projects the retained contract into model-authored fields using the same typed
/// claim shape and bounds. The compiler need not know this envelope's layout.
pub(crate) fn model_conclusion_shape(retained: &FlowValueSchema) -> Option<FlowValueSchema> {
  let key = |name| FactoryKey::new(name).expect("static field");
  let FlowValueSchema::Object { mut fields } = retained.clone() else {
    return None;
  };
  fields.remove(&key("verification"))?;
  fields.remove(&key("evidence"));
  let claim_field = fields.get_mut(&key("claims"))?;
  let FlowValueSchema::Array {
    item,
    min_items,
    max_items,
  } = claim_field.value().clone()
  else {
    return None;
  };
  let FlowValueSchema::Object {
    fields: mut claim_fields,
  } = *item
  else {
    return None;
  };
  claim_fields.remove(&key("reference"));
  let claims = FlowValueSchema::Array {
    item: Box::new(FlowValueSchema::Object { fields: claim_fields }),
    min_items,
    max_items,
  };
  *claim_field = if claim_field.is_required() {
    FlowFieldSchema::required(claims)
  } else {
    FlowFieldSchema::optional(claims)
  };
  Some(FlowValueSchema::Object { fields })
}
