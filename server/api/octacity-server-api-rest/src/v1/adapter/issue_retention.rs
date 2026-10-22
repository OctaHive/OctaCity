//! Thin administrative contracts; ordinary task editing cannot release a hold.
use super::*;
use octacity_server_application::{
  GetIssueContentRetentionQuery, IssueContentRetentionKind, IssueContentRetentionPlan, SetIssueContentRetentionCommand,
};
pub(super) struct Endpoints {
  get: Arc<dyn AuthorizedManagementQueryHandler<GetIssueContentRetentionQuery, Error = ApplicationError>>,
  set: Arc<dyn AuthorizedManagementCommandHandler<SetIssueContentRetentionCommand, Error = ApplicationError>>,
}
impl IssueManagementApplication {
  /// Installs independent administrative lifetimes without adding a manual-creation dependency.
  pub fn with_retention<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<SetIssueContentRetentionCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<GetIssueContentRetentionQuery, Error = ApplicationError> + 'static,
  {
    self.retention = Some(Endpoints {
      get: queries,
      set: commands,
    });
    self
  }
}
fn kind(value: &str, id: &RequestId) -> Result<IssueContentRetentionKind, ApiError> {
  match value {
    "requirements" => Ok(IssueContentRetentionKind::Requirements),
    "discussion" => Ok(IssueContentRetentionKind::Discussion),
    _ => Err(application_error(ApplicationFailure::Invalid, id)),
  }
}
fn endpoints<'a>(app: &'a ManagementApplication, id: &RequestId) -> Result<&'a Endpoints, ApiError> {
  super::endpoints(app, id)?
    .retention
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
pub(in crate::v1::adapter) async fn get(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, selector)): Path<(String, String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let project_id = project.parse().map_err(|_| invalid())?;
  let issue_id = issue.parse().map_err(|_| invalid())?;
  let state = endpoints(&app, &id)?
    .get
    .handle_authorized_query(
      &context,
      GetIssueContentRetentionQuery {
        project_id,
        issue_id,
        kind: kind(&selector, &id)?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let value = match state {
    Some(state) => encode(&state, &id)?,
    None => json!({"project_id":project_id,"issue_id":issue_id,"version":0,"plan":null}),
  };
  Ok(([("cache-control", "no-store")], Json(value)).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::v1::adapter) struct SetRequest {
  expected_version: u64,
  plan: IssueContentRetentionPlan,
}
pub(in crate::v1::adapter) async fn set(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, selector)): Path<(String, String, String)>,
  headers: HeaderMap,
  payload: Result<Json<SetRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let key = idempotency_key(&headers, &id)?;
  let request = body(payload, &id)?;
  if request.plan.scope.kind() != kind(&selector, &id)? {
    return Err(invalid());
  }
  request.plan.validate().map_err(|_| invalid())?;
  let state = endpoints(&app, &id)?
    .set
    .handle_authorized_command(
      &context,
      SetIssueContentRetentionCommand {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
        expected_version: request.expected_version,
        plan: request.plan,
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
        configured_at: Timestamp::from_unix_millis(now_unix_ms(&id)?).map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(&state, &id)?)).into_response())
}
