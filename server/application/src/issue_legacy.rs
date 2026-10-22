//! Legacy history reads reuse original associations and the existing exact Work reader.
use crate::*;
use octacity_server_domain::{IssueId, ProjectId};
use octacity_server_factory::{FactoryDigest, WorkEnvelopeId};
use octacity_server_store::IssueLegacyStore;
pub use octacity_server_store::IssueWorkAssociation;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
  scope: FactoryDigest,
  observation: FactoryDigest,
  after: WorkEnvelopeId,
}
/// One visible task's original Artifact-based executions, without invented request IDs.
#[derive(Clone, Debug)]
pub struct ListIssueLegacyWorkQuery {
  project: ProjectId,
  issue: IssueId,
  after: Option<Cursor>,
  limit: u16,
}
impl ListIssueLegacyWorkQuery {
  /// Validates the common execution cursor/page bounds before observing task state.
  pub fn try_new(
    project: ProjectId,
    issue: IssueId,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    let after = after
      .map(|s| {
        crate::opaque_cursor::decode_canonical(&s, MAX_ISSUE_EXECUTION_CURSOR_BYTES)
          .map_err(|()| ApplicationError::invalid())
      })
      .transpose()?;
    Ok(Self {
      project,
      issue,
      after,
      limit,
    })
  }
}
/// Bounded metadata only; every technical Run read retains its independent authorization.
#[derive(Clone, Debug, Serialize)]
pub struct IssueLegacyWorkHistoryPage {
  /// Exact original associations; request/requirements stay null.
  pub items: Vec<IssueWorkAssociation>,
  /// Scope- and membership-bound continuation, with no text or private payload.
  pub next_cursor: Option<String>,
  /// Retained membership and Issue revision fence.
  pub observation: FactoryDigest,
}
impl Query for ListIssueLegacyWorkQuery {
  type Outcome = IssueLegacyWorkHistoryPage;
}
impl ManagementAuthorizationTarget for ListIssueLegacyWorkQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue)
  }
}
#[async_trait::async_trait]
impl<S: IssueLegacyStore, F: Send + Sync> ManagementQueryUseCase<ListIssueLegacyWorkQuery>
  for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueLegacyWorkQuery,
  ) -> Result<IssueLegacyWorkHistoryPage, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue)?;
    // This view consists of exactly one authorized Issue, not a cross-Issue collection.
    let scope = FactoryDigest::sha256(
      "octacity.issue.legacy-work-scope.v1",
      &[
        context.security_scope().as_str().as_bytes(),
        query.project.as_uuid().as_bytes(),
        query.issue.as_uuid().as_bytes(),
      ],
    );
    if query.after.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let page = self
      .legacy_work(
        query.project,
        query.issue,
        query.after.as_ref().map(|c| c.after),
        query.after.as_ref().map(|c| c.observation),
        query.limit,
      )
      .await?;
    let next_cursor = page.next.map(|after| {
      crate::opaque_cursor::encode(&Cursor {
        scope,
        observation: page.observation,
        after,
      })
    });
    Ok(IssueLegacyWorkHistoryPage {
      items: page.items,
      next_cursor,
      observation: page.observation,
    })
  }
}

impl ManagementVisibilityTarget for ListIssueLegacyWorkQuery {
  type Visibility = <ListIssueExecutionsQuery as ManagementVisibilityTarget>::Visibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] =
    <ListIssueExecutionsQuery as ManagementVisibilityTarget>::RESTRICTED_RESOURCE_KINDS;
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    <ListIssueExecutionsQuery as ManagementVisibilityTarget>::visibility_from(grant)
  }
}
