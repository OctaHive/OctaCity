//! Progressive external CI reads over the existing immutable review observation journal.
use super::*;
/// Progressive history expansion retains one bounded original-head CI page per response.
pub const MAX_ISSUE_EXTERNAL_CHECK_OBSERVATIONS: u16 = 1;
/// One retained original-head observation per page; each has the common bounded CI detail ceiling.
#[derive(Clone, Debug)]
pub struct ListIssueExternalChecksQuery {
  observations: ListIssueCodeReviewObservationsQuery,
}
impl ListIssueExternalChecksQuery {
  /// Shares original association/security/head-history cursor validation; GET never polls a provider.
  pub fn try_new(
    project: ProjectId,
    issue: IssueId,
    association: FactoryDigest,
    after: Option<String>,
  ) -> Result<Self, ApplicationError> {
    Ok(Self {
      observations: ListIssueCodeReviewObservationsQuery::try_new(
        project,
        issue,
        association,
        after,
        MAX_ISSUE_EXTERNAL_CHECK_OBSERVATIONS,
      )?,
    })
  }
}
impl Query for ListIssueExternalChecksQuery {
  type Outcome = IssueExternalChecksPage;
}
impl ManagementAuthorizationTarget for ListIssueExternalChecksQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.observations.management_resource()
  }
}
/// Safe exact-head CI expansion; missing detail is explicitly unknown/unsupported.
#[derive(Clone, Debug, Serialize)]
pub struct IssueExternalCheckObservation {
  /// Exact original retained observation content identity.
  pub id: FactoryDigest,
  /// Original immutable head, if available.
  pub head: Option<octacity_server_domain::ImmutableRevision>,
  /// Original trusted observation time.
  pub observed_at: octacity_server_domain::Timestamp,
  /// Typed external facts only; no report body, provider content link or acceptance verdict.
  pub checks: Option<IssueExternalChecks>,
}
/// Independently pageable original-head CI, separate from Factory evidence and review summaries.
#[derive(Clone, Debug, Serialize)]
pub struct IssueExternalChecksPage {
  /// Explicit external source; never Factory acceptance.
  pub source: IssueExternalCheckSource,
  /// Original native review and optional immutable execution/candidate lineage.
  pub association: IssueCodeReviewAssociation,
  /// Current independent reader authority; retained facts may remain visible when blocked.
  pub availability: IssueCodeReviewAvailability,
  /// Retained bounded observations never assert complete required-check coverage.
  pub coverage: IssueResultCoverage,
  /// At most one original-head observation, with bounded normalized CI facts.
  pub items: Vec<IssueExternalCheckObservation>,
  /// Original head history/visibility cursor, never a provider cursor or read grant.
  pub next_cursor: Option<String>,
}
#[async_trait::async_trait]
impl<S: IssueCodeReviewStore> ManagementQueryUseCase<ListIssueExternalChecksQuery> for IssueCodeReviewHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueExternalChecksQuery,
  ) -> Result<IssueExternalChecksPage, ApplicationError> {
    let page = self.read_observation_page(context, grant, query.observations).await?;
    Ok(IssueExternalChecksPage {
      source: IssueExternalCheckSource::ExternalCi,
      association: page.association,
      availability: page.availability,
      coverage: IssueResultCoverage::Partial,
      items: page
        .items
        .into_iter()
        .map(|record| IssueExternalCheckObservation {
          id: record.id,
          head: record.observation.head,
          observed_at: record.observation.observed_at,
          checks: record.observation.check_details,
        })
        .collect(),
      next_cursor: page.next_cursor,
    })
  }
}
