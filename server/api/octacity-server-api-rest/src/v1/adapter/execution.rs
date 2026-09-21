use super::*;

pub(super) async fn get_build(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(build_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_build(&build_id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .builds
    .get_build
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(build_resource(projection, &request_id)?)))
}

pub(super) async fn get_attempt(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(attempt_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_attempt(&attempt_id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .builds
    .get_attempt
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(attempt_resource(projection, &request_id)?)))
}

pub(super) async fn get_job(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(job_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_job(&job_id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .builds
    .get_job
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(job_resource(projection, &request_id)?)))
}

pub(super) async fn cancel_build(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(build_id): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .cancel_build(&build_id, key.as_str(), now_unix_ms(&request_id)?)
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .builds
    .cancel
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(cancel_build_response(outcome))))
}

pub(super) async fn retry_build(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(build_id): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .retry_build(&build_id, key.as_str(), now_unix_ms(&request_id)?)
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .builds
    .retry
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::CREATED, Json(retry_build_response(outcome))))
}

fn build_resource(projection: BuildDetailsProjection, request_id: &RequestId) -> Result<BuildResource, ApiError> {
  let build = projection.build;
  Ok(BuildResource {
    id: build.id.to_string(),
    project_id: build.project_id.to_string(),
    configuration_id: build.configuration_id.to_string(),
    configuration_version: build.configuration_version.get(),
    pipeline_id: build.pipeline_id.to_string(),
    pipeline_version: build.pipeline_version.get(),
    repository_id: build.repository_id.to_string(),
    repository_version: build.repository_version.get(),
    immutable_revision: build.immutable_revision.to_string(),
    parameters: encode(build.parameters, request_id)?,
    source: encode(build.source, request_id)?,
    effective_policy: encode(build.effective_policy, request_id)?,
    priority: build.priority,
    state: enum_name(build.state, request_id)?,
    version: build.version.get(),
    trigger: encode(build.trigger, request_id)?,
    created_at_unix_ms: build.created_at.unix_millis(),
    updated_at_unix_ms: build.updated_at.unix_millis(),
    current_attempt: attempt_summary(projection.current_attempt, request_id)?,
  })
}

fn attempt_resource(projection: AttemptDetailsProjection, request_id: &RequestId) -> Result<AttemptResource, ApiError> {
  Ok(AttemptResource {
    attempt: attempt_summary(projection.attempt, request_id)?,
    jobs: projection
      .jobs
      .into_iter()
      .map(|job| job_resource(job, request_id))
      .collect::<Result<_, _>>()?,
    edges: projection
      .dag
      .edges
      .into_iter()
      .map(|edge| {
        Ok(DagEdgeResource {
          predecessor_job_id: edge.predecessor_job_id.to_string(),
          dependent_job_id: edge.dependent_job_id.to_string(),
          dependency_policy: encode(edge.dependency_policy, request_id)?,
        })
      })
      .collect::<Result<_, ApiError>>()?,
  })
}

fn attempt_summary(
  projection: octacity_server_application::AttemptProjection,
  request_id: &RequestId,
) -> Result<AttemptSummaryResource, ApiError> {
  Ok(AttemptSummaryResource {
    id: projection.id.to_string(),
    build_id: projection.build_id.to_string(),
    number: projection.number.get(),
    retry_of_attempt_id: projection.retry_of_attempt_id.map(|id| id.to_string()),
    state: enum_name(projection.state, request_id)?,
    version: projection.version.get(),
    created_at_unix_ms: projection.created_at.unix_millis(),
    updated_at_unix_ms: projection.updated_at.unix_millis(),
  })
}

fn job_resource(projection: JobProjection, request_id: &RequestId) -> Result<JobResource, ApiError> {
  let terminal = projection
    .terminal
    .map(|terminal| {
      Ok(JobTerminalResource {
        state: enum_name(terminal.state(), request_id)?,
        failure_classification: terminal
          .failure()
          .map(|failure| enum_name(failure, request_id))
          .transpose()?,
        completed_at_unix_ms: terminal.completed_at().unix_millis(),
      })
    })
    .transpose()?;
  Ok(JobResource {
    id: projection.id.to_string(),
    attempt_id: projection.attempt_id.to_string(),
    pipeline_node_id: projection.pipeline_node_id.to_string(),
    dependency_job_ids: projection.dependencies.into_iter().map(|id| id.to_string()).collect(),
    dependency_policy: encode(projection.dependency_policy, request_id)?,
    allowed_pool_ids: projection.allowed_pools.into_iter().map(|id| id.to_string()).collect(),
    placement: encode(projection.placement, request_id)?,
    state: enum_name(projection.state, request_id)?,
    version: projection.version.get(),
    created_at_unix_ms: projection.created_at.unix_millis(),
    updated_at_unix_ms: projection.updated_at.unix_millis(),
    queue: projection.queue.map(|queue| JobQueueResource {
      priority: queue.priority,
      enqueued_at_unix_ms: queue.enqueued_at.unix_millis(),
    }),
    assignment: projection.assignment.map(|assignment| JobAssignmentResource {
      selected_pool_id: assignment.selected_pool_id.to_string(),
      assigned_agent_id: assignment.assigned_agent_id.to_string(),
    }),
    terminal,
    event_cursor: projection.event_cursor,
    outputs: projection
      .outputs
      .into_iter()
      .map(|output| encode(output, request_id))
      .collect::<Result<_, _>>()?,
  })
}

fn cancel_build_response(outcome: CancelBuildCommandOutcome) -> CancelBuildResponse {
  CancelBuildResponse {
    disposition: mutation_disposition(outcome.disposition),
    build_id: outcome.build_id.to_string(),
    attempt_id: outcome.attempt_id.to_string(),
    cancelled_job_ids: outcome.cancelled_job_ids.into_iter().map(|id| id.to_string()).collect(),
    cancelling_job_ids: outcome
      .cancelling_job_ids
      .into_iter()
      .map(|id| id.to_string())
      .collect(),
  }
}

fn retry_build_response(outcome: RetryBuildCommandOutcome) -> RetryBuildResponse {
  RetryBuildResponse {
    disposition: mutation_disposition(outcome.disposition),
    build_id: outcome.build_id.to_string(),
    source_attempt_id: outcome.source_attempt_id.to_string(),
    attempt_id: outcome.attempt_id.to_string(),
    attempt_number: outcome.attempt_number,
    ready_job_ids: outcome.ready_job_ids.into_iter().map(|id| id.to_string()).collect(),
  }
}

fn encode(value: impl Serialize, request_id: &RequestId) -> Result<Value, ApiError> {
  serde_json::to_value(value).map_err(|_| internal_conversion(request_id))
}

fn enum_name(value: impl Serialize, request_id: &RequestId) -> Result<String, ApiError> {
  encode(value, request_id)?
    .as_str()
    .map(ToOwned::to_owned)
    .ok_or_else(|| internal_conversion(request_id))
}
