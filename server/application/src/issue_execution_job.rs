//! Job progress and log targets use exact retained Build linkage and existing authorized reads.
use super::*;
use octacity_server_domain::{AttemptId, BuildId, JobId, JobVersion};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{
  BuildQueryStore, FactoryBuildExecutionDiagnostic, FactoryCallProfileDiagnostic, FactoryRunStore,
};

#[path = "issue_execution_journal.rs"]
mod journal;
pub use journal::*;

/// Expands one Job from a selected execution's retained Build record.
#[derive(Clone, Debug)]
pub struct GetIssueExecutionJobQuery {
  /// Exact Issue/Work selection, including legacy associations.
  pub execution: GetIssueWorkExecutionQuery,
  /// Immutable Build-execution record identity, never the Build's latest Attempt.
  pub build_execution: FactoryDigest,
  /// One original Job belonging to that record.
  pub job_id: JobId,
}
impl Query for GetIssueExecutionJobQuery {
  type Outcome = IssueExecutionJobProjection;
}
impl ManagementAuthorizationTarget for GetIssueExecutionJobQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.execution.management_resource()
  }
}
/// Allowlisted ordinary Job progress; no JobSpec, output references or placement requirements.
#[derive(Clone, Debug, Serialize)]
pub struct IssueJobProjection {
  /// Immutable ordinary Job identity.
  pub id: JobId,
  /// Independently current execution state, including Ready without an assigned worker.
  pub state: octacity_server_job::JobState,
  /// Current Job version, not an Issue or Run precondition.
  pub version: JobVersion,
  /// Original creation time.
  pub created_at: Timestamp,
  /// Latest authoritative Job transition time.
  pub updated_at: Timestamp,
  /// Actual retained enrolled-worker assignment, independent of any model/tool profile.
  pub assignment: Option<JobAssignmentProjection>,
  /// Safe terminal classification, absent for nonterminal Jobs.
  pub terminal: Option<JobTerminalOutcomeProjection>,
}
/// Navigation facts for existing Build/log and Job-event readers; grants no content authority.
#[derive(Clone, Debug, Serialize)]
pub struct IssueJobLogReference {
  /// Original producing Build.
  pub build_id: BuildId,
  /// Original ordinary Attempt, even after retries.
  pub attempt_id: AttemptId,
  /// Original Job stream.
  pub job_id: JobId,
  /// Greatest retained contiguous event sequence; zero is not a complete-log claim.
  pub event_cursor: u64,
}
/// Selected Job observation without logs, transcripts, rendered tasks or aggregate snapshots.
#[derive(Clone, Debug, Serialize)]
pub struct IssueExecutionJobProjection {
  /// Exact retained Issue association.
  pub association: octacity_server_store::IssueWorkAssociation,
  /// Original node/Build/Attempt/Job linkage from the common journal.
  pub execution: FactoryBuildExecutionDiagnostic,
  /// Actual optional frozen tool/model contract; does not invent an AI role or worker.
  pub profile: Option<FactoryCallProfileDiagnostic>,
  /// Independently authorized ordinary Job progress.
  pub job: IssueJobProjection,
  /// Exact authorized-reader target, without log bodies or private transfer URLs.
  pub log: IssueJobLogReference,
}
/// Optional detailed reads; manual cards and execution reservation need no ordinary Job store.
pub struct IssueExecutionJobHandlers<S, F, B> {
  executions: Arc<IssueExecutionHandlers<S, F>>,
  jobs: Arc<B>,
}
impl<S, F, B> IssueExecutionJobHandlers<S, F, B> {
  /// Composes the existing Issue/Factory and ordinary Job boundaries without a new execution owner.
  pub fn new(executions: Arc<IssueExecutionHandlers<S, F>>, jobs: Arc<B>) -> Self {
    Self { executions, jobs }
  }
}
#[async_trait::async_trait]
impl<
  S: IssueExecutionStore + IssueStore,
  F: FactoryDiscoveryStore + FactoryRunStore + 'static,
  B: BuildQueryStore + 'static,
> ManagementQueryUseCase<GetIssueExecutionJobQuery> for IssueExecutionJobHandlers<S, F, B>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueExecutionJobQuery,
  ) -> Result<IssueExecutionJobProjection, ApplicationError> {
    let handlers = &self.executions;
    let association = handlers.visible_issue_work(grant, &query.execution).await?;
    handlers.visible_execution_run(context, &association).await?;
    let snapshot = handlers.factory.factory_run_snapshot(association.run_id).await?;
    let execution = snapshot
      .flow
      .data
      .build_executions
      .iter()
      .find(|execution| {
        execution.digest().ok() == Some(query.build_execution) && execution.jobs().contains(&query.job_id)
      })
      .ok_or_else(ApplicationError::invalid)?;
    for (kind, identity) in [
      (ManagementResourceKind::Build, execution.build_id().to_string()),
      (ManagementResourceKind::Job, query.job_id.to_string()),
    ] {
      crate::factory_configuration_choices::require_visible_reference(
        handlers.policy.as_ref(),
        context,
        kind,
        identity,
      )
      .await?;
    }
    let jobs = AuthorizedQueryHandler::new(handlers.policy.clone(), Arc::new(BuildHandlers::new(self.jobs.clone())));
    let job = jobs
      .handle_query(context, GetJobQuery { job_id: query.job_id })
      .await
      .map_err(execution_read_error)?;
    if job.id != query.job_id || job.attempt_id != execution.attempt_id() {
      return Err(ApplicationError::unavailable());
    }
    let profile = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .find(|intent| intent.node().id() == execution.node_attempt_id())
      .map(FactoryCallProfileDiagnostic::try_from)
      .transpose()
      .map_err(|_| ApplicationError::unavailable())?;
    Ok(IssueExecutionJobProjection {
      log: IssueJobLogReference {
        build_id: execution.build_id(),
        attempt_id: execution.attempt_id(),
        job_id: job.id,
        event_cursor: job.event_cursor,
      },
      association,
      execution: FactoryBuildExecutionDiagnostic::from(execution),
      profile,
      job: IssueJobProjection {
        id: job.id,
        state: job.state,
        version: job.version,
        created_at: job.created_at,
        updated_at: job.updated_at,
        assignment: job.assignment,
        terminal: job.terminal,
      },
    })
  }
}
