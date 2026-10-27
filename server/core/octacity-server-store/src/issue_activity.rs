//! Derived safe activity references; no event journal or sensitive-body reader.
use crate::*;
use octacity_server_domain::{IssueId, IssueVersion, ProjectId, Timestamp};
use octacity_server_factory::FactoryDigest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
/// Deterministic equal-time source order; independent of UI labels.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueTimelineKind {
  /// Requirements/planning revision reference, without content.
  Revision,
  /// Explicit business transition facts, without reason text.
  Status,
  /// Original execution request reference, not Run progress or acceptance.
  Execution,
  /// Original attributed comment metadata, without body authority.
  Discussion,
  /// Original scoped Source observation reference, without native payload.
  Source,
}
impl IssueTimelineKind {
  /// Closed set of supported section filters.
  pub const ALL: [Self; 5] = [
    Self::Revision,
    Self::Status,
    Self::Execution,
    Self::Discussion,
    Self::Source,
  ];
}
/// Exclusive timestamp/source/identity continuation within one Issue.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueTimelinePosition {
  /// Original authoritative time, never refresh time.
  pub at: Timestamp,
  /// Stable tie-break source.
  pub kind: IssueTimelineKind,
  /// Safe original identity in the source's own namespace.
  pub identity: String,
}
/// Safe references retain independent content/execution reader authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IssueTimelineReference {
  /// Exact retained Issue revision header.
  Revision {
    /// Original revision time.
    at: Timestamp,
    /// Original revision.
    version: IssueVersion,
  },
  /// Explicit business event, with no organizational reason/body.
  Status {
    /// Original transition time.
    at: Timestamp,
    /// Original revision.
    version: IssueVersion,
    /// Before status; absent for initial placement.
    from: Option<octacity_server_domain::IssueStatusId>,
    /// After status.
    to: octacity_server_domain::IssueStatusId,
    /// Actual accepted actor.
    actor: AuditActor,
  },
  /// Original request metadata; detailed Run reads retain independent authority.
  Execution {
    /// Original trusted request time.
    at: Timestamp,
    /// Original reservation identity.
    request_id: octacity_server_domain::IssueExecutionRequestId,
    /// Exact frozen requirements revision.
    requirements_version: IssueVersion,
  },
  /// Scoped immutable read receipt; expiry describes its proof, not a body retention promise.
  Source {
    /// Original provider observation time.
    at: Timestamp,
    /// Original safe receipt identity.
    id: FactoryDigest,
    /// Original provenance expiry.
    expires_at: Timestamp,
  },
  /// Dedicated comment reference; metadata grants no body access.
  Discussion {
    /// Original attribution and identity.
    comment: IssueCommentMetadata,
  },
}
impl IssueTimelineReference {
  /// The ordering contract is shared by memory and PostgreSQL adapters.
  pub fn position(&self) -> IssueTimelinePosition {
    let (at, kind, identity) = match self {
      Self::Revision { at, version } => (*at, IssueTimelineKind::Revision, version.get().to_string()),
      Self::Status { at, version, .. } => (*at, IssueTimelineKind::Status, version.get().to_string()),
      Self::Execution { at, request_id, .. } => (*at, IssueTimelineKind::Execution, request_id.to_string()),
      Self::Source { at, id, .. } => (*at, IssueTimelineKind::Source, id.to_string()),
      Self::Discussion { comment } => (
        comment.created_at,
        IssueTimelineKind::Discussion,
        comment.id.to_string(),
      ),
    };
    IssueTimelinePosition { at, kind, identity }
  }
}
/// Immutable local membership boundaries; Source uses an independently fenced observation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueTimelineBoundary {
  /// Original Issue head, bounding revision and status facts.
  pub issue_version: IssueVersion,
  /// Original committed request membership sequence.
  pub executions: u64,
  /// Original lifetime comment sequence, independent of retained bodies.
  pub discussion: u64,
  /// Scoped Source observation fingerprint, never native text.
  pub source: Option<FactoryDigest>,
}
/// Already authorized and bounded store read; transport cursors remain application-owned.
#[derive(Clone, Debug)]
pub struct ReadIssueTimeline {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Visible owning task.
  pub issue_id: IssueId,
  /// Current accepted security partition for native metadata.
  pub scope: ManagementSecurityScope,
  /// Normalized selected sections.
  pub kinds: BTreeSet<IssueTimelineKind>,
  /// Original membership, absent for a new traversal.
  pub boundary: Option<IssueTimelineBoundary>,
  /// Exclusive original position.
  pub after: Option<IssueTimelinePosition>,
  /// Shared history bound, validated again by the store.
  pub limit: u16,
}
/// One bounded local observation; never an aggregate acceptance verdict.
#[derive(Clone, Debug)]
pub struct IssueTimelineReadPage {
  /// Safe ordered references only.
  pub items: Vec<IssueTimelineReference>,
  /// Exclusive continuation, absent at the observed end.
  pub next: Option<IssueTimelinePosition>,
  /// Original immutable local membership.
  pub boundary: IssueTimelineBoundary,
  /// Independently current Issue head; disagreement labels the traversal stale.
  pub current_version: IssueVersion,
  /// Source changed independently; other observed sections can still be traversed.
  pub source_stale: bool,
  /// Optional Source metadata cannot currently be read; local sections remain usable.
  pub source_unavailable: bool,
}
impl IssueTimelineReadPage {
  /// Shapes already bounded source windows using the shared deterministic order.
  pub fn bounded(
    mut items: Vec<IssueTimelineReference>,
    boundary: IssueTimelineBoundary,
    current_version: IssueVersion,
    source_stale: bool,
    source_unavailable: bool,
    request: &ReadIssueTimeline,
  ) -> Self {
    items.retain(|row| {
      request.kinds.contains(&row.position().kind) && request.after.as_ref().is_none_or(|p| row.position() > *p)
    });
    items.sort_by_key(IssueTimelineReference::position);
    let more = items.len() > usize::from(request.limit);
    items.truncate(usize::from(request.limit));
    let next = more.then(|| items.last().unwrap().position());
    Self {
      items,
      next,
      boundary,
      current_version,
      source_stale,
      source_unavailable,
    }
  }
}
/// Narrow derived timeline read, reusing existing Issue-owned records.
#[async_trait::async_trait]
pub trait IssueTimelineStore: Send + Sync {
  /// Captures membership and reads metadata without mutation or private body expansion.
  async fn issue_timeline(&self, request: ReadIssueTimeline) -> Result<IssueTimelineReadPage, StoreError>;
}

/// Fingerprints Source change membership and reviewed mapping/pull generation.
/// Quiet rereads keep cursors valid; delayed reads that alter event membership invalidate them.
pub fn issue_timeline_source_watermark(
  native: Option<FactoryDigest>,
  membership: FactoryDigest,
  generation: u64,
  mapping: u64,
) -> Result<FactoryDigest, StoreError> {
  let encoded = serde_json::to_vec(&(native, membership, generation, mapping)).map_err(|_| StoreError::Unavailable)?;
  Ok(FactoryDigest::sha256("octacity.issue.timeline.source.v2", &[&encoded]))
}

impl ReadIssueTimeline {
  /// Enforces public store bounds before allocations or integer conversion in adapters.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.limit == 0
      || self.limit > MAX_ISSUE_HISTORY_PAGE_SIZE
      || self.kinds.is_empty()
      || self
        .after
        .as_ref()
        .is_some_and(|p| p.identity.len() > 128 || p.identity.is_empty())
    {
      return Err(invalid_activity());
    }
    Ok(())
  }
  /// A caller cannot manufacture future local membership or overflow a SQL sequence.
  pub fn validate_boundary(&self, current: IssueVersion, executions: u64, discussion: u64) -> Result<(), StoreError> {
    if self
      .boundary
      .is_some_and(|b| b.issue_version > current || b.executions > executions || b.discussion > discussion)
    {
      return Err(invalid_activity());
    }
    Ok(())
  }
}
fn invalid_activity() -> StoreError {
  StoreError::invalid(
    StoreOperation::ListIssueTimeline,
    StoreInputError::InvalidIssueActivityPage,
  )
}
