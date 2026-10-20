//! Reviewed conflict resolution publishes a new policy intent, never reopens an old effect.
use crate::*;
use octacity_server_factory::FactoryDigest;
use serde::{Deserialize, Serialize};

/// Exact terminal observation reviewed by an operator; no force-write flag exists.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceResolutionReview {
  /// Original failed operation, whose desired material remains immutable.
  pub operation_id: FactoryDigest,
  /// Exact reviewed terminal receipt, independently of Issue and mapping versions.
  pub expected_record: FactoryDigest,
}
/// Canonical replay identity, excluding trusted installed capability material.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveIssueSourceConflictIntent {
  /// Current Issue/mapping preconditions and explicitly reviewed replacement policy.
  pub mapping: PublishIssueSourceMappingIntent,
  /// Original terminal receipt being resolved.
  pub review: IssueSourceResolutionReview,
}
/// Trusted resolution reuses the ordinary installed mapping publication ceiling.
#[derive(Clone)]
pub struct ResolveIssueSourceConflict {
  /// Current reviewed policy and separately authorized installed effects.
  pub publication: PublishIssueSourceMapping,
  /// Exact failure being resolved, never a blind retry request.
  pub review: IssueSourceResolutionReview,
}
impl ResolveIssueSourceConflict {
  /// Canonical scoped command identity for restart and changed-key rejection.
  pub fn intent(&self) -> ResolveIssueSourceConflictIntent {
    ResolveIssueSourceConflictIntent {
      mapping: self.publication.intent.clone(),
      review: self.review.clone(),
    }
  }
  /// Qualifies one new desired input against locked local facts and its original receipt.
  pub fn push_intent(
    &self,
    current: &IssueRevision,
    link: &IssueSourceLink,
    mapping: &PublishedIssueSourceMapping,
    original: &IssuePushProjection,
    audit: &MutationAuditContext,
  ) -> Result<IssuePushIntent, StoreError> {
    let conflict = crate::issue_source_mapping::mapping_conflict;
    let previous = original.intent.desired.as_ref().ok_or_else(conflict)?;
    if original.record.operation_id != self.review.operation_id
      || original.record.id != self.review.expected_record
      || original.record.state != FactoryOutboxState::Failed
      || original.intent.project_id != current.project_id
      || original.intent.issue_id != current.id
      || previous.link.selection != link.selection
      || previous.command.security_scope != audit.security_scope().as_str()
      || self.publication.published_at < original.record.recorded_at
    {
      return Err(conflict());
    }
    let mut intent = IssuePushIntent {
      project_id: current.project_id,
      issue_id: current.id,
      version: current.version,
      mapping: mapping.reference(),
      fields: [IssueField::Status].into(),
      desired: None,
    };
    let mut desired = intent.desired_state_at(
      current,
      link,
      mapping,
      audit,
      &self.publication.idempotency_key,
      None,
      self.publication.published_at,
    )?;
    let effects = desired.effects.as_ref().ok_or_else(conflict)?;
    if effects.add.is_empty() && effects.remove.is_empty() && effects.state.is_none() {
      return Err(conflict());
    }
    desired.writer_access = self.publication.writer_access.clone();
    desired.command.resolves = Some(original.record.operation_id);
    intent.desired = Some(desired);
    Ok(intent)
  }
}
/// Optional synchronized-task capability; ordinary manual Issue handlers do not require it.
#[async_trait::async_trait]
pub trait IssueSourceResolutionStore: IssueSourceMappingStore + IssueSourceProjectionStore {
  /// Probes an exact original receipt before resolving current installed credentials.
  async fn issue_source_resolution_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &ResolveIssueSourceConflictIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceMappingOutcome>, StoreError>;
  /// Atomically publishes reviewed policy and one new intent through the common outbox/order journal.
  async fn resolve_issue_source_conflict(
    &self,
    request: ManagementMutation<ResolveIssueSourceConflict>,
  ) -> Result<IssueSourceMappingOutcome, StoreError>;
}
