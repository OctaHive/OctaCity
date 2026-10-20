//! Selected execution reads compose existing Factory authority, never admission or provider calls.
use super::*;
use octacity_server_store::{FactoryDiscoveryStore, FactoryRunSummary, IssueStore};
use serde::Serialize;

#[path = "issue_execution_records.rs"]
mod records;
#[path = "issue_execution_results.rs"]
mod results;
pub use records::*;
pub use results::*;

#[path = "issue_execution_job.rs"]
mod job;
pub use job::*;

/// Selects any retained Issue Work, including imported or legacy Artifact admission.
#[derive(Clone, Debug)]
pub struct GetIssueWorkExecutionQuery {
  /// Owning Project shared by Issue and Work.
  pub project_id: ProjectId,
  /// Visible Issue; a Work identity alone grants no association access.
  pub issue_id: octacity_server_domain::IssueId,
  /// Original Work, never a guessed latest Run.
  pub work_id: octacity_server_factory::WorkEnvelopeId,
}
impl Query for GetIssueWorkExecutionQuery {
  type Outcome = IssueWorkExecutionProjection;
}
impl ManagementAuthorizationTarget for GetIssueWorkExecutionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Exact association; absent requirements/request identity stays absent for Artifact lineage.
#[derive(Clone, Debug, Serialize)]
pub struct IssueWorkExecutionProjection {
  /// Original association, without fabricated content provenance.
  pub association: octacity_server_store::IssueWorkAssociation,
  /// Independently authorized technical observation.
  pub run: IssueRunObservation,
}

/// Reads one original request and its independently current technical state.
#[derive(Clone, Debug)]
pub struct GetIssueExecutionProjectionQuery {
  /// Exact Issue/request selection; no latest-completed shortcut.
  pub execution: GetIssueExecutionQuery,
}
impl Query for GetIssueExecutionProjectionQuery {
  type Outcome = IssueExecutionProjection;
}
impl ManagementAuthorizationTarget for GetIssueExecutionProjectionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.execution.management_resource()
  }
}
/// Compares frozen requirement digests, rather than unrelated status/planning revisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueRequirementsRelation {
  /// Selected title, description and acceptance still match the card.
  Current,
  /// At least one selected requirement differs; original results remain retained.
  Historical,
}
/// Safe owner-projected Run facts, excluding input, Context, leases and result bodies.
#[derive(Clone, Debug, Serialize)]
pub struct IssueRunProjection {
  /// Original Factory Run and Work, including all retries/nested Flows.
  pub id: octacity_server_factory::FactoryRunId,
  /// Original immutable admitted input identity.
  pub work_id: octacity_server_factory::WorkEnvelopeId,
  /// Technical lifecycle only; never an Issue business status.
  pub state: octacity_server_factory::FactoryRunState,
  /// Independently observed owner version.
  pub version: octacity_server_factory::FactoryRunVersion,
  /// Bounded active nodes and exact subject-bound dispositions from common discovery.
  pub diagnostics: octacity_server_store::FactoryRunSummaryDiagnostics,
  /// Original admission time.
  pub admitted_at: Timestamp,
  /// Latest retained transition time.
  pub updated_at: Timestamp,
}
impl From<FactoryRunSummary> for IssueRunProjection {
  fn from(run: FactoryRunSummary) -> Self {
    Self {
      id: run.id,
      work_id: run.work_id,
      state: run.state,
      version: run.version,
      diagnostics: run.diagnostics,
      admitted_at: run.admitted_at,
      updated_at: run.updated_at,
    }
  }
}
/// A missing/hidden technical read cannot impersonate an empty or successful Run.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "availability", content = "resource", rename_all = "snake_case")]
pub enum IssueRunObservation {
  /// Reservation has not yet acquired its durable Work/Run association.
  Preparing,
  /// Exact authorized current observation, independent of synchronization.
  Available(Box<IssueRunProjection>),
  /// Retained association exists but current Factory visibility/availability prevents this read.
  Unavailable,
}
/// One selected execution observation; history enumeration remains the existing request reader.
#[derive(Clone, Debug, Serialize)]
pub struct IssueExecutionProjection {
  /// Immutable request/association provenance with no document bytes.
  pub request: IssueExecutionRequest,
  /// Card revision observed separately from the Run owner version.
  pub issue_version: octacity_server_domain::IssueVersion,
  /// Whether original requirements still describe the current card.
  pub requirements: IssueRequirementsRelation,
  /// Independent technical observation, without overall task acceptance.
  pub run: IssueRunObservation,
}

impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + 'static> IssueExecutionHandlers<S, F> {
  async fn visible_issue_work(
    &self,
    grant: &ManagementAuthorizationGrant,
    query: &GetIssueWorkExecutionQuery,
  ) -> Result<octacity_server_store::IssueWorkAssociation, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    self.store.issue(query.project_id, query.issue_id, None).await?;
    self
      .store
      .issue_work_association(query.project_id, query.work_id)
      .await?
      .filter(|a| a.project_id == query.project_id && a.issue_id == query.issue_id && a.work_id == query.work_id)
      .ok_or(ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Issue,
      }))
  }
  async fn execution_projection(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueExecutionQuery,
  ) -> Result<IssueExecutionProjection, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    let issue = self.store.issue(query.project_id, query.issue_id, None).await?;
    let request = self
      .store
      .issue_execution(query.project_id, query.issue_id, query.request_id)
      .await?;
    let digest = octacity_server_factory::FactoryDigest::content_sha256;
    let requirements = if request.title_digest == digest(issue.content.title.as_bytes())
      && request.description_digest == digest(issue.content.description.as_bytes())
      && request.acceptance_digest == digest(issue.content.acceptance.as_deref().unwrap_or_default().as_bytes())
    {
      IssueRequirementsRelation::Current
    } else {
      IssueRequirementsRelation::Historical
    };
    let run = match &request.association {
      None => IssueRunObservation::Preparing,
      Some(association) => match self.visible_execution_run(context, association).await {
        Ok(run) => IssueRunObservation::Available(Box::new(run.into())),
        Err(_) => IssueRunObservation::Unavailable,
      },
    };
    Ok(IssueExecutionProjection {
      request,
      issue_version: issue.version,
      requirements,
      run,
    })
  }
  async fn visible_execution_run(
    &self,
    context: &ManagementRequestContext,
    association: &octacity_server_store::IssueWorkAssociation,
  ) -> Result<FactoryRunSummary, ApplicationError> {
    let handlers = AuthorizedQueryHandler::new(
      self.policy.clone(),
      Arc::new(FactoryDiscoveryHandlers::with_policy(
        self.factory.clone(),
        self.policy.clone(),
      )),
    );
    let run = handlers
      .handle_query(
        context,
        GetFactoryRunQuery {
          run_id: association.run_id,
        },
      )
      .await
      .map_err(execution_read_error)?;
    if run.project_id != association.project_id || run.work_id != association.work_id || run.id != association.run_id {
      return Err(ApplicationError::unavailable());
    }
    Ok(run)
  }
}

#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + 'static>
  ManagementQueryUseCase<GetIssueWorkExecutionQuery> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueWorkExecutionQuery,
  ) -> Result<IssueWorkExecutionProjection, ApplicationError> {
    let association = self.visible_issue_work(grant, &query).await?;
    let run = match self.visible_execution_run(context, &association).await {
      Ok(run) => IssueRunObservation::Available(Box::new(run.into())),
      Err(_) => IssueRunObservation::Unavailable,
    };
    Ok(IssueWorkExecutionProjection { association, run })
  }
}

fn execution_read_error(error: ManagementHandlerError<ApplicationError>) -> ApplicationError {
  match error {
    ManagementHandlerError::Application(error) => error,
    ManagementHandlerError::Forbidden(_) => ApplicationError::Store(octacity_server_store::StoreError::NotFound {
      entity: octacity_server_domain::EntityKind::FactoryRun,
    }),
  }
}

#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + 'static>
  ManagementQueryUseCase<GetIssueExecutionProjectionQuery> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueExecutionProjectionQuery,
  ) -> Result<IssueExecutionProjection, ApplicationError> {
    self.execution_projection(context, grant, query.execution).await
  }
}
