//! Reviewed import activation is metadata, separate from Source progress, Issue pull and execution.
use crate::*;
pub use octacity_server_domain::IssueSourceImportId;
use octacity_server_domain::{EntityKind, IntegrationVersion, ProjectId, Timestamp};
use octacity_server_factory::{WorkSourceBinding, WorkSourceConnectionRef, WorkSourcePredicate};
use serde::{Deserialize, Serialize};

/// Maximum metadata bytes in one import configuration; raw task content is never retained here.
pub const MAX_ISSUE_SOURCE_IMPORT_BYTES: usize = 64 * 1024;
/// Explicit native reader and optional narrowing, without a Repository, Factory target or writer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceImportDefinition {
  /// Exact Project/connection-bound installed read profile.
  pub source: WorkSourceBinding,
  /// Original metadata/native parent and reviewed Project grant revision.
  pub connection: WorkSourceConnectionRef,
  /// Reviewed normalized import eligibility; retained-link refresh is independent.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub predicate: Option<WorkSourcePredicate>,
  /// Explicit import opt-in; it grants no execution or synchronization authority.
  pub enabled: bool,
}
impl IssueSourceImportDefinition {
  /// Rejects invalid read-only scope and unbounded selectors before publication.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.connection.version.get() > i64::MAX as u64
      || self.connection.access_version.get() > i64::MAX as u64
      || self.connection.reporter_configuration.is_some()
      || self.predicate.as_ref().is_some_and(|p| p.validate().is_err())
      || serde_json::to_vec(self).map_err(|_| StoreError::Unavailable)?.len() > MAX_ISSUE_SOURCE_IMPORT_BYTES
    {
      return Err(StoreError::invalid(
        StoreOperation::PublishIssueSourceImport,
        StoreInputError::InvalidIssueSourceImport,
      ));
    }
    Ok(())
  }
}
/// Immutable publication intent; exact replay does not depend on mutable installed profiles.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceImportIntent {
  /// Stable independent import identity.
  pub id: IssueSourceImportId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Absence creates version one; presence replaces exactly that current revision.
  pub expected_current_version: Option<IntegrationVersion>,
  /// Reviewed reader, predicate and explicit enablement.
  pub definition: IssueSourceImportDefinition,
}
/// One retained immutable import configuration; it contains no task snapshot or workflow authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedIssueSourceImport {
  /// Stable import identity.
  pub id: IssueSourceImportId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Positive immutable integration configuration revision.
  pub version: IntegrationVersion,
  /// Exact original selections.
  pub definition: IssueSourceImportDefinition,
  /// Original acceptance time, not renewed by replay.
  pub published_at: Timestamp,
}
/// Exact historical receipt; replay cannot reactivate a later disabled revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueSourceImportOutcome {
  /// Applied or exact replay.
  pub disposition: MutationDisposition,
  /// Original immutable configuration.
  pub configuration: PublishedIssueSourceImport,
}
/// Atomic metadata publication under the common management replay/audit/outbox boundary.
#[derive(Clone, Debug)]
pub struct PublishIssueSourceImport {
  /// Original public intent.
  pub intent: IssueSourceImportIntent,
  /// Scoped mutation key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted first acceptance time.
  pub published_at: Timestamp,
}
impl PublishIssueSourceImport {
  /// Resolves only static shape and version; current grants remain transactional preconditions.
  pub fn resolve(&self) -> Result<PublishedIssueSourceImport, StoreError> {
    self.intent.definition.validate()?;
    let version = self
      .intent
      .expected_current_version
      .map_or(Ok(IntegrationVersion::INITIAL), |v| v.next())
      .map_err(|_| issue_source_import_conflict())?;
    if version.get() > i64::MAX as u64 {
      return Err(issue_source_import_conflict());
    }
    Ok(PublishedIssueSourceImport {
      id: self.intent.id,
      project_id: self.intent.project_id,
      version,
      definition: self.intent.definition.clone(),
      published_at: self.published_at,
    })
  }
}
/// Exact replay probe, before resolving mutable installed capabilities again.
#[derive(Clone, Debug)]
pub struct ReplayIssueSourceImport {
  /// Original public intent.
  pub intent: IssueSourceImportIntent,
  /// Original scoped key.
  pub idempotency_key: IdempotencyKey,
}
/// Bounded current metadata, independent of polling progress.
#[derive(Clone, Debug, Serialize)]
pub struct IssueSourceImportPage {
  /// Current immutable definitions in stable identity order.
  pub items: Vec<PublishedIssueSourceImport>,
  /// Exclusive stable import position, only when another visible definition exists.
  pub next_cursor: Option<IssueSourceImportId>,
}
/// Safe conflict for stale or foreign import control state.
pub fn issue_source_import_conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Integration,
  }
}
/// Import metadata owns neither Source scans nor task creation; existing consumers own those journals.
#[async_trait::async_trait]
pub trait IssueSourceImportStore: Send + Sync {
  /// Returns an exact original receipt without applying another publication.
  async fn replay_issue_source_import(
    &self,
    request: ManagementMutation<ReplayIssueSourceImport>,
  ) -> Result<Option<IssueSourceImportOutcome>, StoreError>;
  /// Atomically appends one immutable revision, receipt, audit and control-plane outbox event.
  async fn publish_issue_source_import(
    &self,
    request: ManagementMutation<PublishIssueSourceImport>,
  ) -> Result<IssueSourceImportOutcome, StoreError>;
  /// Reads an exact immutable revision; absence selects the current definition.
  async fn issue_source_import(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    id: IssueSourceImportId,
    version: Option<IntegrationVersion>,
  ) -> Result<PublishedIssueSourceImport, StoreError>;
  /// Lists bounded current definitions, including disabled ones, in their original security partition.
  async fn list_issue_source_imports(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    visibility: IntegrationConnectionListVisibility,
    after: Option<IssueSourceImportId>,
    limit: u16,
  ) -> Result<IssueSourceImportPage, StoreError>;
}
