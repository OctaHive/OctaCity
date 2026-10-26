//! Canonical native links; input/admission replay identities remain independent.
use crate::{
  IdempotencyKey, IssueFieldPolicy, IssueStore, ManagementMutation, ManagementSecurityScope, MutationDisposition,
  StoreError, WorkSourceItem,
};
use octacity_server_domain::{IssueId, IssueVersion, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, WorkSourceBinding};
use serde::{Deserialize, Serialize};

/// Exact installed read selection; display values and credentials are not native identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceSelection {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Original managed native connection pin; absent only for explicit legacy installations.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub connection: Option<octacity_server_factory::WorkSourceConnectionRef>,
  /// Reviewed installed capability/configuration; not a runtime grant.
  pub source: WorkSourceBinding,
  /// Verified stable native container and task.
  pub item: WorkSourceItem,
}
impl IssueSourceSelection {
  /// Reuses the existing Source correlation namespace and accepted security partition.
  pub fn native_correlation(&self, scope: &ManagementSecurityScope) -> FactoryDigest {
    crate::source_native_correlation(
      crate::source_native_issue(self.project_id, self.source.installation.identity(), &self.item),
      crate::source_security_partition(scope),
    )
  }
}
/// One retained primary association; attachment grants no synchronization or execution authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceLink {
  /// Original verified display/route locator; never native identity or write authority.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub display_id: Option<String>,
  /// Owning visible local task.
  pub issue_id: IssueId,
  /// Original installed selection, retained across later connection/profile changes.
  pub selection: IssueSourceSelection,
  /// Shared Project/security/native lookup key, independent of the pipeline.
  pub native_correlation: FactoryDigest,
  /// Original server attachment time.
  pub linked_at: Timestamp,
}
impl IssueSourceLink {
  /// Checks immutable native association compatibility; this grants no read or write authority.
  pub fn supports_profile(
    &self,
    source: &WorkSourceBinding,
    native_resource: &octacity_server_factory::ExternalWorkIdentity,
  ) -> bool {
    self.selection.item.resource == *native_resource
      && self.selection.source.installation.identity() == source.installation.identity()
  }
}
/// Frozen manual attachment and ownership review; it cannot replace a link or mapping.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachIssueSourceIntent {
  /// Exact authorized native task selection.
  pub selection: IssueSourceSelection,
  /// Visible unlinked manual task.
  pub issue_id: IssueId,
  /// Reviewed local revision.
  pub expected_version: IssueVersion,
  /// Reviewed existing ownership; attachment does not change it.
  pub expected_field_policy: IssueFieldPolicy,
}
/// Trusted application command after current Source read validation.
#[derive(Clone, Debug)]
pub struct AttachIssueSource {
  /// Original verified read provenance, without provider payload or credentials.
  pub observation: IssueSourceObservation,
  /// Frozen review.
  pub intent: AttachIssueSourceIntent,
  /// Caller replay identity in the accepted management partition.
  pub idempotency_key: IdempotencyKey,
  /// Original server time.
  pub attached_at: Timestamp,
}
/// Trusted normalized import; the same canonical task is returned without refresh or admission.
#[derive(Clone, Debug)]
pub struct ImportIssueSource {
  /// Optional original bounded read fence; checked atomically before new publication.
  pub read_claim: Option<crate::IssueSourceReadClaim>,
  /// Original verified read provenance and exact installed mapping pins.
  pub observation: IssueSourceObservation,
  /// Current authorized local task visibility, checked before canonical replay publication.
  pub visibility: crate::IssueListVisibility,
  /// Exact native read selection.
  pub selection: IssueSourceSelection,
  /// Proposed identity, used only by the winning initial import.
  pub id: IssueId,
  /// Bounded initial draft text from the verified observation.
  pub content: crate::IssueContent,
  /// Scoped caller replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted server time.
  pub imported_at: Timestamp,
}
/// Finite attachment result; the application removes a hidden canonical reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IssueSourceOutcome {
  /// Newly retained or exactly replayed original association.
  Linked {
    /// Original primary association.
    link: IssueSourceLink,
    /// Whether the primary association was newly applied or already retained.
    disposition: MutationDisposition,
  },
  /// No mutation: native task or local primary slot is already associated.
  Conflict {
    /// The application exposes this reference only under current task visibility.
    canonical: Option<IssueSourceLink>,
  },
}
/// Narrow atomic association authority; provider reads belong to the existing Source port.
#[async_trait::async_trait]
pub trait IssueSourceStore: IssueStore {
  /// Reads the original canonical native link in its Project/security partition; performs no import.
  async fn native_issue_source_link(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    native: FactoryDigest,
  ) -> Result<Option<IssueSourceLink>, StoreError>;

  /// Reads the original scoped attachment receipt before another provider read; reserves nothing.
  async fn issue_source_attachment_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &AttachIssueSourceIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceOutcome>, StoreError>;

  /// Reads one retained safe observation under its original native link and partition.
  async fn issue_source_observation(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    id: FactoryDigest,
  ) -> Result<Option<IssueSourceObservation>, StoreError>;
  /// Atomically resolves one native key or creates one ordinary initial Issue and its link.
  async fn import_issue_source(
    &self,
    request: ManagementMutation<ImportIssueSource>,
  ) -> Result<IssueSourceOutcome, StoreError>;
  /// Reads retained association only in its original Project and security partition.
  async fn issue_source_link(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
  ) -> Result<Option<IssueSourceLink>, StoreError>;
  /// Attaches once with shared replay/audit publication; no text/status/provider effects.
  async fn attach_issue_source(
    &self,
    request: ManagementMutation<AttachIssueSource>,
  ) -> Result<IssueSourceOutcome, StoreError>;
}

/// Safe bounded read receipt; native content remains in its dedicated original reader/journal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceObservation {
  /// Verified bounded native display/route locator, absent in legacy provenance.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub display_id: Option<String>,
  /// Exact installed selector/mapping, resource and capability versions used by this read.
  pub source: WorkSourceBinding,
  /// Content-verified executable, independent of canonical native identity.
  pub plugin: octacity_server_factory::ImmutableReference,
  /// Original transport/configuration digest, never credentials.
  pub provider_configuration: FactoryDigest,
  /// Exact installed selector evidence.
  pub selection_digest: FactoryDigest,
  /// Normalized snapshot content identity; no task body or native wire object.
  pub snapshot_digest: FactoryDigest,
  /// Original trusted read time.
  pub observed_at: Timestamp,
  /// Original provenance deadline; replay cannot renew it.
  pub expires_at: Timestamp,
}
impl IssueSourceObservation {
  /// Stable receipt identity bound to the existing Project/security/native correlation.
  pub fn id(&self, native: FactoryDigest) -> Result<FactoryDigest, StoreError> {
    Ok(FactoryDigest::sha256(
      "octacity.issue.source-observation.v1",
      &[
        &native.as_bytes(),
        &serde_json::to_vec(self).map_err(|_| StoreError::Unavailable)?,
      ],
    ))
  }
  /// Validates only provenance and existing bounds, granting no synchronization authority.
  pub fn validate(
    &self,
    selection: &IssueSourceSelection,
    at: Timestamp,
    operation: crate::StoreOperation,
  ) -> Result<(), StoreError> {
    let lifetime = self
      .expires_at
      .unix_millis()
      .checked_sub(self.observed_at.unix_millis())
      .and_then(|v| u64::try_from(v).ok());
    if self
      .display_id
      .as_ref()
      .is_some_and(|id| octacity_server_factory::ExternalWorkIdentity::new(id.clone()).is_err())
      || self.source != selection.source
      || at < self.observed_at
      || at >= self.expires_at
      || lifetime.is_none_or(|v| v == 0 || v > crate::MAX_SOURCE_RETENTION_MILLIS)
    {
      return Err(StoreError::invalid(operation, crate::StoreInputError::InvalidIssue));
    }
    Ok(())
  }
}
