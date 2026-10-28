//! Common evidence-backed stage conclusion, independent of provider search schemas.
use serde::Deserialize;
use serde_json::{Value, json};

use super::{FactoryMcpBoundCapture, FactoryMcpCallStatus, FactoryMcpEvidenceReference, FactoryNativeCaptureStatus};
use crate::ApplicationError;
use octacity_server_factory::{FlowDataMapping, FlowValueMapping, MAX_FLOW_DATA_BYTES};

/// Explicit independent result-verifier pin, selectable through ordinary Build bindings.
pub fn factory_mcp_evidence_verifier() -> octacity_server_factory::ImmutableReference {
  use octacity_server_factory::{FactoryDigest, FactoryKey, ImmutableReference};
  ImmutableReference::new(
    FactoryKey::new("native-mcp-evidence").expect("static identity"),
    FactoryKey::new("v1").expect("static version"),
    FactoryDigest::content_sha256(b"octacity.factory.native-mcp-evidence.v1"),
  )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedClaim {
  connection: String,
  tool: String,
  arguments_json: String,
  pointer: String,
  #[serde(default)]
  json_text: Option<usize>,
  value_json: String,
  #[serde(default)]
  reference: Option<FactoryMcpEvidenceReference>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Conclusion {
  assessment: Value,
  #[serde(default)]
  evidence: Vec<FactoryMcpEvidenceReference>,
  claims: Vec<ObservedClaim>,
  // A model may echo this field; its contents never establish verification facts.
  #[serde(default, rename = "verification")]
  _untrusted_verification: Option<Value>,
}

impl FactoryMcpBoundCapture<'_> {
  /// Verifies exact response/value references, then supplies owner-generated provenance.
  /// Observed candidate fields do not establish a business duplicate or global absence;
  /// the configured stage gate separately evaluates those evidence obligations.
  pub fn verify_conclusion(&self, value: Value) -> Result<Value, ApplicationError> {
    let invalid = ApplicationError::invalid;
    validate_safe_json(&value)?;
    let conclusion: Conclusion = serde_json::from_value(value).map_err(|_| invalid())?;
    if conclusion.evidence.len() > super::mcp::MAX_FACTORY_MCP_CAPTURE_CALLS
      || conclusion.claims.len() > super::mcp::MAX_FACTORY_MCP_CAPTURE_CALLS
    {
      return Err(invalid());
    }
    let mut successful = !conclusion.claims.is_empty() && self.capture.status() == FactoryNativeCaptureStatus::Complete;
    for (index, reference) in conclusion.evidence.iter().enumerate() {
      if conclusion.evidence[..index].contains(reference) {
        return Err(invalid());
      }
      let call = self.capture.resolve_evidence(reference)?;
      successful &= call.status() == FactoryMcpCallStatus::Completed;
    }
    let mut evidence = conclusion.evidence;
    let mut claims = Vec::new();
    for claim in conclusion.claims {
      let arguments: Value = serde_json::from_str(&claim.arguments_json).map_err(|_| invalid())?;
      let wanted: Value = serde_json::from_str(&claim.value_json).map_err(|_| invalid())?;
      validate_safe_json(&arguments)?;
      validate_safe_json(&wanted)?;
      let mut matching = self.capture.mcp_calls().iter().filter_map(|call| {
        if call.connection() != claim.connection || call.tool() != claim.tool || call.arguments() != &arguments {
          return None;
        }
        selected_value(call, &claim.pointer, claim.json_text)
          .ok()
          .filter(|value| value == &wanted)
          .map(|value| (call, value))
      });
      let (call, _) = matching.next().ok_or_else(invalid)?;
      if matching.next().is_some()
        || claim
          .reference
          .as_ref()
          .is_some_and(|reference| reference != &call.reference())
      {
        return Err(invalid());
      }
      successful &= call.status() == FactoryMcpCallStatus::Completed;
      let reference = call.reference();
      if !evidence.contains(&reference) {
        evidence.push(reference.clone());
      }
      let mut retained_claim = json!({"reference":reference,"connection":call.connection(),"tool":call.tool(),
        "arguments_json":serde_json::to_string(call.arguments()).map_err(|_| invalid())?,"pointer":claim.pointer,
        "value_json":serde_json::to_string(&wanted).map_err(|_| invalid())?});
      if let Some(index) = claim.json_text {
        retained_claim["json_text"] = json!(index);
      }
      claims.push(retained_claim);
    }
    let trace = self.capture.trace();
    let provenance = self.capture.provenance();
    let value = json!({"assessment":conclusion.assessment,"evidence":evidence,"claims":claims,
    "verification":{
      "status":if successful {"observed"} else {"needs_review"}, "coverage":"unknown",
      "global_absence_proven":false,"duplicate_proven":false,
      "invocation":self.invocation(),"node_attempt_id":self.node(),
      "producer":{"build_id":trace.build_id,"attempt_id":trace.attempt_id,"job_id":trace.job_id},
      "trace_artifact_id":trace.artifact_id,"trace_digest":trace.digest.to_string(),
      "provenance_artifact_id":provenance.artifact_id,"provenance_digest":provenance.digest.to_string(),
      "capture_status":self.capture.status(),"observations":self.observations(),
    }});
    validate_safe_json(&value)?;
    Ok(value)
  }
}

fn selected_value(
  call: &super::FactoryMcpCallObservation,
  pointer: &str,
  json_text: Option<usize>,
) -> Result<Value, ApplicationError> {
  let invalid = ApplicationError::invalid;
  FlowDataMapping::new(FlowValueMapping::Pointer { path: pointer.into() }).map_err(|_| invalid())?;
  let response = call.result().ok_or_else(invalid)?;
  let decoded;
  let selected = if let Some(index) = json_text {
    let content = response["content"].get(index).ok_or_else(invalid)?;
    if content["type"] != "text" {
      return Err(invalid());
    }
    decoded = serde_json::from_str::<Value>(content["text"].as_str().ok_or_else(invalid)?).map_err(|_| invalid())?;
    &decoded
  } else {
    response
  };
  selected.pointer(pointer).cloned().ok_or_else(invalid)
}

// JSON held inside native text/model strings becomes a new value after decoding.
// Apply the same safety owner before and after that transformation.
fn validate_safe_json(value: &Value) -> Result<(), ApplicationError> {
  let encoded = serde_json::to_string(value).map_err(|_| ApplicationError::invalid())?;
  if encoded.len() > MAX_FLOW_DATA_BYTES || octacity_server_factory::contains_sensitive_material(&encoded) {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}
