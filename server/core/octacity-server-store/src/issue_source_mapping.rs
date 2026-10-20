//! Versioned synchronization policy; workflow and provider vocabulary remain separate.
use crate::*;
use octacity_server_domain::{EntityKind, IssueId, IssueSourceMappingVersion, IssueVersion, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, ImmutableReference, WorkSourceBinding, WorkSourceReportingPolicy};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Maximum encoded bytes in one closed synchronization definition, excluding its publication envelope.
pub const MAX_ISSUE_SOURCE_MAPPING_BYTES: usize = 65_536;
/// Fields supported by the existing normalized Source observation contract.
pub const ISSUE_SOURCE_PULL_FIELDS: &[IssueField] = &[IssueField::Title, IssueField::Description, IssueField::Status];
/// Fields supported by the existing finite Reporter effects contract.
pub const ISSUE_SOURCE_PUSH_FIELDS: &[IssueField] = &[IssueField::Status];

/// One authority and direction per managed field.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSourceFieldAuthority {
  /// Only reconciled authorized observations may replace this field.
  SourceOwnedPull,
  /// Authorized local changes may retain an explicitly enabled push intent.
  OctacityOwnedPush,
  /// Local-only data; observing native divergence grants no mutation.
  Unsynchronized,
}
/// Immutable link policy; native text never selects a workflow or execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceMappingDefinition {
  /// Exact installed read profile, independently of execution selectors.
  pub source: WorkSourceBinding,
  /// Separately installed writer; absent for pull-only mappings.
  pub reporter: Option<WorkSourceBinding>,
  /// Explicit outbound opt-in; publication itself dispatches no provider effect.
  pub push_enabled: bool,
  /// Absent fields are unsynchronized.
  pub fields: BTreeMap<IssueField, IssueSourceFieldAuthority>,
  /// Separate version-pinned translation, never a workflow definition.
  pub status: Option<IssueSourceStatusMapping>,
}
/// Status translation over installed normalized values and existing Reporter effects.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceStatusMapping {
  /// Exact applicable business vocabulary.
  pub workflow: IssueWorkflowPin,
  /// Literal native matches; multiple distinct targets remain ambiguous.
  pub pull: Vec<IssueSourceStatusRule>,
  /// Existing bounded effect contract keyed by stable internal status IDs.
  pub push: Option<octacity_server_factory::WorkSourceStatusMapping>,
}
/// One normalized predicate and an explicit internal status, with no implied path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceStatusRule {
  /// Existing bounded provider-neutral predicate.
  pub native: octacity_server_factory::WorkSourcePredicate<String>,
  /// Stable status in the separately pinned workflow.
  pub status_id: octacity_server_domain::IssueStatusId,
}
/// One original immutable publication; replacement never changes this record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublishedIssueSourceMapping {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact retained primary link owner.
  pub issue_id: IssueId,
  /// Independent mapping version, unrelated to Issue/workflow revision numbers.
  pub version: IssueSourceMappingVersion,
  /// Reviewed ownership and translation.
  pub definition: IssueSourceMappingDefinition,
  /// Original acceptance time.
  pub published_at: Timestamp,
}
impl PublishedIssueSourceMapping {
  /// Exact content pin reused by ordinary field policy and retained push intents.
  pub fn reference(&self) -> ImmutableReference {
    ImmutableReference::new(
      format!("issue-source-mapping.{}", self.issue_id)
        .parse()
        .expect("bounded Issue identity"),
      format!("v{}", self.version.get())
        .parse()
        .expect("bounded mapping version"),
      FactoryDigest::content_sha256(&serde_json::to_vec(&self.definition).expect("closed mapping document")),
    )
  }
}
/// Frozen ownership review, with separate Issue and mapping preconditions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishIssueSourceMappingIntent {
  /// Visible owning Project.
  pub project_id: ProjectId,
  /// Visible source-linked task.
  pub issue_id: IssueId,
  /// Exact reviewed requirements/status head.
  pub expected_issue_version: IssueVersion,
  /// Original mapping pin, or absence for the first publication.
  pub expected_mapping: Option<ImmutableReference>,
  /// Bounded desired policy.
  pub definition: IssueSourceMappingDefinition,
}
/// Trusted application resolution; declarations are not wire authority grants.
#[derive(Clone)]
pub struct PublishIssueSourceMapping {
  /// Trusted installed writer origin, never supplied by a management body.
  pub writer_access: Option<crate::IssueSourceWriterAccess>,
  /// Reviewed intent, excluding mutable capability material.
  pub intent: PublishIssueSourceMappingIntent,
  /// Installed native container, independently verified from profile metadata.
  pub native_resource: octacity_server_factory::ExternalWorkIdentity,
  /// Installed finite read ceiling, never inferred from observed values.
  pub readable: WorkSourceReportingPolicy,
  /// Independently authorized installed write ceiling.
  pub writable: Option<WorkSourceReportingPolicy>,
  /// Scoped original operation.
  pub idempotency_key: IdempotencyKey,
  /// Trusted acceptance time.
  pub published_at: Timestamp,
}
/// Original exact mapping receipt; no provider dispatch or Issue revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueSourceMappingOutcome {
  /// New reviewed resolution effect, absent for ordinary policy publication.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub projection: Option<FactoryDigest>,
  /// Applied or replayed publication.
  pub disposition: MutationDisposition,
  /// Retained original mapping.
  pub mapping: PublishedIssueSourceMapping,
}
/// Optional mapping authority; manual Issue handlers have no dependency on it.
#[async_trait::async_trait]
pub trait IssueSourceMappingStore: IssueSourceRefreshStore + IssueWorkflowStore {
  /// Reads an exact retained version or current mapping under original link visibility.
  async fn issue_source_mapping(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    version: Option<IssueSourceMappingVersion>,
  ) -> Result<Option<PublishedIssueSourceMapping>, StoreError>;
  /// Probes original replay before resolving current installed capabilities; reserves nothing.
  async fn issue_source_mapping_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &PublishIssueSourceMappingIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceMappingOutcome>, StoreError>;
  /// Atomically retains an immutable mapping and current ownership with shared replay/audit.
  async fn publish_issue_source_mapping(
    &self,
    request: ManagementMutation<PublishIssueSourceMapping>,
  ) -> Result<IssueSourceMappingOutcome, StoreError>;
}
pub(crate) fn mapping_conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}
pub(crate) fn invalid_source_mapping() -> StoreError {
  StoreError::invalid(StoreOperation::PublishIssueSourceMapping, StoreInputError::InvalidIssue)
}
impl PublishIssueSourceMapping {
  /// Rechecks reviewed correspondence inside either adapter's authoritative transaction.
  pub fn resolve(
    &self,
    current: &IssueRevision,
    link: &IssueSourceLink,
    previous: Option<&PublishedIssueSourceMapping>,
    workflow: &PublishedIssueWorkflow,
  ) -> Result<PublishedIssueSourceMapping, StoreError> {
    let i = &self.intent;
    if current.project_id != i.project_id
      || current.id != i.issue_id
      || current.version != i.expected_issue_version
      || link.issue_id != current.id
      || link.selection.project_id != current.project_id
      || !link.supports_profile(&i.definition.source, &self.native_resource)
      || previous.map(PublishedIssueSourceMapping::reference) != i.expected_mapping
      || previous.is_some_and(|p| self.published_at < p.published_at)
      || self.published_at < current.updated_at
    {
      return Err(mapping_conflict());
    }
    self.readable.validate().map_err(|_| invalid_source_mapping())?;
    if serde_json::to_vec(&i.definition)
      .map_err(|_| invalid_source_mapping())?
      .len()
      > MAX_ISSUE_SOURCE_MAPPING_BYTES
    {
      return Err(invalid_source_mapping());
    }
    let has_push = i
      .definition
      .fields
      .values()
      .any(|a| *a == IssueSourceFieldAuthority::OctacityOwnedPush);
    if (has_push && (i.definition.reporter.is_none() || self.writable.is_none()))
      || (!has_push && (i.definition.reporter.is_some() || i.definition.push_enabled))
      || i
        .definition
        .reporter
        .as_ref()
        .is_some_and(|r| r.installation.identity() != i.definition.source.installation.identity())
      || i.definition.fields.iter().any(|(f, a)| match a {
        IssueSourceFieldAuthority::Unsynchronized => false,
        IssueSourceFieldAuthority::SourceOwnedPull => !ISSUE_SOURCE_PULL_FIELDS.contains(f),
        IssueSourceFieldAuthority::OctacityOwnedPush => !ISSUE_SOURCE_PUSH_FIELDS.contains(f),
      })
    {
      return Err(invalid_source_mapping());
    }
    if let Some(status) = &i.definition.status {
      let authority = i.definition.fields.get(&IssueField::Status);
      if !matches!(
        authority,
        Some(IssueSourceFieldAuthority::SourceOwnedPull | IssueSourceFieldAuthority::OctacityOwnedPush)
      ) || status.workflow
        != (IssueWorkflowPin {
          id: current.workflow_id,
          version: current.workflow_version,
        })
        || workflow.project_id != current.project_id
        || workflow.id != current.workflow_id
        || workflow.version != current.workflow_version
        || status.pull.len() > octacity_server_factory::MAX_SOURCE_REPORTING_STATUSES
        || (authority == Some(&IssueSourceFieldAuthority::SourceOwnedPull) && status.push.is_some())
        || (authority == Some(&IssueSourceFieldAuthority::OctacityOwnedPush)
          && (!status.pull.is_empty() || status.push.is_none()))
      {
        return Err(invalid_source_mapping());
      }
      if let Some(push) = &status.push {
        self
          .writable
          .as_ref()
          .ok_or_else(invalid_source_mapping)?
          .validate_mapping(push)
          .map_err(|_| invalid_source_mapping())?;
        for row in &push.statuses {
          let id: octacity_server_domain::IssueStatusId = row.status.parse().map_err(|_| invalid_source_mapping())?;
          if !workflow.definition.statuses.iter().any(|s| s.id == id) {
            return Err(invalid_source_mapping());
          }
        }
      }
      for rule in &status.pull {
        rule.native.validate().map_err(|_| invalid_source_mapping())?;
        if !workflow.definition.statuses.iter().any(|s| s.id == rule.status_id)
          || rule
            .native
            .state
            .as_ref()
            .is_some_and(|s| !self.readable.states.iter().any(|v| v == s.as_str()))
          || rule
            .native
            .all_of
            .iter()
            .chain(&rule.native.any_of)
            .chain(&rule.native.none_of)
            .any(|l| !self.readable.labels.contains(l))
        {
          return Err(invalid_source_mapping());
        }
      }
    } else if i
      .definition
      .fields
      .get(&IssueField::Status)
      .is_some_and(|a| *a != IssueSourceFieldAuthority::Unsynchronized)
    {
      return Err(invalid_source_mapping());
    }
    Ok(PublishedIssueSourceMapping {
      project_id: current.project_id,
      issue_id: current.id,
      version: previous
        .map_or(Ok(IssueSourceMappingVersion::INITIAL), |p| p.version.next())
        .map_err(|_| mapping_conflict())?,
      definition: i.definition.clone(),
      published_at: self.published_at,
    })
  }
}
impl PublishedIssueSourceMapping {
  /// Derived existing policy; ownership has one authoritative mapping document.
  pub fn field_policy(&self) -> IssueFieldPolicy {
    IssueFieldPolicy {
      source_owned: self
        .definition
        .fields
        .iter()
        .filter_map(|(f, a)| (*a == IssueSourceFieldAuthority::SourceOwnedPull).then_some(*f))
        .collect(),
      status_mapping_workflow: self.definition.status.as_ref().map(|s| s.workflow.clone()),
      push: self.definition.push_enabled.then(|| IssuePushPolicy {
        mapping: self.reference(),
        fields: self
          .definition
          .fields
          .iter()
          .filter_map(|(f, a)| (*a == IssueSourceFieldAuthority::OctacityOwnedPush).then_some(*f))
          .collect(),
      }),
    }
  }
}
