//! Read-only, watermarked execution discovery over the same retained request port.
use super::*;
use octacity_server_domain::IssueVersion;
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{IssueExecutionObservation, IssueExecutionPosition, ReadVisibilityView};
use serde::{Deserialize, Serialize};

/// Canonical cursor bound; contains only hashes and retained sequence/identity metadata.
pub const MAX_ISSUE_EXECUTION_CURSOR_BYTES: usize = 1024;
pub use octacity_server_store::MAX_ISSUE_HISTORY_PAGE_SIZE;
const OBSERVATION: &str = "request-watermark-v1";
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
  observation: String,
  scope: FactoryDigest,
  boundary: IssueExecutionObservation,
  after: IssueExecutionPosition,
}
/// Discover execution requests without admitting, linking or contacting providers.
#[derive(Clone, Debug)]
pub struct ListIssueExecutionsQuery {
  project_id: ProjectId,
  issue_id: IssueId,
  after: Option<Cursor>,
  limit: u16,
}
impl ListIssueExecutionsQuery {
  /// Validates the same bounded history grammar used by both persistence backends.
  pub fn try_new(
    project_id: ProjectId,
    issue_id: IssueId,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    let after: Option<Cursor> = after
      .map(|encoded| {
        crate::opaque_cursor::decode_canonical(&encoded, MAX_ISSUE_EXECUTION_CURSOR_BYTES)
          .map_err(|()| ApplicationError::invalid())
      })
      .transpose()?;
    if after.as_ref().is_some_and(|c| c.observation != OBSERVATION) {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      project_id,
      issue_id,
      after,
      limit,
    })
  }
}
impl Query for ListIssueExecutionsQuery {
  type Outcome = IssueExecutionHistoryPage;
}
impl ManagementAuthorizationTarget for ListIssueExecutionsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Retained membership with live association facts; never a second Run projection.
#[derive(Clone, Debug, Serialize)]
pub struct IssueExecutionHistoryPage {
  /// Bounded chronological metadata, excluding private requirements bodies.
  pub items: Vec<IssueExecutionRequest>,
  /// Canonical continuation bound to current authorization and the original observation.
  pub next_cursor: Option<String>,
  /// Declared membership/association observation contract.
  pub observation: &'static str,
  /// Maximum committed reservation sequence in the original traversal.
  pub watermark: u64,
  /// Requirements revision observed when traversal began.
  pub issue_version: IssueVersion,
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore, F: Send + Sync> ManagementQueryUseCase<ListIssueExecutionsQuery>
  for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueExecutionsQuery,
  ) -> Result<IssueExecutionHistoryPage, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    let visibility = grant
      .visibility_for::<ListIssueExecutionsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let normalized = match visibility.view() {
      ReadVisibilityView::All => serde_json::json!({"kind":"all"}),
      ReadVisibilityView::None => serde_json::json!({"kind":"none"}),
      ReadVisibilityView::Restricted(ids) => serde_json::json!({"kind":"restricted","ids":ids}),
    };
    let scope = FactoryDigest::content_sha256(
      serde_json::json!({"project":query.project_id,"issue":query.issue_id,
      "security":context.security_scope().as_str(),"visibility":normalized,"observation":OBSERVATION})
      .to_string()
      .as_bytes(),
    );
    if query.after.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let page = self
      .store
      .issue_executions(
        query.project_id,
        query.issue_id,
        query.after.as_ref().map(|c| c.after),
        query.after.as_ref().map(|c| c.boundary),
        query.limit,
      )
      .await?;
    let next_cursor = page.next.map(|after| {
      crate::opaque_cursor::encode(&Cursor {
        observation: OBSERVATION.into(),
        scope,
        boundary: page.observation,
        after,
      })
    });
    Ok(IssueExecutionHistoryPage {
      items: page.items,
      next_cursor,
      observation: OBSERVATION,
      watermark: page.observation.watermark,
      issue_version: page.observation.issue_version,
    })
  }
}
