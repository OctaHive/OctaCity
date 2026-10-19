//! Bounded organizational metadata; values never grant execution or source authority.
use crate::*;
use octacity_server_domain::{IssueId, ProjectId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Optional planning snapshot retained with an immutable Issue revision.
#[derive(Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePlanning {
  /// Exact Project definition publication; zero is the empty declarative catalog.
  pub definition_version: u64,
  /// Server-resolved organizational identity, independent of actor or worker.
  pub assignee: Option<String>,
  /// Stable Project label identities.
  #[serde(deserialize_with = "unique_labels")]
  pub labels: BTreeSet<String>,
  /// Date-only planning value in YYYY-MM-DD form.
  pub due_date: Option<String>,
  /// Configured type/component/size and simple custom values by stable property key.
  pub values: BTreeMap<String, IssuePropertyValue>,
  /// Optional same-Project organizational parent, never an execution dependency.
  pub parent_id: Option<IssueId>,
}
// Preserve the public set contract at deserialization: do not silently collapse
// an oversized or duplicate wire list before shared policy can inspect it.
fn unique_labels<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<BTreeSet<String>, D::Error> {
  let labels = Vec::<String>::deserialize(deserializer)?;
  if labels.len() > MAX_ISSUE_LABELS {
    return Err(serde::de::Error::custom("too many planning labels"));
  }
  let count = labels.len();
  let labels: BTreeSet<_> = labels.into_iter().collect();
  if labels.len() != count {
    return Err(serde::de::Error::custom("duplicate planning labels"));
  }
  Ok(labels)
}
impl std::fmt::Debug for IssuePlanning {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IssuePlanning").finish_non_exhaustive()
  }
}
/// Closed simple value grammar; objects, scripts and arbitrary JSON are unsupported.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum IssuePropertyValue {
  /// Bounded plain text, never executable.
  Text(String),
  /// Stable identity from a finite declared choice set.
  Choice(String),
  /// Boolean organizational fact.
  Boolean(bool),
  /// Bounded integer planning quantity.
  Number(i64),
  /// Date-only YYYY-MM-DD value.
  Date(String),
}
impl std::fmt::Debug for IssuePropertyValue {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("IssuePropertyValue { .. }")
  }
}
/// Stable label/choice identity with an editable display name and explicit retirement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePropertyChoice {
  /// Stable Project-owned key.
  pub id: String,
  /// Human-readable display name.
  pub name: String,
  /// Retained for history; unavailable for new selections.
  pub retired: bool,
}
/// Supported finite property types; no formulas or nested values.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum IssuePropertyKind {
  /// Plain text bounded by UTF-8 bytes.
  Text {
    /// Inclusive maximum UTF-8 bytes.
    max_bytes: u16,
  },
  /// One stable choice identity.
  Choice {
    /// Stable finite choices, including retained retired entries.
    choices: Vec<IssuePropertyChoice>,
  },
  /// Boolean value.
  Boolean,
  /// Integer with inclusive finite limits.
  Number {
    /// Inclusive lower bound.
    minimum: i64,
    /// Inclusive upper bound.
    maximum: i64,
  },
  /// Valid calendar date without time or timezone.
  Date,
}
/// One Project-defined planning field, including optional type/component/size.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePropertyDefinition {
  /// Stable non-reserved property key.
  pub id: String,
  /// Display label, with no workflow semantics.
  pub name: String,
  /// Explicit supported type and bounds.
  pub kind: IssuePropertyKind,
  /// Retains a field while preventing new values.
  pub retired: bool,
}
/// One immutable Project catalog; an empty catalog needs no administrator setup.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePropertyDefinitionSet {
  /// Bounded Project labels.
  pub labels: Vec<IssuePropertyChoice>,
  /// Bounded optional planning definitions.
  pub properties: Vec<IssuePropertyDefinition>,
}
/// Retained exact catalog publication used to explain old Issue revisions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublishedIssueProperties {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact immutable publication; zero denotes the empty template.
  pub version: u64,
  /// Original declarative definitions.
  pub definition: IssuePropertyDefinitionSet,
  /// Original server publication time.
  pub published_at: Timestamp,
}
/// Narrow persistence boundary for retained Project planning definitions.
#[async_trait::async_trait]
pub trait IssuePropertyStore: Send + Sync {
  /// Filters same-Project visible direct children before pagination.
  async fn issue_children(&self, request: ListProjectIssues, parent_id: IssueId) -> Result<IssuePage, StoreError>;
  /// Atomically replaces planning under the current task and definition versions.
  async fn edit_issue_planning(
    &self,
    request: ManagementMutation<EditIssuePlanning>,
  ) -> Result<IssueMutationOutcome, StoreError>;
  /// Publishes one preconditioned immutable catalog with original replay.
  async fn publish_issue_properties(
    &self,
    request: ManagementMutation<PublishIssueProperties>,
  ) -> Result<IssuePropertiesOutcome, StoreError>;
  /// Reads the current or retained catalog without initializing state.
  async fn issue_properties(
    &self,
    project_id: ProjectId,
    version: Option<u64>,
  ) -> Result<PublishedIssueProperties, StoreError>;
}

/// Maximum configured planning fields per Project.
pub const MAX_ISSUE_PROPERTIES: usize = 32;
/// Maximum bytes in a stable ASCII property or choice key.
pub const MAX_ISSUE_PROPERTY_KEY_BYTES: usize = 64;
/// Maximum labels or choices per catalog list.
pub const MAX_ISSUE_PROPERTY_CHOICES: usize = 64;
/// Maximum bytes of a plain-text custom value.
pub const MAX_ISSUE_PROPERTY_TEXT_BYTES: u16 = 4096;
/// Maximum labels selected on one task.
pub const MAX_ISSUE_LABELS: usize = 32;
/// Largest magnitude supported by numeric planning fields; safe in generated clients.
pub const MAX_ISSUE_PROPERTY_NUMBER: i64 = 9_007_199_254_740_991;
/// Frozen Project catalog replacement; administration is independent of task editing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishIssuePropertiesIntent {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Current catalog version; zero publishes the first definition.
  pub expected_version: u64,
  /// Proposed bounded immutable definitions.
  pub definition: IssuePropertyDefinitionSet,
}
/// Trusted publication envelope; actor attribution comes from management context.
#[derive(Clone, Debug)]
pub struct PublishIssueProperties {
  /// Original reviewed replacement.
  pub intent: PublishIssuePropertiesIntent,
  /// Scoped replay key.
  pub idempotency_key: IdempotencyKey,
  /// Original server publication time.
  pub published_at: Timestamp,
}
/// Exact original publication receipt, retained across replacements.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssuePropertiesOutcome {
  /// Applied versus exact replay.
  pub disposition: MutationDisposition,
  /// Original immutable definitions.
  pub resource: PublishedIssueProperties,
}
fn invalid() -> StoreError {
  crate::issue::invalid_issue(StoreOperation::PublishIssueProperties)
}
pub(crate) fn planning_conflict() -> StoreError {
  StoreError::Conflict {
    entity: octacity_server_domain::EntityKind::Issue,
  }
}
fn valid_key(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= MAX_ISSUE_PROPERTY_KEY_BYTES
    && value
      .bytes()
      .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn valid_name(value: &str) -> bool {
  !value.is_empty()
    && value.trim() == value
    && value.len() <= MAX_ISSUE_TITLE_BYTES
    && !value.chars().any(char::is_control)
}
fn choices_valid(choices: &[IssuePropertyChoice]) -> bool {
  choices.len() <= MAX_ISSUE_PROPERTY_CHOICES
    && choices.iter().all(|c| valid_key(&c.id) && valid_name(&c.name))
    && choices.iter().map(|c| &c.id).collect::<BTreeSet<_>>().len() == choices.len()
}
impl IssuePropertyDefinitionSet {
  /// Checks finite supported types and rejects reserved state/authority fields.
  pub fn validate(&self) -> Result<(), StoreError> {
    const RESERVED: &[&str] = &[
      "id",
      "project_id",
      "title",
      "description",
      "acceptance",
      "priority",
      "status",
      "status_id",
      "workflow",
      "workflow_id",
      "workflow_version",
      "run",
      "run_id",
      "run_state",
      "work",
      "work_id",
      "execution",
      "execution_state",
      "decision",
      "accepted",
      "approved",
      "permissions",
      "source_owned",
      "assignee",
      "parent",
      "parent_id",
      "due_date",
      "labels",
      "agent",
      "worker",
      "profile",
      "created_at",
      "updated_at",
    ];
    if self.properties.len() > MAX_ISSUE_PROPERTIES
      || !choices_valid(&self.labels)
      || self.properties.iter().map(|p| &p.id).collect::<BTreeSet<_>>().len() != self.properties.len()
      || serde_json::to_vec(self).map_err(|_| invalid())?.len() > MAX_ISSUE_DOCUMENT_BYTES
      || self.properties.iter().any(|p| {
        !valid_key(&p.id)
          || RESERVED.contains(&p.id.as_str())
          || !valid_name(&p.name)
          || match &p.kind {
            IssuePropertyKind::Text { max_bytes } => *max_bytes == 0 || *max_bytes > MAX_ISSUE_PROPERTY_TEXT_BYTES,
            IssuePropertyKind::Choice { choices } => choices.is_empty() || !choices_valid(choices),
            IssuePropertyKind::Number { minimum, maximum } => {
              minimum > maximum || *minimum < -MAX_ISSUE_PROPERTY_NUMBER || *maximum > MAX_ISSUE_PROPERTY_NUMBER
            }
            _ => false,
          }
      })
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Stable keys/types are retained; retirement replaces removal from the catalog.
  pub fn validate_replacement(&self, previous: &Self) -> Result<(), StoreError> {
    self.validate()?;
    let retains =
      |old: &[IssuePropertyChoice], new: &[IssuePropertyChoice]| old.iter().all(|o| new.iter().any(|n| n.id == o.id));
    if !retains(&previous.labels, &self.labels)
      || previous.properties.iter().any(|old| {
        self.properties.iter().find(|p| p.id == old.id).is_none_or(|new| {
          std::mem::discriminant(&old.kind) != std::mem::discriminant(&new.kind)
            || match (&old.kind, &new.kind) {
              (IssuePropertyKind::Choice { choices: a }, IssuePropertyKind::Choice { choices: b }) => !retains(a, b),
              _ => false,
            }
        })
      })
    {
      return Err(planning_conflict());
    }
    Ok(())
  }
}

/// Preconditioned organizational replacement; cannot set requirements or business status.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditIssuePlanningIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Stable task identity.
  pub id: IssueId,
  /// Exact reviewed task revision.
  pub expected_version: octacity_server_domain::IssueVersion,
  /// Complete reviewed optional planning snapshot and definition version.
  pub planning: IssuePlanning,
}
/// Trusted planning mutation envelope.
#[derive(Clone, Debug)]
pub struct EditIssuePlanning {
  /// Current authorized directory identities; never caller-supplied intent.
  pub assignees: BTreeSet<String>,
  /// Frozen original organizational intent.
  pub intent: EditIssuePlanningIntent,
  /// Scoped original replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Server mutation time.
  pub edited_at: Timestamp,
}
impl IssuePlanning {
  /// True for an ordinary task requiring no planning catalog setup.
  pub fn is_empty(&self) -> bool {
    self == &Self::default()
  }
  /// Validates the reviewed current catalog and unchanged retained retired values.
  pub fn validate(
    &self,
    catalog: &PublishedIssueProperties,
    previous: Option<&Self>,
    assignees: &BTreeSet<String>,
  ) -> Result<(), StoreError> {
    let invalid = || {
      crate::issue::invalid_issue(if previous.is_some() {
        StoreOperation::EditIssuePlanning
      } else {
        StoreOperation::CreateIssue
      })
    };
    if previous.is_none() && self.is_empty() {
      return Ok(());
    }
    if self.definition_version != catalog.version {
      return Err(planning_conflict());
    }
    if self.assignee.as_ref().is_some_and(|id| !assignees.contains(id))
      && previous.is_none_or(|p| p.assignee != self.assignee)
    {
      return Err(planning_conflict());
    }
    if self.labels.len() > MAX_ISSUE_LABELS
      || self.values.len() > MAX_ISSUE_PROPERTIES
      || self.due_date.as_deref().is_some_and(|s| !valid_date(s))
    {
      return Err(invalid());
    }
    for id in &self.labels {
      if !catalog
        .definition
        .labels
        .iter()
        .any(|c| c.id == *id && (!c.retired || previous.is_some_and(|p| p.labels.contains(id))))
      {
        return Err(invalid());
      }
    }
    for (id, value) in &self.values {
      let property = catalog
        .definition
        .properties
        .iter()
        .find(|p| p.id == *id)
        .ok_or_else(invalid)?;
      if previous.is_some_and(|p| p.values.get(id) == Some(value)) {
        continue;
      }
      if property.retired {
        return Err(invalid());
      }
      let valid = match (&property.kind, value) {
        (IssuePropertyKind::Text { max_bytes }, IssuePropertyValue::Text(s)) => {
          s.len() <= usize::from(*max_bytes) && !s.contains('\0')
        }
        (IssuePropertyKind::Choice { choices }, IssuePropertyValue::Choice(id)) => {
          choices.iter().any(|c| c.id == *id && !c.retired)
        }
        (IssuePropertyKind::Number { minimum, maximum }, IssuePropertyValue::Number(n)) => {
          (*minimum..=*maximum).contains(n)
        }
        (IssuePropertyKind::Boolean, IssuePropertyValue::Boolean(_)) => true,
        (IssuePropertyKind::Date, IssuePropertyValue::Date(s)) => valid_date(s),
        _ => false,
      };
      if !valid {
        return Err(invalid());
      }
    }
    Ok(())
  }
}
fn valid_date(s: &str) -> bool {
  let bytes = s.as_bytes();
  if bytes.len() != 10
    || bytes[4] != b'-'
    || bytes[7] != b'-'
    || bytes
      .iter()
      .enumerate()
      .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
  {
    return false;
  }
  let Ok(year) = s[..4].parse::<u16>() else {
    return false;
  };
  let Ok(month) = s[5..7].parse::<u8>() else {
    return false;
  };
  let Ok(day) = s[8..].parse::<u8>() else {
    return false;
  };
  let days = match month {
    1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
    4 | 6 | 9 | 11 => 30,
    2 => {
      if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
        29
      } else {
        28
      }
    }
    _ => 0,
  };
  year > 0 && day > 0 && day <= days
}
impl IssueRevision {
  /// Produces a planning-only revision without invalidating requirements or status.
  pub fn planned(
    &self,
    request: &EditIssuePlanning,
    policy: &IssueFieldPolicy,
    catalog: &PublishedIssueProperties,
  ) -> Result<Self, StoreError> {
    let invalid = || crate::issue::invalid_issue(StoreOperation::EditIssuePlanning);
    let i = &request.intent;
    if self.id != i.id
      || self.project_id != i.project_id
      || self.version != i.expected_version
      || catalog.project_id != self.project_id
    {
      return Err(planning_conflict());
    }
    i.planning.validate(catalog, Some(&self.planning), &request.assignees)?;
    if request.edited_at < self.updated_at {
      return Err(invalid());
    }
    let changed = [
      (IssueField::Assignee, self.planning.assignee != i.planning.assignee),
      (IssueField::Labels, self.planning.labels != i.planning.labels),
      (IssueField::DueDate, self.planning.due_date != i.planning.due_date),
      (IssueField::Properties, self.planning.values != i.planning.values),
      (IssueField::Parent, self.planning.parent_id != i.planning.parent_id),
    ];
    if changed
      .iter()
      .any(|(field, changed)| *changed && policy.source_owned.contains(field))
    {
      return Err(planning_conflict());
    }
    Ok(Self {
      version: self.version.next().map_err(|_| invalid())?,
      planning: i.planning.clone(),
      updated_at: request.edited_at,
      ..self.clone()
    })
  }
}
