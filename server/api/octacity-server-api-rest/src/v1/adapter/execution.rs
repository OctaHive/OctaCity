use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectBuildListParameters {
  configuration_id: Option<String>,
  state: Option<String>,
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}

pub(super) async fn list_project_builds(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project_id): Path<String>,
  parameters: Result<Query<ProjectBuildListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_project_builds(
      &project_id,
      parameters.configuration_id.as_deref(),
      parameters.state.as_deref(),
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .builds
    .list
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(build_summary_page(page, &request_id)?)))
}

pub(super) async fn get_build(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(build_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_build(&build_id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .builds
    .get_build
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(build_resource(projection, &request_id)?)))
}

pub(super) async fn get_attempt(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(attempt_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_attempt(&attempt_id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .builds
    .get_attempt
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(attempt_resource(projection, &request_id)?)))
}

pub(super) async fn get_job(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(job_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_job(&job_id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .builds
    .get_job
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(job_resource(projection, &request_id)?)))
}

pub(super) async fn cancel_build(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
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
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(cancel_build_response(outcome))))
}

pub(super) async fn retry_build(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
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
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
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
    state: build_state_resource(build.state),
    version: build.version.get(),
    trigger: encode(build.trigger, request_id)?,
    created_at_unix_ms: build.created_at.unix_millis(),
    updated_at_unix_ms: build.updated_at.unix_millis(),
    current_attempt: attempt_summary(projection.current_attempt),
  })
}

fn build_summary_page(
  page: octacity_server_application::BuildPageProjection,
  request_id: &RequestId,
) -> Result<BuildSummaryPage, ApiError> {
  cursor_page(
    page
      .items
      .into_iter()
      .map(|summary| build_summary(summary, request_id))
      .collect::<Result<_, _>>()?,
    page
      .next_cursor
      .map(octacity_server_application::BuildPageCursor::encode),
    request_id,
  )
}

fn build_summary(
  projection: octacity_server_application::BuildSummaryProjection,
  request_id: &RequestId,
) -> Result<BuildSummaryResource, ApiError> {
  Ok(BuildSummaryResource {
    id: projection.id.to_string(),
    project_id: projection.project_id.to_string(),
    configuration_id: projection.configuration_id.to_string(),
    configuration_version: projection.configuration_version.get(),
    cause: trigger_cause_resource(projection.cause, request_id)?,
    state: build_state_resource(projection.state),
    created_at_unix_ms: projection.created_at.unix_millis(),
    current_attempt_id: projection.current_attempt_id.to_string(),
    current_attempt_number: projection.current_attempt_number.get(),
    current_attempt_state: attempt_state_resource(projection.current_attempt_state),
    terminal_at_unix_ms: projection
      .terminal_at
      .map(octacity_server_application::Timestamp::unix_millis),
  })
}

fn trigger_cause_resource(
  projection: octacity_server_application::TriggerCauseProjection,
  request_id: &RequestId,
) -> Result<TriggerCauseResource, ApiError> {
  use octacity_server_application::TriggerCauseProjection;

  Ok(match projection {
    TriggerCauseProjection::Manual => TriggerCauseResource::Manual,
    TriggerCauseProjection::Scheduled => TriggerCauseResource::Scheduled,
    TriggerCauseProjection::External {
      integration_id,
      repository_id,
      event_kind,
      reference,
      revision,
    } => TriggerCauseResource::External {
      integration_id: integration_id.to_string(),
      repository_id: repository_id.to_string(),
      event_kind: enum_name(event_kind, request_id)?,
      reference: reference.map(|value| value.to_string()),
      revision: revision.map(|value| value.to_string()),
    },
    TriggerCauseProjection::Internal {
      source_build_id,
      event_kind,
    } => TriggerCauseResource::Internal {
      source_build_id: source_build_id.to_string(),
      event_kind: enum_name(event_kind, request_id)?,
    },
  })
}

fn attempt_resource(projection: AttemptDetailsProjection, request_id: &RequestId) -> Result<AttemptResource, ApiError> {
  Ok(AttemptResource {
    attempt: attempt_summary(projection.attempt),
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

fn attempt_summary(projection: octacity_server_application::AttemptProjection) -> AttemptSummaryResource {
  AttemptSummaryResource {
    id: projection.id.to_string(),
    build_id: projection.build_id.to_string(),
    number: projection.number.get(),
    retry_of_attempt_id: projection.retry_of_attempt_id.map(|id| id.to_string()),
    state: attempt_state_resource(projection.state),
    version: projection.version.get(),
    created_at_unix_ms: projection.created_at.unix_millis(),
    updated_at_unix_ms: projection.updated_at.unix_millis(),
  }
}

fn build_state_resource(state: BuildState) -> BuildStateResource {
  match state {
    BuildState::Queued => BuildStateResource::Queued,
    BuildState::Running => BuildStateResource::Running,
    BuildState::Succeeded => BuildStateResource::Succeeded,
    BuildState::Failed => BuildStateResource::Failed,
    BuildState::Cancelled => BuildStateResource::Cancelled,
  }
}

fn attempt_state_resource(state: AttemptState) -> AttemptStateResource {
  match state {
    AttemptState::Created => AttemptStateResource::Created,
    AttemptState::Running => AttemptStateResource::Running,
    AttemptState::Succeeded => AttemptStateResource::Succeeded,
    AttemptState::Failed => AttemptStateResource::Failed,
    AttemptState::Cancelled => AttemptStateResource::Cancelled,
  }
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
