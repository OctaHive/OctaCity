use super::*;
use crate::v1::{
  BuildResultHoldResource, BuildResultHoldState, BuildResultRetentionDeadlines, BuildResultRetentionMutationResponse,
  BuildResultRetentionResource, BuildResultVisibility, PlaceBuildResultHoldRequest, RetentionAuditIdentity,
};
use octacity_server_application::{
  BuildResultHoldStateProjection, BuildResultRetentionCommandOutcome, BuildResultRetentionProjection,
};

pub(super) async fn get_build_result_retention(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(build_id): Path<String>,
) -> Result<Json<BuildResultRetentionResource>, ApiError> {
  let query = application
    .inputs
    .get_build_result_retention(&build_id, now_unix_ms(&request_id)?)
    .map_err(|error| invalid_input(error, &request_id))?;
  application
    .retention
    .get
    .handle_query(query)
    .await
    .map(resource)
    .map(Json)
    .map_err(|error| application_error(error.classification(), &request_id))
}

pub(super) async fn place_build_result_hold(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(build_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<PlaceBuildResultHoldRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .place_build_result_hold(
      &build_id,
      body.reason,
      body.expires_at_unix_ms,
      request_id.0.clone(),
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .retention
    .place
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::CREATED, Json(mutation_response(outcome))))
}

pub(super) async fn release_build_result_hold(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(build_id): Path<String>,
  headers: HeaderMap,
) -> Result<Json<BuildResultRetentionMutationResponse>, ApiError> {
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .release_build_result_hold(
      &build_id,
      expected.version(),
      request_id.0.clone(),
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  application
    .retention
    .release
    .handle_command(command)
    .await
    .map(mutation_response)
    .map(Json)
    .map_err(|error| application_error(error.classification(), &request_id))
}

fn mutation_response(outcome: BuildResultRetentionCommandOutcome) -> BuildResultRetentionMutationResponse {
  BuildResultRetentionMutationResponse {
    disposition: mutation_disposition(outcome.disposition),
    retention: resource(outcome.retention),
  }
}

fn resource(value: BuildResultRetentionProjection) -> BuildResultRetentionResource {
  BuildResultRetentionResource {
    build_id: value.build_id.to_string(),
    deadlines: BuildResultRetentionDeadlines {
      metadata_at_unix_ms: value.deadlines.metadata.unix_millis(),
      logs_at_unix_ms: value.deadlines.logs.unix_millis(),
      artifacts_at_unix_ms: value.deadlines.artifacts.unix_millis(),
      reports_at_unix_ms: value.deadlines.reports.unix_millis(),
    },
    visibility: BuildResultVisibility {
      metadata: value.visibility.metadata,
      logs: value.visibility.logs,
      artifacts: value.visibility.artifacts,
      reports: value.visibility.reports,
    },
    hold: value.hold.map(|hold| BuildResultHoldResource {
      version: hold.version.get(),
      reason: hold.reason,
      created_at_unix_ms: hold.created_at.unix_millis(),
      expires_at_unix_ms: hold.expires_at.map(|value| value.unix_millis()),
      released_at_unix_ms: hold.released_at.map(|value| value.unix_millis()),
      state: match hold.state {
        BuildResultHoldStateProjection::Active => BuildResultHoldState::Active,
        BuildResultHoldStateProjection::Released => BuildResultHoldState::Released,
        BuildResultHoldStateProjection::Expired => BuildResultHoldState::Expired,
      },
      creation_audit: RetentionAuditIdentity {
        actor_kind: hold.creation_audit.actor_kind,
        actor_identity: hold.creation_audit.actor_identity,
        request_identity: hold.creation_audit.request_identity,
      },
      release_audit: hold.release_audit.map(|audit| RetentionAuditIdentity {
        actor_kind: audit.actor_kind,
        actor_identity: audit.actor_identity,
        request_identity: audit.request_identity,
      }),
    }),
  }
}
