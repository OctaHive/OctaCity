use axum::{
  Json,
  body::Body,
  extract::{FromRequest, Path, Request, State},
  http::StatusCode,
  response::{IntoResponse, Response},
};
use octacity_protocol::{BeginCacheSessionRequest, RevokeCacheSessionRequest};
use octacity_server_application::{AgentCacheSessionError, BeginAgentCacheSessionInput, RevokeAgentCacheSessionInput};

use super::{AgentState, operation_context, operation_context_error, protocol_error, rejection_response};

pub(super) async fn begin_session(
  State(state): State<AgentState>,
  Path(lease_id): Path<String>,
  request: Request<Body>,
) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<BeginCacheSessionRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match state
    .cache
    .begin_session(BeginAgentCacheSessionInput {
      route_lease_id: lease_id,
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(response) => Json(response).into_response(),
    Err(error) => cache_error(&request_id, error),
  }
}

pub(super) async fn revoke_session(
  State(state): State<AgentState>,
  Path(lease_id): Path<String>,
  request: Request<Body>,
) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<RevokeCacheSessionRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match state
    .cache
    .revoke_session(RevokeAgentCacheSessionInput {
      route_lease_id: lease_id,
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(response) => Json(response).into_response(),
    Err(error) => cache_error(&request_id, error),
  }
}

fn cache_error(request_id: &str, error: AgentCacheSessionError) -> Response {
  match error {
    AgentCacheSessionError::InvalidRequest => protocol_error(
      StatusCode::BAD_REQUEST,
      request_id,
      "invalid_request",
      "cache session request is invalid",
      false,
    ),
    AgentCacheSessionError::CredentialRejected => protocol_error(
      StatusCode::UNAUTHORIZED,
      request_id,
      "credential_rejected",
      "agent credential rejected",
      false,
    ),
    AgentCacheSessionError::Fenced => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_fenced",
      "lease is no longer current",
      false,
    ),
    AgentCacheSessionError::Expired => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_expired",
      "lease has expired",
      false,
    ),
    AgentCacheSessionError::Conflict => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "idempotency_conflict",
      "cache session request conflicts with durable state",
      false,
    ),
    AgentCacheSessionError::NotFound => protocol_error(
      StatusCode::NOT_FOUND,
      request_id,
      "unknown_cache_session",
      "cache session was not found",
      false,
    ),
    AgentCacheSessionError::Unavailable => protocol_error(
      StatusCode::SERVICE_UNAVAILABLE,
      request_id,
      "unavailable",
      "cache session service unavailable",
      true,
    ),
  }
}
