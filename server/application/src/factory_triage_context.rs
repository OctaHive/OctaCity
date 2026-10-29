//! Reviewed triage selections compiled to the ordinary immutable Context contract.
use octacity_server_domain::ProjectId;
use octacity_server_factory::{
  ContextSourceKind, FactoryArtifactReference, FactoryKey, FlowContextBinding, FlowContextSelection, FlowDataMapping,
  FlowInputBinding, FlowValueMapping,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const SUPPORTING_DOCUMENT_PREFIX: &str = "document.";
/// Supporting aliases share the ordinary Context key bound after their technical namespace.
pub const MAX_TRIAGE_SUPPORTING_ALIAS_BYTES: usize =
  octacity_server_factory::MAX_FACTORY_KEY_BYTES - SUPPORTING_DOCUMENT_PREFIX.len();

/// Safe draft diagnostics; no private document text or installation details.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TriageContextProblem {
  /// Select an immutable Project goals document before compiling the Project stage.
  #[error("select the required Project goals document")]
  MissingProjectGoals,
  /// Select the controlling Project rules instead of inferring them from Role instructions.
  #[error("select the required Project rules document")]
  MissingProjectRules,
  /// Review the selection in the Project that owns this stage.
  #[error("select context from the same Project")]
  ForeignProject,
  /// The selected context cannot form a bounded same-Project binding.
  #[error("review the Project context selection and its bounds")]
  InvalidSelection,
}

/// Operator-reviewed immutable references, never inferred from a ticket or Role name.
/// Publication and dispatch retain their existing authorization and Artifact readers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriageProjectContextDraft {
  /// Owning Project; selecting the code Repository remains a separate configuration choice.
  pub project_id: ProjectId,
  /// Required goals; absence remains an actionable draft problem rather than invented context.
  pub goals: Option<FactoryArtifactReference>,
  /// Exact controlling Project rules document.
  pub rules: Option<FactoryArtifactReference>,
  /// Explicitly selected supporting documents under operator-owned aliases.
  pub documents: BTreeMap<FactoryKey, FactoryArtifactReference>,
}
impl TriageProjectContextDraft {
  /// Ordinary node input projection used with `bind`: exact admitted ticket and completion.
  /// The independently selected code Repository stays in the immutable Work/node subject.
  /// No ticket text, prior model output, Role name or ambient repository can retarget it.
  pub fn ticket_binding() -> FlowInputBinding {
    serde_json::from_slice(include_bytes!(
      "../../../config/factory/presets/triage/ticket-input-v1.json"
    ))
    .expect("the bundled triage ticket projection is a valid ordinary input binding")
  }
  /// Compiles exact selected references without I/O, publication or execution authority.
  pub fn bind(&self, project: ProjectId) -> Result<FlowContextBinding, TriageContextProblem> {
    let goals = self.goals.as_ref().ok_or(TriageContextProblem::MissingProjectGoals)?;
    if self.project_id != project {
      return Err(TriageContextProblem::ForeignProject);
    }
    let rules = self.rules.as_ref().ok_or(TriageContextProblem::MissingProjectRules)?;
    let mut entries = BTreeMap::new();
    for (name, kind, source) in [
      (
        "task",
        ContextSourceKind::Task,
        FlowValueMapping::Pointer { path: "/task".into() },
      ),
      (
        "completion",
        ContextSourceKind::Specification,
        FlowValueMapping::Pointer {
          path: "/completion".into(),
        },
      ),
      (
        "project-goals",
        ContextSourceKind::Specification,
        FlowValueMapping::Constant {
          value: serde_json::json!(goals),
        },
      ),
      (
        "project-rules",
        ContextSourceKind::Policy,
        FlowValueMapping::Constant {
          value: serde_json::json!(rules),
        },
      ),
    ] {
      entries.insert(
        FactoryKey::new(name).map_err(|_| TriageContextProblem::InvalidSelection)?,
        selection(kind, source)?,
      );
    }
    for (alias, document) in &self.documents {
      let alias = FactoryKey::new(format!("{SUPPORTING_DOCUMENT_PREFIX}{alias}"))
        .map_err(|_| TriageContextProblem::InvalidSelection)?;
      entries.insert(
        alias,
        selection(
          ContextSourceKind::Specification,
          FlowValueMapping::Constant {
            value: serde_json::json!(document),
          },
        )?,
      );
    }
    FlowContextBinding::new(None, entries).map_err(|_| TriageContextProblem::InvalidSelection)
  }
}
fn selection(
  source_kind: ContextSourceKind,
  source: FlowValueMapping,
) -> Result<FlowContextSelection, TriageContextProblem> {
  Ok(FlowContextSelection {
    when_present: None,
    source_kind,
    source: FlowDataMapping::new(FlowValueMapping::Object {
      fields: [
        (
          FactoryKey::new("kind").unwrap(),
          FlowValueMapping::Constant {
            value: serde_json::json!("artifact"),
          },
        ),
        (FactoryKey::new("value").unwrap(), source),
      ]
      .into(),
    })
    .map_err(|_| TriageContextProblem::InvalidSelection)?,
  })
}
