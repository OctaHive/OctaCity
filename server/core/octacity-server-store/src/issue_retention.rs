//! Explicit collaboration-content lifetime, independent of execution evidence retention.
use crate::{IdempotencyKey, ManagementMutation, StoreError};
use octacity_server_domain::{IssueId, IssueVersion, ProjectId, Timestamp};
use serde::{Deserialize, Serialize};

/// A content hold preserves bodies still present; it cannot restore expired data.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueContentHold {
  /// Bounded operational explanation, not an execution command.
  pub reason: String,
  /// Absent means an indefinite hold until an explicitly reviewed replacement.
  pub expires_at: Option<Timestamp>,
}
impl std::fmt::Debug for IssueContentHold {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IssueContentHold")
      .field("expires_at", &self.expires_at)
      .finish_non_exhaustive()
  }
}
/// Independently administered body lifetimes; neither kind controls execution evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueContentRetentionKind {
  /// Versioned requirements text.
  Requirements,
  /// Append-only operator discussion bodies.
  Discussion,
}
impl IssueContentRetentionKind {
  /// Stable storage and HTTP selector, never provider vocabulary.
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Requirements => "requirements",
      Self::Discussion => "discussion",
    }
  }
}
/// A reviewed immutable cutoff excludes later content in the same lifetime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IssueContentRetentionScope {
  /// Highest existing revision whose text may expire.
  Requirements {
    /// Inclusive existing requirements revision.
    through: IssueVersion,
  },
  /// Highest allocated sequence whose body may expire.
  Discussion {
    /// Inclusive allocated discussion sequence.
    through: u64,
  },
}
impl IssueContentRetentionScope {
  /// Each kind has its own version, deadline and hold.
  pub const fn kind(&self) -> IssueContentRetentionKind {
    match self {
      Self::Requirements { .. } => IssueContentRetentionKind::Requirements,
      Self::Discussion { .. } => IssueContentRetentionKind::Discussion,
    }
  }
}
/// Reviewed body lifetime, separate for requirements and discussion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueContentRetentionPlan {
  /// Exact body class and immutable reviewed cutoff.
  pub scope: IssueContentRetentionScope,
  /// Exclusive content lifetime, with no effect on original Work/Artifact lifetimes.
  pub expires_at: Timestamp,
  /// Independent collaboration-content hold.
  pub hold: Option<IssueContentHold>,
}
impl IssueContentRetentionPlan {
  /// Uses existing text and lifetime bounds, without provider or execution policy.
  pub fn validate(&self) -> Result<(), StoreError> {
    if matches!(self.scope, IssueContentRetentionScope::Discussion { through } if through == 0 || through > u64::from(crate::MAX_ISSUE_DISCUSSION_COMMENTS))
      || self.hold.as_ref().is_some_and(|hold| {
        hold.reason.is_empty()
          || hold.reason.trim() != hold.reason
          || hold.reason.len() > crate::MAX_ISSUE_TITLE_BYTES
          || hold.reason.chars().any(char::is_control)
          || octacity_server_factory::contains_sensitive_material(&hold.reason)
      })
    {
      return Err(crate::issue::invalid_issue(crate::StoreOperation::EditIssue));
    }
    Ok(())
  }
}
/// Trusted maintenance command; ordinary text edits cannot configure cleanup or release a hold.
#[derive(Clone, Debug)]
pub struct SetIssueContentRetention {
  /// Exact Project authority.
  pub project_id: ProjectId,
  /// Exact task, never latest Work.
  pub issue_id: IssueId,
  /// Current retention plan version; zero reviews an absent plan.
  pub expected_version: u64,
  /// Frozen reviewed cutoffs and hold.
  pub plan: IssueContentRetentionPlan,
  /// Ordinary security-scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted first acceptance time.
  pub configured_at: Timestamp,
}
/// Retained plan identity; body cleanup never removes this fence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueContentRetentionState {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Original Issue.
  pub issue_id: IssueId,
  /// Positive plan version, distinct from the Issue requirements revision.
  pub version: u64,
  /// Original accepted plan, including an explicit hold if present.
  pub plan: IssueContentRetentionPlan,
}
/// Operational retention seam; callers supply independently authorized maintenance context.
#[async_trait::async_trait]
pub trait IssueContentRetentionStore: Send + Sync {
  /// Reads an independently administered plan; an existing Issue may have no plan.
  async fn issue_content_retention(
    &self,
    project: ProjectId,
    issue: IssueId,
    kind: IssueContentRetentionKind,
  ) -> Result<Option<IssueContentRetentionState>, StoreError>;
  /// Changes a plan under its own version and common scoped mutation/audit transaction.
  async fn set_issue_content_retention(
    &self,
    request: ManagementMutation<SetIssueContentRetention>,
  ) -> Result<IssueContentRetentionState, StoreError>;
  /// Removes at most `limit` bodies from one due task, retaining all authority/provenance.
  async fn expire_issue_content(&self, at: Timestamp, limit: u16) -> Result<u16, StoreError>;
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn content_hold_diagnostics_exclude_private_operational_explanations() {
    let plan = IssueContentRetentionPlan {
      scope: IssueContentRetentionScope::Requirements {
        through: IssueVersion::INITIAL,
      },
      expires_at: Timestamp::from_unix_millis(100).unwrap(),
      hold: Some(IssueContentHold {
        reason: "Private incident explanation".into(),
        expires_at: None,
      }),
    };
    plan.validate().unwrap();
    assert!(!format!("{plan:?}").contains("Private incident explanation"));
  }
}
