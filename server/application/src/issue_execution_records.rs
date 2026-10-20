//! Selected collections retain the common journal's exact subjects and privacy allowlists.
use super::*;
use octacity_server_store::FactoryRunStore;

/// One bounded collection for an exact Issue-associated Work, not a full Run dump.
#[derive(Clone, Debug)]
pub struct ListIssueExecutionRecordsQuery {
  execution: GetIssueWorkExecutionQuery,
  collection: FactoryDiagnosticCollection,
  after: Option<String>,
  limit: u16,
}
impl ListIssueExecutionRecordsQuery {
  /// Reuses the Factory page bound; the common reader validates Run/collection continuation.
  pub fn try_new(
    execution: GetIssueWorkExecutionQuery,
    collection: FactoryDiagnosticCollection,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      execution,
      collection,
      after,
      limit,
    })
  }
}
impl Query for ListIssueExecutionRecordsQuery {
  type Outcome = IssueExecutionRecordPage;
}
impl ManagementAuthorizationTarget for ListIssueExecutionRecordsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.execution.management_resource()
  }
}
/// Independent live collection coverage; empty evidence/Decisions never establishes a pass.
#[derive(Clone, Debug, Serialize)]
pub struct IssueExecutionRecordPage {
  /// Original selected execution, with nullable legacy requirement provenance.
  pub association: octacity_server_store::IssueWorkAssociation,
  /// Live per-collection observation; no cross-section or frozen-membership claim.
  pub observation: &'static str,
  /// Common bounded safe records, including original candidates/evidence/plans/assessments/Decisions.
  pub page: FactoryDiagnosticPage,
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + FactoryRunStore + 'static>
  ManagementQueryUseCase<ListIssueExecutionRecordsQuery> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueExecutionRecordsQuery,
  ) -> Result<IssueExecutionRecordPage, ApplicationError> {
    let association = self.visible_issue_work(grant, &query.execution).await?;
    let diagnostics = ListFactoryDiagnosticsQuery::try_new(
      association.run_id,
      query.collection,
      query.after.as_deref(),
      query.limit,
    )?;
    self.visible_execution_run(context, &association).await?;
    let handlers = AuthorizedQueryHandler::new(
      self.policy.clone(),
      Arc::new(FactoryDiscoveryHandlers::with_policy(
        self.factory.clone(),
        self.policy.clone(),
      )),
    );
    let page = handlers
      .handle_query(context, diagnostics)
      .await
      .map_err(execution_read_error)?;
    Ok(IssueExecutionRecordPage {
      association,
      observation: "live-factory-collection-v1",
      page,
    })
  }
}
