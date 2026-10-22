//! Thin task HTTP translation; existing application commands own every mutation.
use super::*;
use octacity_server_application::{
  AdoptIssueWorkflowCommand, AdoptIssueWorkflowIntent, ApplicationError, AuthorizedManagementCommandHandler,
  AuthorizedManagementQueryHandler, CreateIssueCommand, CreateIssueIntent, EditIssueCommand, EditIssueIntent,
  GetIssueQuery, GetIssueTransitionsQuery, IssueContent, IssueRevision, IssueVersion, ListIssueRevisionsQuery,
  ListIssueStatusHistoryQuery, ListProjectIssuesQuery, Timestamp, TransitionIssueCommand, TransitionIssueIntent,
};

/// Task services can be installed without Factory, providers or execution configuration.
pub struct IssueManagementApplication {
  retention: Option<retention::Endpoints>,
  children: Arc<
    dyn AuthorizedManagementQueryHandler<octacity_server_application::ListIssueChildrenQuery, Error = ApplicationError>,
  >,
  edit_planning: Arc<
    dyn AuthorizedManagementCommandHandler<
        octacity_server_application::EditIssuePlanningCommand,
        Error = ApplicationError,
      >,
  >,
  definitions: Arc<
    dyn AuthorizedManagementQueryHandler<octacity_server_application::GetIssuePropertiesQuery, Error = ApplicationError>,
  >,
  publish_properties: Arc<
    dyn AuthorizedManagementCommandHandler<
        octacity_server_application::PublishIssuePropertiesCommand,
        Error = ApplicationError,
      >,
  >,
  choices: Arc<
    dyn AuthorizedManagementQueryHandler<
        octacity_server_application::GetIssuePropertyChoicesQuery,
        Error = ApplicationError,
      >,
  >,
  adopt: Arc<dyn AuthorizedManagementCommandHandler<AdoptIssueWorkflowCommand, Error = ApplicationError>>,
  transition: Arc<dyn AuthorizedManagementCommandHandler<TransitionIssueCommand, Error = ApplicationError>>,
  transitions: Arc<dyn AuthorizedManagementQueryHandler<GetIssueTransitionsQuery, Error = ApplicationError>>,
  history: Arc<dyn AuthorizedManagementQueryHandler<ListIssueStatusHistoryQuery, Error = ApplicationError>>,
  create: Arc<dyn AuthorizedManagementCommandHandler<CreateIssueCommand, Error = ApplicationError>>,
  get: Arc<dyn AuthorizedManagementQueryHandler<GetIssueQuery, Error = ApplicationError>>,
  revisions: Arc<dyn AuthorizedManagementQueryHandler<ListIssueRevisionsQuery, Error = ApplicationError>>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListProjectIssuesQuery, Error = ApplicationError>>,
  edit: Arc<dyn AuthorizedManagementCommandHandler<EditIssueCommand, Error = ApplicationError>>,
}
impl IssueManagementApplication {
  /// Composes already authorized task use cases.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<
        octacity_server_application::PublishIssuePropertiesCommand,
        Error = ApplicationError,
      > + AuthorizedManagementCommandHandler<
        octacity_server_application::EditIssuePlanningCommand,
        Error = ApplicationError,
      > + AuthorizedManagementCommandHandler<CreateIssueCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<EditIssueCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<TransitionIssueCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<AdoptIssueWorkflowCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<
        octacity_server_application::GetIssuePropertyChoicesQuery,
        Error = ApplicationError,
      > + AuthorizedManagementQueryHandler<octacity_server_application::GetIssuePropertiesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<octacity_server_application::ListIssueChildrenQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetIssueQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListProjectIssuesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListIssueRevisionsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetIssueTransitionsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListIssueStatusHistoryQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      retention: None,
      children: queries.clone(),
      edit_planning: commands.clone(),
      definitions: queries.clone(),
      publish_properties: commands.clone(),
      choices: queries.clone(),
      create: commands.clone(),
      edit: commands.clone(),
      transition: commands.clone(),
      adopt: commands,
      transitions: queries.clone(),
      history: queries.clone(),
      get: queries.clone(),
      list: queries.clone(),
      revisions: queries,
    }
  }
}
impl ManagementApplication {
  /// Enables manual task management independently of optional runtimes.
  pub fn with_issues(mut self, issues: IssueManagementApplication) -> Self {
    self.issues = Some(issues);
    self
  }
}
fn endpoints<'a>(app: &'a ManagementApplication, id: &RequestId) -> Result<&'a IssueManagementApplication, ApiError> {
  app
    .issues
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
fn representation(issue: &IssueRevision, id: &RequestId) -> Result<Value, ApiError> {
  let mut value = json!({"id":issue.id.to_string(),"project_id":issue.project_id.to_string(),"number":issue.number,
    "display_key":issue.display_key(),"version":issue.version.get(),"applicability_version":issue.applicability_version.get(),
    "content_available":issue.content_available,"content":encode(&issue.content,id)?,"workflow_id":issue.workflow_id.to_string(),"workflow_version":issue.workflow_version.get(),
    "status_id":issue.status_id.to_string(),"created_at_unix_ms":issue.created_at.unix_millis(),"updated_at_unix_ms":issue.updated_at.unix_millis()});
  if !issue.planning.is_empty() {
    value["planning"] = encode(&issue.planning, id)?;
  }
  Ok(value)
}
fn response(value: Value, version: u64, status: StatusCode) -> axum::response::Response {
  (
    status,
    [("etag", format!("\"{version}\"")), ("cache-control", "no-store".into())],
    Json(value),
  )
    .into_response()
}
pub(super) async fn create(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::CreateIssueRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let key = idempotency_key(&headers, &id)?;
  let request = body(payload, &id)?;
  let command = CreateIssueCommand {
    intent: CreateIssueIntent {
      planning: request.planning,
      project_id: project
        .parse()
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      content: IssueContent {
        title: request.title,
        description: request.description,
        acceptance: request.acceptance,
        priority: request.priority,
      },
    },
    idempotency_key: key
      .as_str()
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    created_at: Timestamp::from_unix_millis(now_unix_ms(&id)?)
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  let outcome = endpoints(&app, &id)?
    .create
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let status = if outcome.disposition == octacity_server_application::StoreMutationDisposition::Applied {
    StatusCode::CREATED
  } else {
    StatusCode::OK
  };
  Ok(response(
    json!({"disposition":encode(outcome.disposition,&id)?,"resource":representation(&outcome.issue,&id)?}),
    outcome.issue.version.get(),
    status,
  ))
}
pub(super) async fn get(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  read(&app, &context, &id, &project, &issue, None).await
}
async fn read(
  app: &ManagementApplication,
  context: &octacity_server_application::ManagementRequestContext,
  id: &RequestId,
  project: &str,
  issue: &str,
  version: Option<u64>,
) -> Result<axum::response::Response, ApiError> {
  let query = GetIssueQuery {
    project_id: project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, id))?,
    id: issue
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, id))?,
    version: version
      .map(IssueVersion::new)
      .transpose()
      .map_err(|_| application_error(ApplicationFailure::Invalid, id))?,
  };
  let issue = endpoints(app, id)?
    .get
    .handle_authorized_query(context, query)
    .await
    .map_err(|e| authorized_handler_error(e, id))?;
  Ok(response(
    representation(&issue, id)?,
    issue.version.get(),
    StatusCode::OK,
  ))
}
pub(super) async fn get_revision(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, version)): Path<(String, String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let version = version
    .parse()
    .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  read(&app, &context, &id, &project, &issue, Some(version)).await
}
pub(super) async fn edit(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<IssueContent>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let content = body(payload, &id)?;
  let command = EditIssueCommand {
    intent: EditIssueIntent {
      project_id: project
        .parse()
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      id: issue
        .parse()
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      expected_version: IssueVersion::new(expected.version())
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      content,
    },
    idempotency_key: key
      .as_str()
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    edited_at: Timestamp::from_unix_millis(now_unix_ms(&id)?)
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  let outcome = endpoints(&app, &id)?
    .edit
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(response(
    json!({"disposition":encode(outcome.disposition,&id)?,"resource":representation(&outcome.issue,&id)?}),
    outcome.issue.version.get(),
    StatusCode::OK,
  ))
}

pub(super) async fn list(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<super::definition_discovery::CurrentDefinitionListParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|e| query_error(e, &id))?;
  let query = ListProjectIssuesQuery::try_new(
    project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    p.after
      .map(|a| a.as_str().parse())
      .transpose()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    p.limit,
  )
  .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let page = endpoints(&app, &id)?
    .list
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  issue_page(page, &id)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueHistoryParameters {
  after: Option<u64>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}
pub(super) async fn revisions(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<IssueHistoryParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|e| query_error(e, &id))?;
  let query = ListIssueRevisionsQuery::try_new(
    project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    issue
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    p.after
      .map(IssueVersion::new)
      .transpose()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    p.limit,
  )
  .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let page = endpoints(&app, &id)?
    .revisions
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(
    (
      [("cache-control", "no-store")],
      Json(json!({"items":encode(page.items,&id)?,"next_cursor":page.next_cursor.map(|v|v.get())})),
    )
      .into_response(),
  )
}

pub(super) async fn transitions(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let query = GetIssueTransitionsQuery {
    project_id: project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    id: issue
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  let review = endpoints(&app, &id)?
    .transitions
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(response(
    encode(&review, &id)?,
    review.issue_version.get(),
    StatusCode::OK,
  ))
}
pub(super) async fn transition(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::TransitionIssueRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let body = body(payload, &id)?;
  let intent:TransitionIssueIntent=serde_json::from_value(json!({"project_id":project,"id":issue,"expected_version":expected.version(),
    "workflow_id":body.workflow_id,"workflow_version":body.workflow_version,"expected_status_id":body.expected_status_id,"transition_id":body.transition_id,"reason":body.reason})).map_err(|_|application_error(ApplicationFailure::Invalid,&id))?;
  let command = TransitionIssueCommand {
    intent,
    idempotency_key: key
      .as_str()
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    moved_at: Timestamp::from_unix_millis(now_unix_ms(&id)?)
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  let outcome = endpoints(&app, &id)?
    .transition
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(response(
    json!({"disposition":encode(outcome.disposition,&id)?,"resource":representation(&outcome.issue,&id)?,"transition":encode(outcome.transition,&id)?}),
    outcome.issue.version.get(),
    StatusCode::OK,
  ))
}
pub(super) async fn history(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<IssueHistoryParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|e| query_error(e, &id))?;
  let query = ListIssueStatusHistoryQuery::try_new(
    project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    issue
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    p.after
      .map(IssueVersion::new)
      .transpose()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    p.limit,
  )
  .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let page = endpoints(&app, &id)?
    .history
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(
    (
      [("cache-control", "no-store")],
      Json(json!({"items":encode(page.items,&id)?,"next_cursor":page.next_cursor.map(|v|v.get())})),
    )
      .into_response(),
  )
}

pub(super) async fn adopt(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::AdoptIssueWorkflowRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let body = body(payload, &id)?;
  let intent:AdoptIssueWorkflowIntent=serde_json::from_value(json!({"project_id":project,"id":issue,"expected_version":expected.version(),
    "expected_workflow_id":body.expected_workflow_id,"expected_workflow_version":body.expected_workflow_version,"expected_status_id":body.expected_status_id,
    "target_workflow_id":body.target_workflow_id,"target_workflow_version":body.target_workflow_version,"status_correspondence":body.status_correspondence,"expected_field_policy":body.expected_field_policy})).map_err(|_|application_error(ApplicationFailure::Invalid,&id))?;
  let command = AdoptIssueWorkflowCommand {
    intent,
    idempotency_key: key
      .as_str()
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    adopted_at: Timestamp::from_unix_millis(now_unix_ms(&id)?)
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  let outcome = endpoints(&app, &id)?
    .adopt
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(response(
    json!({"disposition":encode(outcome.disposition,&id)?,"resource":representation(&outcome.issue,&id)?,"transition":encode(outcome.transition,&id)?}),
    outcome.issue.version.get(),
    StatusCode::OK,
  ))
}

#[path = "issue/planning.rs"]
pub(super) mod planning;

fn issue_page(
  page: octacity_server_application::IssuePage,
  id: &RequestId,
) -> Result<axum::response::Response, ApiError> {
  Ok(
    (
      [("cache-control", "no-store")],
      Json(json!({"items":encode(page.items,id)?,"next_cursor":page.next_cursor.map(|i|i.to_string())})),
    )
      .into_response(),
  )
}

#[path = "issue_retention.rs"]
pub(super) mod retention;
