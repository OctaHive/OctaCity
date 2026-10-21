//! Version-fenced bounded reads over the common safe Factory diagnostics.
use super::*;
use octacity_server_factory::{FactoryDigest, FactoryRunVersion};
use octacity_server_store::{FactoryRunStore, IssueTimelineStore, ReadIssueTimeline};
use serde::{Deserialize, Serialize};
const OBSERVATION: &str = "issue-result-run-version-v1";
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
  scope: FactoryDigest,
  boundary: IssueTimelineBoundary,
  run_version: FactoryRunVersion,
  after: String,
}
/// Exact execution/collection selection; cursors never confer execution or body authority.
#[derive(Clone, Debug)]
pub struct ListIssueResultsQuery {
  execution: GetIssueWorkExecutionQuery,
  collection: FactoryDiagnosticCollection,
  after: Option<Cursor>,
  limit: u16,
}
impl ListIssueResultsQuery {
  /// Uses shared Factory bounds and the canonical Issue observation cursor codec.
  pub fn try_new(
    execution: GetIssueWorkExecutionQuery,
    collection: FactoryDiagnosticCollection,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    let after = after
      .map(|s| {
        crate::opaque_cursor::decode_canonical(&s, MAX_ISSUE_OBSERVATION_CURSOR_BYTES)
          .map_err(|()| ApplicationError::invalid())
      })
      .transpose()?;
    Ok(Self {
      execution,
      collection,
      after,
      limit,
    })
  }
}
impl Query for ListIssueResultsQuery {
  type Outcome = IssueResultPage;
}
impl ManagementAuthorizationTarget for ListIssueResultsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.execution.management_resource()
  }
}
/// Observed collection coverage is independent of technical completion and acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueResultCoverage {
  /// A bounded collection is available; absent evidence and exhausted pages prove no pass.
  Partial,
  /// The original Run changed; start a fresh traversal rather than mix observations.
  Stale,
  /// The retained association exists but its independently authorized content cannot be read.
  Unavailable,
}
/// One independently observed collection with exact immutable execution provenance.
#[derive(Clone, Debug, Serialize)]
pub struct IssueResultPage {
  /// Original selected association, including nullable legacy requirements provenance.
  pub association: octacity_server_store::IssueWorkAssociation,
  /// Explicit per-Run observation contract, never a global verification verdict.
  pub observation: &'static str,
  /// Original Issue/request/comment/Source observation, shared with timeline reads.
  pub boundary: IssueTimelineBoundary,
  /// Separately current Issue revision; changes do not rewrite historical results.
  pub current_version: octacity_server_domain::IssueVersion,
  /// Source changed independently of the retained execution.
  pub source_stale: bool,
  /// Optional Source coverage is independently unavailable.
  pub source_unavailable: bool,
  /// Original Run version, absent when technical content was unavailable from the outset.
  pub run_version: Option<FactoryRunVersion>,
  /// Explicit partial/stale/unavailable coverage; no implicit acceptance boolean.
  pub coverage: IssueResultCoverage,
  /// Common subject-bound safe records, with no body expansion.
  pub items: Vec<FactoryDiagnosticRecord>,
  /// Exact scope/selection/observation continuation, absent on stale/unavailable reads.
  pub next_cursor: Option<String>,
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore + IssueTimelineStore, F: FactoryDiscoveryStore + FactoryRunStore + 'static>
  ManagementQueryUseCase<ListIssueResultsQuery> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueResultsQuery,
  ) -> Result<IssueResultPage, ApplicationError> {
    crate::issue::visible_issue(grant, query.execution.issue_id)?;
    let scope = crate::issue_activity::observation_scope(
      context,
      grant,
      serde_json::json!({"project":query.execution.project_id,"issue":query.execution.issue_id,"work":query.execution.work_id,"collection":query.collection}),
      OBSERVATION,
    )?;
    if query.after.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let association = self.visible_issue_work(grant, &query.execution).await?;
    let local = self
      .store
      .issue_timeline(ReadIssueTimeline {
        project_id: association.project_id,
        issue_id: association.issue_id,
        scope: context.security_scope().to_store(),
        kinds: [IssueTimelineKind::Source].into_iter().collect(),
        boundary: query.after.as_ref().map(|c| c.boundary),
        after: None,
        limit: 1,
      })
      .await?;
    let mut result = IssueResultPage {
      association: association.clone(),
      observation: OBSERVATION,
      boundary: local.boundary,
      current_version: local.current_version,
      source_stale: local.source_stale,
      source_unavailable: local.source_unavailable,
      run_version: query.after.as_ref().map(|c| c.run_version),
      coverage: IssueResultCoverage::Unavailable,
      items: Vec::new(),
      next_cursor: None,
    };
    let Ok(run) = self.visible_execution_run(context, &association).await else {
      return Ok(result);
    };
    let version = result.run_version.unwrap_or(run.version);
    result.run_version = Some(version);
    if run.version != version {
      result.coverage = IssueResultCoverage::Stale;
      return Ok(result);
    }
    // Reuse the existing authorized record reader and its privacy/subject allowlists.
    let page = match <Self as ManagementQueryUseCase<ListIssueExecutionRecordsQuery>>::execute_management_query(
      self,
      context,
      grant,
      ListIssueExecutionRecordsQuery::try_new(
        query.execution,
        query.collection,
        query.after.map(|c| c.after),
        query.limit,
      )?,
    )
    .await
    {
      Ok(page) => page.page,
      Err(error)
        if matches!(
          error.classification(),
          ApplicationFailure::NotFound | ApplicationFailure::Unavailable
        ) =>
      {
        return Ok(result);
      }
      Err(error) => return Err(error),
    };
    result.coverage = self.result_coverage(context, &association, version).await;
    if result.coverage != IssueResultCoverage::Partial {
      return Ok(result);
    }
    result.items = page.items;
    result.next_cursor = page.next_cursor.map(|after| {
      crate::opaque_cursor::encode(&Cursor {
        scope,
        boundary: result.boundary,
        run_version: version,
        after,
      })
    });
    Ok(result)
  }
}
