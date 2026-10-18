//! Project business workflow routes are independent of Factory and provider setup.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler, GetDefaultIssueWorkflowQuery,
  GetIssueWorkflowQuery, IssueWorkflowSelectionVersion, IssueWorkflowVersion, ListProjectIssueWorkflowsQuery,
  PublishIssueWorkflowCommand, PublishIssueWorkflowIntent, SelectDefaultIssueWorkflowCommand,
  SelectDefaultIssueWorkflowIntent, ValidateIssueWorkflowQuery,
};

/// Explicitly composed workflow services, without an integration/runtime dependency.
pub struct IssueWorkflowManagementApplication {
  default: Arc<dyn AuthorizedManagementQueryHandler<GetDefaultIssueWorkflowQuery, Error = ApplicationError>>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListProjectIssueWorkflowsQuery, Error = ApplicationError>>,
  get: Arc<dyn AuthorizedManagementQueryHandler<GetIssueWorkflowQuery, Error = ApplicationError>>,
  validate: Arc<dyn AuthorizedManagementQueryHandler<ValidateIssueWorkflowQuery, Error = ApplicationError>>,
  select: Arc<dyn AuthorizedManagementCommandHandler<SelectDefaultIssueWorkflowCommand, Error = ApplicationError>>,
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishIssueWorkflowCommand, Error = ApplicationError>>,
}
impl IssueWorkflowManagementApplication {
  /// Installs authorized workflow handlers supplied by the composition root.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishIssueWorkflowCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<SelectDefaultIssueWorkflowCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<GetDefaultIssueWorkflowQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListProjectIssueWorkflowsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetIssueWorkflowQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ValidateIssueWorkflowQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      default: queries.clone(),
      list: queries.clone(),
      get: queries.clone(),
      validate: queries,
      select: commands.clone(),
      publish: commands,
    }
  }
}
pub(super) async fn get_current(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, workflow)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  read_workflow(&app, &context, &id, &project, &workflow, None).await
}
pub(super) async fn get_version(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, workflow, version)): Path<(String, String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let version = version
    .parse()
    .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  read_workflow(&app, &context, &id, &project, &workflow, Some(version)).await
}
async fn read_workflow(
  app: &ManagementApplication,
  context: &octacity_server_application::ManagementRequestContext,
  id: &RequestId,
  project: &str,
  workflow: &str,
  version: Option<u64>,
) -> Result<axum::response::Response, ApiError> {
  let query = GetIssueWorkflowQuery {
    project_id: project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, id))?,
    id: workflow
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, id))?,
    version: version
      .map(IssueWorkflowVersion::new)
      .transpose()
      .map_err(|_| application_error(ApplicationFailure::Invalid, id))?,
  };
  let endpoints = app
    .issue_workflows
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))?;
  let result = endpoints
    .get
    .handle_authorized_query(context, query)
    .await
    .map_err(|e| authorized_handler_error(e, id))?;
  Ok(
    (
      [("etag", format!("\"{}\"", result.version.get()))],
      Json(representation(result, id)?),
    )
      .into_response(),
  )
}
pub(super) async fn publish(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, workflow)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::IssueWorkflowDefinitionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let expected = precondition(&headers, &id)?;
  let body = body(payload, &id)?;
  let intent = PublishIssueWorkflowIntent {
    project_id: project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    id: workflow
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    expected_current_version: Some(
      IssueWorkflowVersion::new(expected.version()).map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    ),
    definition: serde_json::from_value(body.definition)
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  publish_intent(&app, &context, &id, &headers, intent, StatusCode::OK).await
}
pub(super) async fn create(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::CreateIssueWorkflowRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &id)?;
  let intent = PublishIssueWorkflowIntent {
    project_id: project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    id: body
      .id
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    expected_current_version: None,
    definition: serde_json::from_value(body.definition)
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  publish_intent(&app, &context, &id, &headers, intent, StatusCode::CREATED).await
}
async fn publish_intent(
  app: &ManagementApplication,
  context: &octacity_server_application::ManagementRequestContext,
  id: &RequestId,
  headers: &HeaderMap,
  intent: PublishIssueWorkflowIntent,
  status: StatusCode,
) -> Result<axum::response::Response, ApiError> {
  let key = idempotency_key(headers, id)?;
  let endpoints = app
    .issue_workflows
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))?;
  let outcome = endpoints
    .publish
    .handle_authorized_command(
      context,
      PublishIssueWorkflowCommand {
        intent,
        idempotency_key: key
          .as_str()
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, id))?,
        published_at: octacity_server_application::Timestamp::from_unix_millis(now_unix_ms(id)?)
          .map_err(|_| application_error(ApplicationFailure::Internal, id))?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, id))?;
  let version = outcome.workflow.version.get();
  Ok(
    (
      status,
      [("etag", format!("\"{version}\""))],
      Json(json!({"disposition":encode(outcome.disposition,id)?,"resource":representation(outcome.workflow,id)?})),
    )
      .into_response(),
  )
}
pub(super) async fn validate(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  payload: Result<Json<crate::v1::IssueWorkflowDefinitionRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let body = body(payload, &id)?;
  let project_id = project
    .parse()
    .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let definition =
    serde_json::from_value(body.definition).map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let endpoints = app
    .issue_workflows
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let accepted = endpoints
    .validate
    .handle_authorized_query(&context, ValidateIssueWorkflowQuery { project_id, definition })
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(encode(accepted, &id)?))
}
impl ManagementApplication {
  /// Enables Project workflow endpoints independently of optional Factory/integration services.
  pub fn with_issue_workflows(mut self, workflows: IssueWorkflowManagementApplication) -> Self {
    self.issue_workflows = Some(workflows);
    self
  }
}
pub(super) async fn get_default(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let project_id = project
    .parse()
    .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let endpoints = app
    .issue_workflows
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = endpoints
    .default
    .handle_authorized_query(&context, GetDefaultIssueWorkflowQuery { project_id })
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let body = json!({"version":result.version.get(), "workflow":representation(result.workflow, &id)?});
  Ok(([("etag", format!("\"{}\"", result.version.get()))], Json(body)))
}
fn representation(
  workflow: octacity_server_application::PublishedIssueWorkflow,
  id: &RequestId,
) -> Result<Value, ApiError> {
  let definition = encode(workflow.definition, id)?;
  Ok(
    json!({"project_id":workflow.project_id.to_string(),"id":workflow.id.to_string(),"version":workflow.version.get(),
    "definition":definition,"published_at_unix_ms":workflow.published_at.unix_millis()}),
  )
}

pub(super) async fn select_default(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::SelectDefaultIssueWorkflowRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let key = idempotency_key(&headers, &id)?;
  let expected = precondition(&headers, &id)?;
  let body = body(payload, &id)?;
  let command = SelectDefaultIssueWorkflowCommand {
    intent: SelectDefaultIssueWorkflowIntent {
      project_id: project
        .parse()
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      expected_version: IssueWorkflowSelectionVersion::new(expected.version())
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      workflow_id: body
        .workflow_id
        .parse()
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      workflow_version: IssueWorkflowVersion::new(body.workflow_version)
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    },
    idempotency_key: key
      .as_str()
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  let endpoints = app
    .issue_workflows
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let outcome = endpoints
    .select
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let version = outcome.default.version.get();
  Ok((
    [("etag", format!("\"{version}\""))],
    Json(json!({
      "disposition":encode(outcome.disposition,&id)?,
      "resource":{"version":version,"workflow":representation(outcome.default.workflow,&id)?}
    })),
  ))
}

pub(super) async fn list(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<super::definition_discovery::CurrentDefinitionListParameters>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &id))?;
  let query = ListProjectIssueWorkflowsQuery::try_new(
    project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    parameters
      .after
      .map(|cursor| cursor.as_str().parse())
      .transpose()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    parameters.limit,
  )
  .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let endpoints = app
    .issue_workflows
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let page = endpoints
    .list
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let items: Vec<_> = page
    .items
    .into_iter()
    .map(|item| {
      json!({
        "project_id":item.project_id.to_string(),"id":item.id.to_string(),"version":item.version.get(),
        "name":item.name,"published_at_unix_ms":item.published_at.unix_millis()
      })
    })
    .collect();
  Ok(Json(
    json!({"items":items,"next_cursor":page.next_cursor.map(|id| id.to_string())}),
  ))
}
