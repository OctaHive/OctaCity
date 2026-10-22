//! Exact journal translation; bodies remain separate from bounded safe metadata pages.
use super::*;
use octacity_server_application::{
  FactoryDiagnosticCollection, GetIssueExecutionJobQuery, GetIssueWorkExecutionQuery, ManagementRequestContext,
};

fn selection(
  project: String,
  issue: String,
  request: String,
  id: &RequestId,
) -> Result<GetIssueExecutionQuery, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, id);
  Ok(GetIssueExecutionQuery {
    project_id: project.parse().map_err(|_| invalid())?,
    issue_id: issue.parse().map_err(|_| invalid())?,
    request_id: request.parse().map_err(|_| invalid())?,
  })
}
fn job_target(
  request: &IssueExecutionRequest,
  build: String,
  job: String,
  id: &RequestId,
) -> Result<GetIssueExecutionJobQuery, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, id);
  let association = request
    .association
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))?;
  Ok(GetIssueExecutionJobQuery {
    execution: GetIssueWorkExecutionQuery {
      project_id: request.intent.project_id,
      issue_id: request.intent.issue_id,
      work_id: association.work_id,
    },
    build_execution: build.parse().map_err(|_| invalid())?,
    job_id: job.parse().map_err(|_| invalid())?,
  })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JournalParameters {
  collection: Option<FactoryDiagnosticCollection>,
  after: Option<String>,
  limit: Option<u16>,
}
pub(crate) async fn list(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, request)): Path<(String, String, String)>,
  parameters: Result<Query<JournalParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let query = ListIssueExecutionJournalQuery::try_new(
    selection(project, issue, request, &id)?,
    p.collection.unwrap_or(FactoryDiagnosticCollection::FlowRuns),
    p.after,
    p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let page = endpoints(&app, &id)?
    .journal
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(page, &id)?)).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventParameters {
  build_execution: String,
  job_id: String,
  after: Option<String>,
  limit: Option<u16>,
}
pub(crate) async fn events(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, request)): Path<(String, String, String)>,
  parameters: Result<Query<EventParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let selected = selection(project, issue, request, &id)?;
  let base = format!(
    "/api/v1/projects/{}/issues/{}/executions/{}/journal/events",
    selected.project_id, selected.issue_id, selected.request_id
  );
  let endpoints = endpoints(&app, &id)?;
  let receipt = endpoints
    .get
    .handle_authorized_query(&context, selected)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let target = job_target(&receipt, p.build_execution, p.job_id, &id)?;
  read_events(&app, &id, &context, target, p.after, p.limit, base).await
}
async fn read_events(
  app: &ManagementApplication,
  id: &RequestId,
  context: &ManagementRequestContext,
  target: GetIssueExecutionJobQuery,
  after: Option<String>,
  limit: Option<u16>,
  base: String,
) -> Result<axum::response::Response, ApiError> {
  let endpoints = endpoints(app, id)?;
  let selector = format!("build_execution={}&job_id={}", target.build_execution, target.job_id);
  let query = ReadIssueExecutionJournalEventsQuery::try_new(target, after, limit.unwrap_or(DEFAULT_PAGE_LIMIT))
    .map_err(|e| application_error(e.classification(), id))?;
  let page = endpoints
    .journal_events
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))?
    .handle_authorized_query(context, query)
    .await
    .map_err(|e| authorized_handler_error(e, id))?;
  let log_url = format!("/api/v1/jobs/{}/events", page.job.log.job_id);
  let mut value = encode(page, id)?;
  value["job"]["log"]["events_url"] = json!(log_url);
  for event in value["events"].as_array_mut().expect("serialized event vector") {
    event["detail_url"] = json!(format!("{base}/{}?{selector}", event["sequence"]));
  }
  Ok(([("cache-control", "no-store")], Json(value)).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DetailParameters {
  build_execution: String,
  job_id: String,
}
pub(crate) async fn detail(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, request, sequence)): Path<(String, String, String, String)>,
  parameters: Result<Query<DetailParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let sequence = sequence
    .parse()
    .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let selected = selection(project, issue, request, &id)?;
  let endpoints = endpoints(&app, &id)?;
  let receipt = endpoints
    .get
    .handle_authorized_query(&context, selected)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let query =
    GetIssueExecutionJournalEventQuery::try_new(job_target(&receipt, p.build_execution, p.job_id, &id)?, sequence)
      .map_err(|e| application_error(e.classification(), &id))?;
  read_detail(&app, &id, &context, query).await
}
async fn read_detail(
  app: &ManagementApplication,
  id: &RequestId,
  context: &ManagementRequestContext,
  query: GetIssueExecutionJournalEventQuery,
) -> Result<axum::response::Response, ApiError> {
  let page = endpoints(app, id)?
    .journal_detail
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))?
    .handle_authorized_query(context, query)
    .await
    .map_err(|e| authorized_handler_error(e, id))?;
  Ok(([("cache-control", "no-store")], Json(job_event_page(page))).into_response())
}

// Original Artifact Work uses the same exact Job/linkage/visibility reader.
fn work_job(
  project: String,
  issue: String,
  work: String,
  build: String,
  job: String,
  id: &RequestId,
) -> Result<GetIssueExecutionJobQuery, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, id);
  Ok(GetIssueExecutionJobQuery {
    execution: GetIssueWorkExecutionQuery {
      project_id: project.parse().map_err(|_| invalid())?,
      issue_id: issue.parse().map_err(|_| invalid())?,
      work_id: work.parse().map_err(|_| invalid())?,
    },
    build_execution: build.parse().map_err(|_| invalid())?,
    job_id: job.parse().map_err(|_| invalid())?,
  })
}
pub(crate) async fn work_events(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, work)): Path<(String, String, String)>,
  parameters: Result<Query<EventParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let target = work_job(project, issue, work, p.build_execution, p.job_id, &id)?;
  let selected = &target.execution;
  let base = format!(
    "/api/v1/projects/{}/issues/{}/work/{}/journal/events",
    selected.project_id, selected.issue_id, selected.work_id
  );
  read_events(&app, &id, &context, target, p.after, p.limit, base).await
}
pub(crate) async fn work_detail(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, work, sequence)): Path<(String, String, String, String)>,
  parameters: Result<Query<DetailParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let query = GetIssueExecutionJournalEventQuery::try_new(
    work_job(project, issue, work, p.build_execution, p.job_id, &id)?,
    sequence.parse().map_err(|_| invalid())?,
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  read_detail(&app, &id, &context, query).await
}
