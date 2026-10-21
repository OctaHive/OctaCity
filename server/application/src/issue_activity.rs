//! Bounded safe activity over existing Issue records; no execution/provider or body dependencies.
use crate::*;
use octacity_server_domain::{IssueId, IssueVersion, ProjectId};
use octacity_server_factory::FactoryDigest;
pub use octacity_server_store::{IssueTimelineBoundary, IssueTimelineKind, IssueTimelineReference};
use octacity_server_store::{IssueTimelineStore, ReadIssueTimeline, ReadVisibilityView};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};
/// Maximum canonical metadata-only timeline/result cursor size.
pub const MAX_ISSUE_OBSERVATION_CURSOR_BYTES: usize = 2048;
const OBSERVATION: &str = "issue-timeline-watermark-v1";
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
  scope: FactoryDigest,
  boundary: IssueTimelineBoundary,
  after: octacity_server_store::IssueTimelinePosition,
}
/// Bounded metadata timeline; independent content readers retain their own permissions.
#[derive(Clone, Debug)]
pub struct ListIssueTimelineQuery {
  project_id: ProjectId,
  issue_id: IssueId,
  kinds: BTreeSet<IssueTimelineKind>,
  after: Option<Cursor>,
  limit: u16,
}
impl ListIssueTimelineQuery {
  /// Validates shared page bounds and canonical cursor grammar before reading a resource.
  pub fn try_new(
    project_id: ProjectId,
    issue_id: IssueId,
    kinds: Option<BTreeSet<IssueTimelineKind>>,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > octacity_server_store::MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    let kinds = kinds.unwrap_or_else(|| IssueTimelineKind::ALL.into_iter().collect());
    if kinds.is_empty() {
      return Err(ApplicationError::invalid());
    }
    let after = after
      .map(|s| {
        crate::opaque_cursor::decode_canonical(&s, MAX_ISSUE_OBSERVATION_CURSOR_BYTES)
          .map_err(|()| ApplicationError::invalid())
      })
      .transpose()?;
    Ok(Self {
      project_id,
      issue_id,
      kinds,
      after,
      limit,
    })
  }
}
impl Query for ListIssueTimelineQuery {
  type Outcome = IssueTimelinePage;
}
impl ManagementAuthorizationTarget for ListIssueTimelineQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Independent observed membership and freshness, with no implied full verification coverage.
#[derive(Clone, Debug, Serialize)]
pub struct IssueTimelinePage {
  /// Bounded safe references ordered by original timestamp/source/identity.
  pub items: Vec<IssueTimelineReference>,
  /// Opaque continuation bound to current scope, filters and original observation.
  pub next_cursor: Option<String>,
  /// Declared local membership contract; no cross-system atomicity claim.
  pub observation: &'static str,
  /// Original revision/request/comment/Source membership boundaries.
  pub boundary: IssueTimelineBoundary,
  /// Independently current local revision.
  pub current_version: IssueVersion,
  /// Independently changed Source metadata does not erase other visible sections.
  pub source_stale: bool,
  /// Source coverage is unavailable independently of retained local metadata.
  pub source_unavailable: bool,
  /// Trusted read time, not the original time of an event/provider observation.
  pub observed_at: Timestamp,
}
/// Pure optional metadata composition, usable without Factory or installed providers.
pub struct IssueActivityHandlers<S> {
  store: Arc<S>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> IssueActivityHandlers<S> {
  /// Composes the retained metadata store and existing trusted clock.
  pub fn new(store: Arc<S>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self { store, clock }
  }
}
#[async_trait::async_trait]
impl<S: IssueTimelineStore> ManagementQueryUseCase<ListIssueTimelineQuery> for IssueActivityHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueTimelineQuery,
  ) -> Result<IssueTimelinePage, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    let scope = observation_scope(
      context,
      grant,
      serde_json::json!({"project":query.project_id,"issue":query.issue_id,"kinds":query.kinds}),
      OBSERVATION,
    )?;
    if query.after.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let page = self
      .store
      .issue_timeline(ReadIssueTimeline {
        project_id: query.project_id,
        issue_id: query.issue_id,
        scope: context.security_scope().to_store(),
        kinds: query.kinds,
        boundary: query.after.as_ref().map(|c| c.boundary),
        after: query.after.map(|c| c.after),
        limit: query.limit,
      })
      .await?;
    let next_cursor = page.next.map(|after| {
      crate::opaque_cursor::encode(&Cursor {
        scope,
        boundary: page.boundary,
        after,
      })
    });
    Ok(IssueTimelinePage {
      items: page.items,
      next_cursor,
      observation: OBSERVATION,
      boundary: page.boundary,
      current_version: page.current_version,
      source_stale: page.source_stale,
      source_unavailable: page.source_unavailable,
      observed_at: self.clock.now()?,
    })
  }
}

/// Canonical current visibility and security binding shared by independent card observations.
pub(crate) fn observation_scope(
  context: &ManagementRequestContext,
  grant: &ManagementAuthorizationGrant,
  selection: serde_json::Value,
  contract: &str,
) -> Result<FactoryDigest, ApplicationError> {
  let visibility = grant
    .visibility_for::<ListProjectIssuesQuery>()
    .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
  Ok(observation_scope_for(context, visibility.view(), selection, contract))
}

/// Shared canonical observation binding for each reader's typed visibility identities.
pub(crate) fn observation_scope_for<I: Serialize>(
  context: &ManagementRequestContext,
  visibility: ReadVisibilityView<'_, I>,
  selection: serde_json::Value,
  contract: &str,
) -> FactoryDigest {
  let normalized = match visibility {
    ReadVisibilityView::All => serde_json::json!("all"),
    ReadVisibilityView::None => serde_json::json!("none"),
    ReadVisibilityView::Restricted(ids) => serde_json::json!(ids),
  };
  FactoryDigest::content_sha256(serde_json::json!({"selection":selection,"security":context.security_scope().as_str(),"visibility":normalized,"observation":contract}).to_string().as_bytes())
}
