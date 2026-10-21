//! Execution HTTP translation over existing authorized Issue requests and shared publication.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler, GetIssueExecutionQuery,
  IssueExecutionRequest, ListIssueExecutionsQuery, PublishIssueExecutionCommand, RequestIssueExecutionCommand,
};
#[path = "issue_execution_journal.rs"]
pub(super) mod journal;
use octacity_server_application::{
  GetIssueExecutionJournalEventQuery, ListIssueExecutionJournalQuery, ReadIssueExecutionJournalEventsQuery,
};

/// Optional execution composition; manual Issue management has no execution dependency.
struct Publication {
  request: Arc<dyn AuthorizedManagementCommandHandler<RequestIssueExecutionCommand, Error = ApplicationError>>,
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishIssueExecutionCommand, Error = ApplicationError>>,
  timeout: Duration,
}
/// Optional execution services independent of manual Issue management.
pub struct IssueExecutionManagementApplication {
  journal: Option<Arc<dyn AuthorizedManagementQueryHandler<ListIssueExecutionJournalQuery, Error = ApplicationError>>>,
  journal_events:
    Option<Arc<dyn AuthorizedManagementQueryHandler<ReadIssueExecutionJournalEventsQuery, Error = ApplicationError>>>,
  journal_detail:
    Option<Arc<dyn AuthorizedManagementQueryHandler<GetIssueExecutionJournalEventQuery, Error = ApplicationError>>>,
  publication: Option<Publication>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListIssueExecutionsQuery, Error = ApplicationError>>,
  get: Arc<dyn AuthorizedManagementQueryHandler<GetIssueExecutionQuery, Error = ApplicationError>>,
}
impl IssueExecutionManagementApplication {
  /// Installs already authorized exact request reads.
  pub fn new<Q>(queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<GetIssueExecutionQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListIssueExecutionsQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      journal: None,
      journal_events: None,
      journal_detail: None,
      get: queries.clone(),
      list: queries,
      publication: None,
    }
  }
}
impl IssueExecutionManagementApplication {
  /// Installs optional journal reads without enabling publication or requiring them for manual tasks.
  pub fn with_journal<Q, E>(mut self, queries: Arc<Q>, events: Arc<E>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<ListIssueExecutionJournalQuery, Error = ApplicationError> + 'static,
    E: AuthorizedManagementQueryHandler<ReadIssueExecutionJournalEventsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetIssueExecutionJournalEventQuery, Error = ApplicationError>
      + 'static,
  {
    self.journal = Some(queries);
    self.journal_events = Some(events.clone());
    self.journal_detail = Some(events);
    self
  }
  /// Installs reviewed reservation and the shared publisher with a bounded HTTP wait.
  pub fn with_publication<C, P>(mut self, requests: Arc<C>, publisher: Arc<P>, timeout: Duration) -> Self
  where
    C: AuthorizedManagementCommandHandler<RequestIssueExecutionCommand, Error = ApplicationError> + 'static,
    P: AuthorizedManagementCommandHandler<PublishIssueExecutionCommand, Error = ApplicationError> + 'static,
  {
    self.publication = Some(Publication {
      request: requests,
      publish: publisher,
      timeout,
    });
    self
  }
}
impl ManagementApplication {
  /// Enables execution receipts independently of manual task creation.
  pub fn with_issue_executions(mut self, executions: IssueExecutionManagementApplication) -> Self {
    self.issue_executions = Some(executions);
    self
  }
}
fn endpoints<'a>(
  app: &'a ManagementApplication,
  id: &RequestId,
) -> Result<&'a IssueExecutionManagementApplication, ApiError> {
  app
    .issue_executions
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
fn representation(request: &IssueExecutionRequest, id: &RequestId) -> Result<Value, ApiError> {
  let mut value = encode(request, id)?;
  value["state"] = json!(if request.association.is_some() {
    "admitted"
  } else {
    "pending"
  });
  value["run_url"] = json!(
    request
      .association
      .as_ref()
      .map(|a| format!("/api/v1/factory-runs/{}", a.run_id))
  );
  Ok(value)
}
pub(super) async fn get(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, request)): Path<(String, String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let request = endpoints(&app, &id)?
    .get
    .handle_authorized_query(
      &context,
      GetIssueExecutionQuery {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
        request_id: request.parse().map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(representation(&request, &id)?)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StartRequest {
  target: octacity_server_application::IssueExecutionTarget,
  classification: Value,
}
pub(super) async fn start(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<StartRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let body = body(payload, &id)?;
  let intent: octacity_server_application::RequestIssueExecutionIntent = serde_json::from_value(json!({
    "project_id":project,"issue_id":issue,"expected_version":expected.version(),
    "target":body.target,"classification":body.classification
  }))
  .map_err(|_| invalid())?;
  let classification = intent.classification.clone().ok_or_else(invalid)?;
  let services = endpoints(&app, &id)?
    .publication
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let outcome = services
    .request
    .handle_authorized_command(
      &context,
      RequestIssueExecutionCommand {
        intent,
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
        requested_at: octacity_server_application::Timestamp::from_unix_millis(now_unix_ms(&id)?)
          .map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let command = PublishIssueExecutionCommand {
    project_id: outcome.request.intent.project_id,
    issue_id: outcome.request.intent.issue_id,
    request_id: outcome.request.id,
    classification,
  };
  let publication = tokio::time::timeout(
    services.timeout,
    services.publish.handle_authorized_command(&context, command),
  )
  .await;
  let request = match publication {
    Ok(Ok(request)) => request,
    Ok(Err(error))
      if !error.application().is_some_and(|e| {
        matches!(
          e.classification(),
          ApplicationFailure::Unavailable | ApplicationFailure::Conflict
        )
      }) =>
    {
      return Err(authorized_handler_error(error, &id));
    }
    // Publication may already have linked before its acknowledgement was lost.
    // Read current authority rather than returning the original unassociated reservation snapshot.
    _ => endpoints(&app, &id)?
      .get
      .handle_authorized_query(
        &context,
        GetIssueExecutionQuery {
          project_id: outcome.request.intent.project_id,
          issue_id: outcome.request.intent.issue_id,
          request_id: outcome.request.id,
        },
      )
      .await
      .map_err(|e| authorized_handler_error(e, &id))?,
  };
  let status = if request.association.is_none() {
    StatusCode::ACCEPTED
  } else if outcome.disposition == octacity_server_application::StoreMutationDisposition::Applied {
    StatusCode::CREATED
  } else {
    StatusCode::OK
  };
  Ok(
    (
      status,
      [
        ("cache-control", "no-store".to_owned()),
        (
          "location",
          format!("/api/v1/projects/{project}/issues/{issue}/executions/{}", request.id),
        ),
      ],
      Json(json!({"disposition":encode(outcome.disposition,&id)?,"resource":representation(&request,&id)?})),
    )
      .into_response(),
  )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryParameters {
  after: Option<String>,
  limit: Option<u16>,
}
pub(super) async fn list(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<HistoryParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(parameters) = parameters.map_err(|_| invalid())?;
  let query = ListIssueExecutionsQuery::try_new(
    project.parse().map_err(|_| invalid())?,
    issue.parse().map_err(|_| invalid())?,
    parameters.after,
    parameters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let page = endpoints(&app, &id)?
    .list
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let items = page
    .items
    .iter()
    .map(|r| representation(r, &id))
    .collect::<Result<Vec<_>, _>>()?;
  Ok(
    (
      [("cache-control", "no-store")],
      Json(json!({"items":items,"next_cursor":page.next_cursor,
    "observation":page.observation,"watermark":page.watermark,"issue_version":page.issue_version})),
    )
      .into_response(),
  )
}
