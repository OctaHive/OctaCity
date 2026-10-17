//! Scoped grant commands use the same protected ingress and publication headers as connections.
use super::*;
use octacity_server_application::{
  IntegrationProjectAccessIntent, PublishIntegrationProjectAccess, PublishedIntegrationProjectAccess,
};
fn representation(access: PublishedIntegrationProjectAccess, request_id: &RequestId) -> Result<Value, ApiError> {
  Ok(
    json!({"connection_id":access.connection_id.to_string(),"project_id":access.project_id.to_string(),
    "connection_version":access.connection_version.get(),"version":access.version.get(),"resources":encode(access.resources,request_id)?,
    "published_at_unix_ms":access.published_at.unix_millis()}),
  )
}
pub(crate) async fn publish_project_access(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((id, project_id)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<SafeIntent>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let body = body(payload, &request_id)?;
  let intent: IntegrationProjectAccessIntent =
    serde_json::from_value(body.intent).map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?;
  if intent.connection_id.to_string() != id || intent.project_id.to_string() != project_id {
    return Err(application_error(ApplicationFailure::Invalid, &request_id));
  }
  check_precondition(
    &headers,
    intent.expected_current_version.map_or(0, |v| v.get()),
    &request_id,
  )?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = PublishIntegrationProjectAccessCommand {
    request: PublishIntegrationProjectAccess {
      intent,
      idempotency_key: key
        .as_str()
        .parse()
        .map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?,
      published_at: Timestamp::from_unix_millis(now_unix_ms(&request_id)?)
        .map_err(|_| application_error(ApplicationFailure::Internal, &request_id))?,
    },
  };
  let services = application
    .project_access
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &request_id))?;
  let outcome = services
    .publish
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &request_id))?;
  Ok(Json(
    json!({"disposition":encode(outcome.disposition,&request_id)?,"access":representation(outcome.access,&request_id)?}),
  ))
}
pub(crate) async fn get_project_access(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((id, project_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
  let query = GetIntegrationProjectAccessQuery {
    connection_id: id
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?,
    project_id: project_id
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?,
  };
  let services = application
    .project_access
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &request_id))?;
  let access = services
    .get
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &request_id))?;
  Ok(Json(
    json!({"access":access.map(|a| representation(a,&request_id)).transpose()?}),
  ))
}
pub(crate) async fn list_project_resources(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((id, project_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
  let query = ListIntegrationProjectResourcesQuery {
    connection_id: id
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?,
    project_id: project_id
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?,
  };
  let services = application
    .project_access
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &request_id))?;
  let resources = services
    .list
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &request_id))?;
  let resources = resources
    .into_iter()
    .map(|r| {
      let capabilities = r
        .capabilities
        .into_iter()
        .map(|c| json!({"purpose":c.purpose,"capability":c.capability,"configuration":c.configuration}))
        .collect::<Vec<_>>();
      json!({"reference":r.reference,"native_id":r.native_id,"locator":r.locator,"capabilities":capabilities})
    })
    .collect::<Vec<_>>();
  Ok(Json(json!({"resources":resources})))
}
