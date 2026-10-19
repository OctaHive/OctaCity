//! Provider-neutral planning HTTP translation; application handlers own authority.
use super::*;
use octacity_server_application::{GetIssuePropertyChoicesQuery, PublishedIssueProperties};
fn definitions(value: PublishedIssueProperties, id: &RequestId) -> Result<Value, ApiError> {
  Ok(json!({"project_id":value.project_id,"version":value.version,
    "definition":encode(value.definition,id)?,"published_at_unix_ms":value.published_at.unix_millis()}))
}
pub(in super::super) async fn choices(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
) -> Result<axum::response::Response, ApiError> {
  let result = endpoints(&app, &id)?
    .choices
    .handle_authorized_query(
      &context,
      GetIssuePropertyChoicesQuery {
        project_id: project
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let items: Vec<_> = result
    .assignees
    .items
    .into_iter()
    .map(|p| json!({"id":p.id,"name":p.name}))
    .collect();
  Ok(([("cache-control","no-store")],Json(json!({"definitions":definitions(result.definitions,&id)?,"assignees":{"available":result.assignees.available,"items":items}}))).into_response())
}

pub(in super::super) async fn current(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
) -> Result<axum::response::Response, ApiError> {
  read(&app, &context, &id, &project, None).await
}
pub(in super::super) async fn version(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, version)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  read(
    &app,
    &context,
    &id,
    &project,
    Some(
      version
        .parse()
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    ),
  )
  .await
}
async fn read(
  app: &ManagementApplication,
  context: &octacity_server_application::ManagementRequestContext,
  id: &RequestId,
  project: &str,
  version: Option<u64>,
) -> Result<axum::response::Response, ApiError> {
  let result = endpoints(app, id)?
    .definitions
    .handle_authorized_query(
      context,
      octacity_server_application::GetIssuePropertiesQuery {
        project_id: project
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, id))?,
        version,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, id))?;
  let version = result.version;
  Ok(response(definitions(result, id)?, version, StatusCode::OK))
}
pub(in super::super) async fn publish(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::issue::PublishIssuePropertiesRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let key = idempotency_key(&headers, &id)?;
  let body = body(payload, &id)?;
  let result = endpoints(&app, &id)?
    .publish_properties
    .handle_authorized_command(
      &context,
      octacity_server_application::PublishIssuePropertiesCommand {
        intent: octacity_server_application::PublishIssuePropertiesIntent {
          project_id: project
            .parse()
            .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
          expected_version: body.expected_version,
          definition: body.definition,
        },
        idempotency_key: key
          .as_str()
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        published_at: Timestamp::from_unix_millis(now_unix_ms(&id)?)
          .map_err(|_| application_error(ApplicationFailure::Internal, &id))?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let version = result.resource.version;
  Ok(response(
    json!({"disposition":encode(result.disposition,&id)?,"resource":definitions(result.resource,&id)?}),
    version,
    StatusCode::OK,
  ))
}

pub(in super::super) async fn edit(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<octacity_server_application::IssuePlanning>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let planning = body(payload, &id)?;
  let result = endpoints(&app, &id)?
    .edit_planning
    .handle_authorized_command(
      &context,
      octacity_server_application::EditIssuePlanningCommand {
        intent: octacity_server_application::EditIssuePlanningIntent {
          project_id: project
            .parse()
            .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
          id: issue
            .parse()
            .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
          expected_version: IssueVersion::new(expected.version())
            .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
          planning,
        },
        idempotency_key: key
          .as_str()
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        edited_at: Timestamp::from_unix_millis(now_unix_ms(&id)?)
          .map_err(|_| application_error(ApplicationFailure::Internal, &id))?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(response(
    json!({"disposition":encode(result.disposition,&id)?,"resource":representation(&result.issue,&id)?}),
    result.issue.version.get(),
    StatusCode::OK,
  ))
}

pub(in super::super) async fn children(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<super::super::definition_discovery::CurrentDefinitionListParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|e| query_error(e, &id))?;
  let query = octacity_server_application::ListIssueChildrenQuery::try_new(
    project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    issue
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
    .children
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  issue_page(page, &id)
}
