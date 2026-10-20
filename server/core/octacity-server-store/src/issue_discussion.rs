//! Operator discussion is separate from requirements, evidence and execution authority.
use crate::*;
use octacity_server_domain::{IssueCommentId, IssueId, ProjectId, Timestamp};
use octacity_server_factory::FactoryDigest;
use serde::{Deserialize, Serialize};

/// Maximum nonblank UTF-8 bytes per operator comment, independently of character count.
pub const MAX_ISSUE_COMMENT_BODY_BYTES: usize = 16_384;
/// Hard lifetime ceiling, shared by root comments and replies; retention cannot reset it.
pub const MAX_ISSUE_DISCUSSION_COMMENTS: u16 = 1_000;
/// Trusted deployment bounds may narrow, never widen, the discussion contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IssueDiscussionLimits {
  body_bytes: usize,
  lifetime_comments: u16,
}
impl Default for IssueDiscussionLimits {
  fn default() -> Self {
    Self {
      body_bytes: MAX_ISSUE_COMMENT_BODY_BYTES,
      lifetime_comments: MAX_ISSUE_DISCUSSION_COMMENTS,
    }
  }
}
impl IssueDiscussionLimits {
  /// Validates positive deployment bounds against the fixed privacy/storage ceilings.
  pub fn try_new(body_bytes: usize, lifetime_comments: u16) -> Result<Self, StoreError> {
    if body_bytes == 0
      || body_bytes > MAX_ISSUE_COMMENT_BODY_BYTES
      || lifetime_comments == 0
      || lifetime_comments > MAX_ISSUE_DISCUSSION_COMMENTS
    {
      return Err(invalid_issue_discussion());
    }
    Ok(Self {
      body_bytes,
      lifetime_comments,
    })
  }
  /// Effective nonblank UTF-8 byte bound.
  pub const fn body_bytes(self) -> usize {
    self.body_bytes
  }
  /// Effective lifetime bound, including replies and unavailable bodies.
  pub const fn lifetime_comments(self) -> u16 {
    self.lifetime_comments
  }
}
pub(crate) fn invalid_issue_discussion() -> StoreError {
  StoreError::invalid(
    StoreOperation::AppendIssueComment,
    StoreInputError::InvalidIssueDiscussion,
  )
}

/// Private plain-text input; Debug never renders its contents.
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct IssueCommentBody(String);
impl IssueCommentBody {
  /// Constructs operator text for dedicated discussion commands and reads.
  pub fn try_new(value: impl Into<String>) -> Result<Self, StoreError> {
    let value = value.into();
    if value.trim().is_empty()
      || value.len() > MAX_ISSUE_COMMENT_BODY_BYTES
      || octacity_server_factory::contains_sensitive_material(&value)
      || value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
      return Err(invalid_issue_discussion());
    }
    Ok(Self(value))
  }
  /// Borrows the body only at its independently authorized content boundary.
  pub fn as_str(&self) -> &str {
    &self.0
  }
}
impl std::fmt::Debug for IssueCommentBody {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("IssueCommentBody(<redacted>)")
  }
}
impl<'de> Deserialize<'de> for IssueCommentBody {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    Self::try_new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
  }
}
/// Closed operator input; attribution and original time come from trusted management context.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppendIssueCommentIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible owning Issue.
  pub issue_id: IssueId,
  /// Optional same-Issue root comment.
  pub parent_id: Option<IssueCommentId>,
  /// Private non-executable operator text.
  pub body: IssueCommentBody,
}
/// Trusted atomic append input, not a wire actor/time setter.
#[derive(Clone, Debug)]
pub struct AppendIssueComment {
  /// Trusted active deployment bounds, never supplied in a comment body.
  pub limits: IssueDiscussionLimits,
  /// Server-generated stable identity.
  pub id: IssueCommentId,
  /// Original caller intent.
  pub intent: AppendIssueCommentIntent,
  /// Scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Original server time.
  pub created_at: Timestamp,
}
/// Safe metadata for card/timeline references; never includes a comment body or command key.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueCommentMetadata {
  /// Stable original comment identity.
  pub id: IssueCommentId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Owning task.
  pub issue_id: IssueId,
  /// Monotonic lifetime Issue-local sequence.
  pub sequence: u64,
  /// Original root, when this is a reply.
  pub parent_id: Option<IssueCommentId>,
  /// Actual accepted management actor, not a model, assignee or invented human.
  pub actor: AuditActor,
  /// Original server timestamp.
  pub created_at: Timestamp,
  /// Original content identity without its bytes.
  pub body_digest: FactoryDigest,
  /// Original UTF-8 byte size.
  pub body_bytes: u32,
}
/// Dedicated sensitive body read; transports must apply independent authorization and no-store.
#[derive(Clone, Debug, Serialize)]
pub struct IssueCommentContent {
  /// Original attributed metadata.
  pub comment: IssueCommentMetadata,
  /// Original plain text, excluded from generic projections.
  pub body: IssueCommentBody,
}
/// Append receipt contains safe metadata only, including on replay.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IssueCommentOutcome {
  /// Applied or original scoped replay.
  pub disposition: MutationDisposition,
  /// Original immutable metadata.
  pub comment: IssueCommentMetadata,
}
/// Bounded append-order metadata only; dedicated content reads keep their separate permission.
#[derive(Clone, Debug, Serialize)]
pub struct IssueCommentPage {
  /// Safe attributed references in Issue-local sequence order.
  pub items: Vec<IssueCommentMetadata>,
  /// Exclusive continuation when another metadata row exists.
  pub next_sequence: Option<u64>,
}
/// Narrow discussion authority with no Factory, input-publication or provider port.
#[async_trait::async_trait]
pub trait IssueDiscussionStore: Send + Sync {
  /// Lists safe references using the shared Issue history bound, without reading body content.
  async fn issue_comments(
    &self,
    project: ProjectId,
    issue: IssueId,
    after: Option<u64>,
    limit: u16,
  ) -> Result<IssueCommentPage, StoreError>;
  /// Atomically allocates sequence and commits body, original replay and safe evidence.
  async fn append_issue_comment(
    &self,
    request: ManagementMutation<AppendIssueComment>,
  ) -> Result<IssueCommentOutcome, StoreError>;
  /// Reads one dedicated body only within its owning Project/Issue.
  async fn issue_comment_body(
    &self,
    project: ProjectId,
    issue: IssueId,
    comment: IssueCommentId,
  ) -> Result<IssueCommentContent, StoreError>;
}
