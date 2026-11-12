//! Producer-independent Issue input selections. These declarations grant no content or execution authority.
use crate::{FactoryKey, ImmutableReference};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Versioned selection feeding the existing immutable Incoming input channel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIssueSnapshotInput {
  /// Supported contract version, independent of a producer Flow.
  pub version: u16,
  /// Consumer data schema; Project types must fit without copying choice vocabularies.
  pub schema: ImmutableReference,
  /// Exact Project definition publication, including zero for the default catalog.
  pub definition_version: u64,
  /// Optional exact scheduling/readiness mapping, pinned by this Flow publication.
  /// Absent on earlier contracts retaining explicit operator classification.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub admission: Option<FlowIssueAdmissionPolicy>,
  /// Named field projections; keys address top-level fields in the Incoming schema.
  pub fields: BTreeMap<FactoryKey, FlowIssueFieldSelection>,
  /// Explicit text selections, empty unless the author opts in.
  #[serde(default)]
  pub materials: BTreeMap<FactoryKey, FlowIssueMaterialSelection>,
  /// Safe evidence identities for selected fields; never private report bodies.
  #[serde(default)]
  pub evidence: BTreeMap<FactoryKey, FlowIssueEvidenceSelection>,
}
impl FlowIssueSnapshotInput {
  pub(crate) fn validate_structure(&self, flow: &crate::FactoryFlowConfiguration) -> Result<(), crate::FactoryError> {
    let invalid = |field| crate::FactoryError::InvalidConfiguration { field };
    if self.version != 1 {
      return Err(invalid("issue_snapshot/version"));
    }
    if flow.accepted_predecessor.is_some() {
      return Err(invalid("issue_snapshot"));
    }
    let count = self.fields.len() + self.materials.len() + self.evidence.len();
    if count == 0 {
      return Err(invalid("issue_snapshot"));
    }
    if count > crate::MAX_FLOW_SCHEMA_ENTRIES
      || serde_json::to_vec(self).map_err(|_| invalid("issue_snapshot"))?.len() > crate::MAX_FLOW_INPUT_BINDING_BYTES
    {
      return Err(invalid("issue_snapshot/fields"));
    }
    let mut aliases = std::collections::BTreeSet::new();
    if self
      .fields
      .keys()
      .chain(self.materials.keys())
      .chain(self.evidence.keys())
      .any(|alias| !aliases.insert(alias))
    {
      return Err(invalid("issue_snapshot/fields"));
    }
    let mut incoming = flow.incoming_schemas().peekable();
    if incoming.peek().is_none()
      || incoming.any(|schema| schema != &self.schema)
      || !flow
        .data_schemas
        .iter()
        .any(|schema| schema.reference() == &self.schema)
    {
      return Err(invalid("issue_snapshot/schema"));
    }
    Ok(())
  }
}
/// One optional/required canonical Issue value or typed dependency collection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIssueFieldSelection {
  /// Built-in location, stable property ID or explicit classification binding.
  pub source: FlowIssueFieldSource,
  /// Missing values block preparation when true; otherwise omission remains explicit.
  pub required: bool,
}
/// Canonical field selection, independent of display labels and producing executions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowIssueFieldSource {
  /// Current requirements title.
  Title,
  /// Current requirements description.
  Description,
  /// Explicit implementation acceptance.
  Acceptance,
  /// Built-in Project priority choice; legacy numbers require a separate supported mapping.
  Priority,
  /// Built-in Project component choice.
  Component,
  /// Built-in Project type choice.
  Type,
  /// Fixed built-in size.
  Size,
  /// Existing bounded exact Work/Run dependencies; never a new scheduling graph.
  Dependencies,
  /// Ordinary supported Project property; no classification semantics required.
  Property {
    /// Stable ID in the selected Project catalog.
    id: String,
  },
  /// A semantic slot resolved under the exact selected Project catalog.
  Classification {
    /// Stable supported binding name, never an inferred display label.
    slot: FactoryKey,
  },
}
/// Explicit bounded text consumption; ownership is resolved against the execution's Issue/Project.
/// Its Incoming field contains an ordered text array; exact source lineage stays in the retained snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIssueMaterialSelection {
  /// Exact sources or the Issue's explicitly approved material selections.
  pub source: FlowIssueMaterialSource,
  /// An unavailable required selection blocks preparation.
  pub required: bool,
  /// Inclusive aggregate UTF-8 byte ceiling for this selection.
  pub max_bytes: usize,
}
/// No whole-thread, latest-document or implicit attachment lookup is supported.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowIssueMaterialSource {
  /// Only the exact named sources, checked against the Issue's approved selections.
  Exact {
    /// Immutable comment/document selectors; bodies remain with their owners.
    references: Vec<FlowIssueMaterialReference>,
  },
  /// An explicit opt-in to a bounded set already selected on the Issue.
  Selected {
    /// Inclusive source count ceiling; the Source must explicitly target this published material contract.
    max_entries: u16,
  },
}
/// Exact selector scoped to the execution's canonical Issue and Project, without embedded bodies.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowIssueMaterialReference {
  /// Same-Issue append-only comment and original body digest.
  Comment {
    /// Exact comment identity.
    comment_id: octacity_server_domain::IssueCommentId,
    /// Original UTF-8 content digest.
    body_digest: crate::FactoryDigest,
  },
  /// Same-Issue-owned immutable publication; not a Project catalog entry.
  IssueDocument {
    /// Exact artifact identity, digest and size.
    artifact: crate::FactoryArtifactReference,
  },
  /// Explicitly linked same-Project publication; ownership stays with the Project.
  ProjectDocument {
    /// Exact artifact identity, digest and size.
    artifact: crate::FactoryArtifactReference,
  },
}
/// Evidence record digests associated with one selected field, independently qualified at admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIssueEvidenceSelection {
  /// Alias of a declared scalar field, not an arbitrary producing Run.
  pub field: FactoryKey,
  /// Required evidence must be currently accessible and applicable.
  pub required: bool,
  /// Inclusive reference count ceiling; no report bytes are projected.
  pub maximum: u16,
}

/// Deterministic admission rules over selected canonical classification, never model output.
/// Missing mappings deny admission; there is no inferred numeric priority or risk default.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIssueAdmissionPolicy {
  /// Explicit translation of Project priority choice IDs to Work scheduling priorities.
  pub priority: FlowIssueAdmissionMapping<crate::WorkPriority>,
  /// Explicit translation of an assessed classification field to admission risk.
  pub risk: FlowIssueAdmissionMapping<crate::RiskClass>,
  /// Additional exact choice allowlists; all must match after applicability/evidence checks.
  #[serde(default)]
  pub allowed: BTreeMap<FactoryKey, std::collections::BTreeSet<String>>,
}
/// A finite mapping under a declared field alias; vocabularies remain Project-owned.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIssueAdmissionMapping<T> {
  /// Required selected classification field, not a path into arbitrary metadata.
  pub field: FactoryKey,
  /// Explicit supported choices only; unmapped choices block admission.
  pub values: BTreeMap<String, T>,
}
impl FlowIssueAdmissionPolicy {
  /// Maps already qualified selected data. This neither grants execution authority
  /// nor replaces the independent launch, access, evidence and code-subject checks.
  pub fn classify(
    &self,
    values: &BTreeMap<FactoryKey, serde_json::Value>,
  ) -> Result<crate::WorkClassification, FactoryKey> {
    for (field, allowed) in &self.allowed {
      if !values
        .get(field)
        .and_then(serde_json::Value::as_str)
        .is_some_and(|v| allowed.contains(v))
      {
        return Err(field.clone());
      }
    }
    let priority = mapped(&self.priority, values)?;
    let risk = mapped(&self.risk, values)?;
    Ok(crate::WorkClassification::new(
      *priority,
      *risk,
      crate::FactoryMetadata::default(),
    ))
  }
}
fn mapped<'a, T>(
  mapping: &'a FlowIssueAdmissionMapping<T>,
  values: &BTreeMap<FactoryKey, serde_json::Value>,
) -> Result<&'a T, FactoryKey> {
  values
    .get(&mapping.field)
    .and_then(serde_json::Value::as_str)
    .and_then(|v| mapping.values.get(v))
    .ok_or_else(|| mapping.field.clone())
}
