//! Immutable bundled release assets; no caller-supplied path or new registry.
use crate::*;
use octacity_server_factory::*;

macro_rules! asset {
  ($path:literal) => {
    include_bytes!(concat!("../../../../../../config/factory/", $path))
  };
}

// Compile only reviewed release assets. A manifest may not read operator-owned files.
macro_rules! bundled_parts {
  ($($path:literal),* $(,)?) => {
    fn bundled_part(path: &str) -> Option<&'static [u8]> {
      match path {
        $($path => Some(asset!($path)),)*
        _ => None,
      }
    }
  };
}

bundled_parts!(
  "presets/triage/inputs-v2.json",
  "presets/triage/inputs-v3.json",
  "presets/triage/eligibility-results-v3.json",
  "flows/base-flow/schemas/artifact_reference.json",
  "presets/triage/classification-results-v2.json",
  "flows/base-flow/schemas/work.json",
  "presets/triage/eligibility-v1.json",
  "presets/triage/eligibility-gate-v3.json",
  "presets/triage/classification-v2.json",
  "presets/triage/classification-gate-v2.json",
  "presets/triage/routing-v1.json",
  "presets/triage/direct-classification-v1.json",
  "presets/triage/selected-fields-v1.json",
  "presets/triage/assignments-v4.json",
  "flows/base-flow/schemas/declared_requirements.json",
  "flows/base-flow/schemas/summary.json",
  "flows/base-flow/schemas/spec_manifest.json",
  "flows/base-flow/schemas/spec_handoff.json",
  "flows/base-flow/schemas/spec_facts.json",
  "flows/base-flow/schemas/spec_result.json",
  "flows/base-flow/schemas/spec_author_input.json",
  "flows/base-flow/schemas/spec_gate_input.json",
  "flows/base-flow/nodes/specification_author.json",
  "flows/base-flow/nodes/specification_gate.json",
  "presets/ticket-delivery/assignments-v1.json",
  "presets/ticket-delivery/issue-summary-v2.json",
  "presets/ticket-delivery/requirements-branch-v2.json",
  "presets/ticket-delivery/requirements-approval-v2.json",
  "presets/ticket-delivery/requirements-branch-v1.json",
  "presets/ticket-delivery/requirements-approval-v1.json",
  "flows/base-flow/schemas/test_correction_input.json",
  "flows/base-flow/schemas/protected_author_input.json",
  "flows/base-flow/schemas/protected_test_handoff.json",
  "flows/base-flow/schemas/implementation_entry_input.json",
  "flows/base-flow/schemas/implementation_input.json",
  "flows/base-flow/schemas/execution_facts.json",
  "flows/base-flow/schemas/candidate_capture_facts.json",
  "flows/base-flow/schemas/candidate_subject.json",
  "flows/base-flow/schemas/code_review.json",
  "flows/base-flow/schemas/code_review_input.json",
  "flows/base-flow/schemas/spec_review_facts.json",
  "flows/base-flow/schemas/execution_gate_input.json",
  "flows/base-flow/schemas/protected_validation_input.json",
  "flows/base-flow/schemas/protected_validation_report.json",
  "flows/base-flow/schemas/protected_validation_facts.json",
  "flows/base-flow/schemas/protected_validation_gate_input.json",
  "flows/base-flow/schemas/validation_diagnostic_input.json",
  "flows/base-flow/schemas/validation_classification.json",
  "flows/base-flow/schemas/validation_classification_input.json",
  "flows/base-flow/nodes/protected_tests.json",
  "flows/base-flow/nodes/implementation_entry.json",
  "flows/base-flow/nodes/implementation.json",
  "flows/base-flow/nodes/implementation_rework_entry.json",
  "flows/base-flow/nodes/review.json",
  "flows/base-flow/nodes/protected_validation.json",
  "flows/base-flow/nodes/protected_validation_gate.json",
  "flows/base-flow/nodes/validation_classification.json",
  "flows/base-flow/policies/public-deliverables.json",
  "flows/base-flow/gate-programs.json",
  "presets/ticket-delivery/reproduction-branch-v1.json",
  "presets/ticket-delivery/predecessor-route-v1.json",
  "presets/ticket-delivery/issue-route-v1.json",
  "presets/ticket-delivery/protected-preparation-v1.json",
  "presets/ticket-delivery/implementation-capture-gate-v1.json",
  "presets/ticket-delivery/agent-result-gates-v1.json",
  "presets/ticket-delivery/development-v1.json",
  "presets/ticket-delivery/pull-request-v1.json",
);

pub(super) const TRIAGE: Release = Release {
  identity: "triage",
  version: "v4",
  manifest: asset!("presets/triage/v4.json"),
  completion: asset!("presets/triage/completion-v3.json"),
  name: None,
  issue_snapshot: false,
  parameters: Some(asset!("presets/triage/parameters-v1.json")),
};
pub(super) const TICKET_DELIVERY: Release = Release {
  identity: "ticket-delivery",
  version: "v1",
  manifest: asset!("presets/ticket-delivery/v1.json"),
  completion: asset!("presets/ticket-delivery/completion-v1.json"),
  name: Some("Ticket delivery"),
  issue_snapshot: false,
  parameters: Some(asset!("presets/ticket-delivery/setup-v1.json")),
};

const ISSUE_DELIVERY: Release = Release {
  identity: "ticket-delivery",
  version: "v2",
  manifest: asset!("presets/ticket-delivery/v2.json"),
  completion: asset!("presets/ticket-delivery/completion-v1.json"),
  name: Some("Ticket delivery"),
  issue_snapshot: true,
  parameters: Some(asset!("presets/ticket-delivery/setup-v1.json")),
};

#[derive(Clone, Copy)]
pub(super) struct Release {
  identity: &'static str,
  version: &'static str,
  manifest: &'static [u8],
  pub(super) completion: &'static [u8],
  pub(super) name: Option<&'static str>,
  pub(super) issue_snapshot: bool,
  parameters: Option<&'static [u8]>,
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Parameters {
  pub field_forms: std::collections::BTreeMap<FactoryKey, Vec<IssueFormField>>,
  pub publication: Option<FactoryKey>,
  pub optional_command: Option<OptionalCommand>,
  pub incoming: Option<Incoming>,
}
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OptionalCommand {
  pub alias: FactoryKey,
  pub nodes: std::collections::BTreeSet<FactoryKey>,
  pub terminal: FactoryKey,
  pub route: FlowTransition,
}
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Incoming {
  pub source_alias: FactoryKey,
  pub schema_alias: FactoryKey,
  pub inputs: std::collections::BTreeMap<FactoryKey, FlowDataMapping>,
  pub preconditions: std::collections::BTreeMap<FactoryKey, FlowPredicate>,
}

// Two explicit consumer input modes and one selected-field producer example.
pub(super) const RELEASES: [Release; 3] = [TICKET_DELIVERY, ISSUE_DELIVERY, TRIAGE];

impl Release {
  pub(super) fn cursor(self) -> FactoryKey {
    FactoryKey::new(format!("{}.{}", self.identity, self.version)).expect("bundled release cursor")
  }

  pub(super) fn load(self) -> Result<(FlowDefinitionTemplate, ImmutableReference), ApplicationError> {
    let template = FlowDefinitionTemplate::from_manifest(self.manifest, |path| {
      bundled_part(path.as_str())
        .map(<[u8]>::to_vec)
        .ok_or(FactoryError::InvalidConfiguration {
          field: "preset release part",
        })
    })
    .map_err(|_| ApplicationError::invalid())?;
    let completion: serde_json::Value =
      serde_json::from_slice(self.completion).map_err(|_| ApplicationError::invalid())?;
    let mut content = serde_json::json!({"template":template,"completion":completion});
    content["parameters"] = serde_json::to_value(self.parameters()?).map_err(|_| ApplicationError::invalid())?;
    if self.issue_snapshot {
      content["input"] = "issue_snapshot".into();
      content["snapshot_parameters"] = super::issue_delivery::content();
    }
    if let Some(name) = self.name {
      content["name"] = name.into();
    }
    let bytes = serde_json::to_vec(&content).map_err(|_| ApplicationError::invalid())?;
    let reference = ImmutableReference::new(
      self.identity.parse().map_err(|_| ApplicationError::invalid())?,
      self.version.parse().map_err(|_| ApplicationError::invalid())?,
      FactoryDigest::content_sha256(&bytes),
    );
    Ok((template, reference))
  }

  pub(super) fn matches(self, reference: &ImmutableReference) -> bool {
    reference.identity().as_str() == self.identity && reference.version().as_str() == self.version
  }

  pub(super) fn parameters(self) -> Result<Parameters, ApplicationError> {
    self
      .parameters
      .map(|bytes| serde_json::from_slice(bytes).map_err(|_| ApplicationError::invalid()))
      .transpose()
      .map(Option::unwrap_or_default)
  }

  #[cfg(test)]
  pub(super) fn predecessor_fixture(self) -> Result<FlowDefinitionTemplate, ApplicationError> {
    FlowDefinitionTemplate::from_manifest(asset!("presets/triage/v3.json"), |path| {
      Ok(bundled_part(path.as_str()).unwrap().to_vec())
    })
    .map_err(|_| ApplicationError::invalid())
  }
}

#[cfg(test)]
mod tests;
