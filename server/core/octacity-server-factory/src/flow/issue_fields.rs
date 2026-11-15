//! Project-owned field identities selected by an ordinary agent outcome.
use crate::{FactoryKey, ImmutableReference};
use octacity_server_domain::{IssueId, IssueVersion, ProjectId};
use serde::{Deserialize, Serialize};

/// Writable scalar location; control, requirements and evidence keep their separate commands.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IssueFieldTarget {
  /// Project priority choice, independent of Work scheduling.
  Priority,
  /// Project component choice.
  Component,
  /// Project Issue type choice.
  Type,
  /// Ordered Project size choice.
  Size,
  /// Supported scalar Project property, without semantic-role registration.
  Property {
    /// Stable property identity within the selected Project.
    id: String,
  },
}

/// One field selected once for both the result form and guarded application.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueFormField {
  /// Canonical destination; display names and agent aliases cannot change it.
  pub field: IssueFieldTarget,
  /// Missing optional values preserve the current Issue; selection defaults to required.
  #[serde(default = "required")]
  pub required: bool,
}
fn required() -> bool {
  true
}

/// Exact reviewed Project selection. The catalog is the only source of value types and bounds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueFieldForm {
  /// Owning Project, preventing reuse of another Project's same-numbered publication.
  pub project_id: ProjectId,
  /// Exact immutable catalog, including the initial configured defaults at version zero.
  pub definition_version: u64,
  /// Bounded distinct fields; no output aliases, import paths or clear operations.
  pub fields: Vec<IssueFormField>,
}

/// Reviewed local writer and exact form; no result-to-field mapping is accepted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIssueFieldWrite {
  /// Installed trusted action identity, narrowed by the node's permissions.
  pub writer: ImmutableReference,
  /// Stable source ownership policy for guarded application.
  pub policy: FactoryKey,
  /// Single selection generating both the model's form and the allowed write set.
  pub form: IssueFieldForm,
}

/// Trusted Issue revision captured before the model invocation and retained with its input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowIssueFieldBaseline {
  /// Issue associated with this Work; never supplied by model output.
  pub issue_id: IssueId,
  /// Exact revision before analysis, independent of Work admission age.
  pub version: IssueVersion,
}
