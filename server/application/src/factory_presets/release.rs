//! Immutable bundled release assets; no caller-supplied path or new registry.
use crate::*;
use octacity_server_factory::*;

// Immutable release assets use the existing manifest resolver. No request supplies a filesystem path.
pub(super) fn released_triage() -> Result<(FlowDefinitionTemplate, ImmutableReference), ApplicationError> {
  let template = FlowDefinitionTemplate::from_manifest(
    include_bytes!("../../../../config/factory/presets/triage/v1.json"),
    |path| {
      let bytes: &[u8] = match path.as_str() {
        "presets/triage/inputs-v1.json" => include_bytes!("../../../../config/factory/presets/triage/inputs-v1.json"),
        "presets/triage/eligibility-results-v1.json" => {
          include_bytes!("../../../../config/factory/presets/triage/eligibility-results-v1.json")
        }
        "flows/base-flow/schemas/artifact_reference.json" => {
          include_bytes!("../../../../config/factory/flows/base-flow/schemas/artifact_reference.json")
        }
        "presets/triage/classification-results-v1.json" => {
          include_bytes!("../../../../config/factory/presets/triage/classification-results-v1.json")
        }
        "flows/base-flow/schemas/work.json" => {
          include_bytes!("../../../../config/factory/flows/base-flow/schemas/work.json")
        }
        "presets/triage/eligibility-v1.json" => {
          include_bytes!("../../../../config/factory/presets/triage/eligibility-v1.json")
        }
        "presets/triage/eligibility-gate-v1.json" => {
          include_bytes!("../../../../config/factory/presets/triage/eligibility-gate-v1.json")
        }
        "presets/triage/classification-v1.json" => {
          include_bytes!("../../../../config/factory/presets/triage/classification-v1.json")
        }
        "presets/triage/classification-gate-v1.json" => {
          include_bytes!("../../../../config/factory/presets/triage/classification-gate-v1.json")
        }
        "presets/triage/routing-v1.json" => include_bytes!("../../../../config/factory/presets/triage/routing-v1.json"),
        "presets/triage/direct-classification-v1.json" => {
          include_bytes!("../../../../config/factory/presets/triage/direct-classification-v1.json")
        }
        _ => {
          return Err(FactoryError::InvalidConfiguration {
            field: "preset release part",
          });
        }
      };
      Ok(bytes.to_vec())
    },
  )
  .map_err(|_| ApplicationError::invalid())?;
  let completion: serde_json::Value =
    serde_json::from_slice(TRIAGE_COMPLETION_PRESET).map_err(|_| ApplicationError::invalid())?;
  let bytes = serde_json::to_vec(&serde_json::json!({"template":template,"completion":completion}))
    .map_err(|_| ApplicationError::invalid())?;
  let reference = ImmutableReference::new(
    "triage".parse().map_err(|_| ApplicationError::invalid())?,
    "v1".parse().map_err(|_| ApplicationError::invalid())?,
    FactoryDigest::content_sha256(&bytes),
  );
  Ok((template, reference))
}
