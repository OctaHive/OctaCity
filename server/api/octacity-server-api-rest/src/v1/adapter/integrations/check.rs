//! Read-only diagnostics accept only an existing metadata selection, never credentials or endpoints.
use super::*;
use octacity_server_application::IntegrationPurpose;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckSelection {
  version: u64,
  resource: Value,
  purpose: IntegrationPurpose,
}

pub(crate) async fn check_connection(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
  payload: Result<Json<CheckSelection>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let selection = body(payload, &request_id)?;
  let invalid = || application_error(ApplicationFailure::Invalid, &request_id);
  let query = CheckIntegrationConnectionQuery {
    id: id.parse().map_err(|_| invalid())?,
    version: serde_json::from_value(json!(selection.version)).map_err(|_| invalid())?,
    resource: serde_json::from_value(selection.resource).map_err(|_| invalid())?,
    purpose: selection.purpose,
  };
  let service = application
    .checks
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &request_id))?;
  let outcome = service
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok(Json(encode(outcome, &request_id)?))
}
