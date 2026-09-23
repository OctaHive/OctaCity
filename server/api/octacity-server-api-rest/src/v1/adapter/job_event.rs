use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct JobEventReadParameters {
  #[serde(default)]
  after: u64,
  #[serde(default = "default_job_event_limit")]
  limit: u16,
  #[serde(default)]
  wait_ms: u64,
}

pub(super) async fn read_job_events(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(job_id): Path<String>,
  parameters: Result<Query<JobEventReadParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .read_job_events(
      &job_id,
      parameters.after,
      parameters.limit,
      std::time::Duration::from_millis(parameters.wait_ms),
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .job_events
    .0
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(job_event_page(page))))
}
