use axum::{
  Json,
  body::Body,
  extract::{FromRequest, Path, Request, State},
  http::StatusCode,
  response::{IntoResponse, Response},
};
use octacity_protocol::{BeginOutputUploadRequest, CompleteOutputUploadRequest};
use octacity_server_application::{
  AgentArtifactError, BeginAgentArtifactUploadInput, CompleteAgentArtifactUploadInput,
};

use super::{AgentState, operation_context, operation_context_error, protocol_error, rejection_response};

pub(super) async fn begin_upload(
  State(state): State<AgentState>,
  Path(lease_id): Path<String>,
  request: Request<Body>,
) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<BeginOutputUploadRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match state
    .artifacts
    .begin_upload(BeginAgentArtifactUploadInput {
      route_lease_id: lease_id,
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(response) => Json(response).into_response(),
    Err(error) => artifact_error(&request_id, error),
  }
}

pub(super) async fn complete_upload(
  State(state): State<AgentState>,
  Path(lease_id): Path<String>,
  request: Request<Body>,
) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<CompleteOutputUploadRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match state
    .artifacts
    .complete_upload(CompleteAgentArtifactUploadInput {
      route_lease_id: lease_id,
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(response) => Json(response).into_response(),
    Err(error) => artifact_error(&request_id, error),
  }
}

fn artifact_error(request_id: &str, error: AgentArtifactError) -> Response {
  match error {
    AgentArtifactError::InvalidRequest => protocol_error(
      StatusCode::BAD_REQUEST,
      request_id,
      "invalid_request",
      "Artifact request is invalid",
      false,
    ),
    AgentArtifactError::CredentialRejected => protocol_error(
      StatusCode::UNAUTHORIZED,
      request_id,
      "credential_rejected",
      "agent credential rejected",
      false,
    ),
    AgentArtifactError::Fenced => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_fenced",
      "lease is no longer current",
      false,
    ),
    AgentArtifactError::Expired => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_expired",
      "lease has expired",
      false,
    ),
    AgentArtifactError::Integrity => protocol_error(
      StatusCode::UNPROCESSABLE_ENTITY,
      request_id,
      "artifact_integrity",
      "uploaded bytes do not match declared content identity",
      false,
    ),
    AgentArtifactError::Conflict => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "idempotency_conflict",
      "Artifact request conflicts with durable state",
      false,
    ),
    AgentArtifactError::NotFound => protocol_error(
      StatusCode::NOT_FOUND,
      request_id,
      "unknown_upload",
      "Artifact upload was not found",
      false,
    ),
    AgentArtifactError::Unavailable => protocol_error(
      StatusCode::SERVICE_UNAVAILABLE,
      request_id,
      "unavailable",
      "Artifact service unavailable",
      true,
    ),
  }
}
