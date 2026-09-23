use std::sync::Arc;

use axum::{
  Extension, Json,
  extract::{Path, Query, State, rejection::QueryRejection},
};
use octacity_server_application::{
  CacheSessionDiagnosticState, CacheSessionProjection, GetCacheSessionQuery, ListBuildCacheSessionsQuery,
};
use serde::Deserialize;

use super::{
  ApiError, DEFAULT_CACHE_SESSION_LIMIT, ManagementApplication, application_error, now_unix_ms, query_error,
};
use crate::{
  RequestId,
  v1::{CacheSessionPage, CacheSessionResource, CacheSessionState, ErrorCode},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CacheSessionListQuery {
  #[serde(default = "default_limit")]
  limit: u16,
}

const fn default_limit() -> u16 {
  DEFAULT_CACHE_SESSION_LIMIT
}

pub(super) async fn get_cache_session(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(session_id): Path<String>,
) -> Result<Json<CacheSessionResource>, ApiError> {
  let session_id = session_id.parse().map_err(|_| invalid(&request_id))?;
  application
    .cache
    .get
    .handle_query(GetCacheSessionQuery {
      session_id,
      observed_at_unix_ms: now_unix_ms(&request_id)?,
    })
    .await
    .map(resource)
    .map(Json)
    .map_err(|error| application_error(error.classification(), &request_id))
}

pub(super) async fn list_build_cache_sessions(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(build_id): Path<String>,
  query: Result<Query<CacheSessionListQuery>, QueryRejection>,
) -> Result<Json<CacheSessionPage>, ApiError> {
  let build_id = build_id.parse().map_err(|_| invalid(&request_id))?;
  let Query(query) = query.map_err(|error| query_error(error, &request_id))?;
  application
    .cache
    .list
    .handle_query(ListBuildCacheSessionsQuery {
      build_id,
      limit: query.limit,
      observed_at_unix_ms: now_unix_ms(&request_id)?,
    })
    .await
    .map(|items| CacheSessionPage {
      items: items.into_iter().map(resource).collect(),
    })
    .map(Json)
    .map_err(|error| application_error(error.classification(), &request_id))
}

fn resource(value: CacheSessionProjection) -> CacheSessionResource {
  CacheSessionResource {
    id: value.id.to_string(),
    project_id: value.project_id.to_string(),
    build_id: value.build_id.to_string(),
    job_id: value.job_id.to_string(),
    agent_id: value.agent_id.to_string(),
    registration_epoch: value.registration_epoch,
    lease_id: value.lease_id.to_string(),
    namespace: value.namespace,
    read: value.read,
    write: value.write,
    quota_bytes: value.quota_bytes,
    created_at_unix_ms: value.created_at.unix_millis(),
    expires_at_unix_ms: value.expires_at.unix_millis(),
    retention_until_unix_ms: value.retention_until.unix_millis(),
    state: match value.state {
      CacheSessionDiagnosticState::Active => CacheSessionState::Active,
      CacheSessionDiagnosticState::Revoked => CacheSessionState::Revoked,
      CacheSessionDiagnosticState::Expired => CacheSessionState::Expired,
      CacheSessionDiagnosticState::Fenced => CacheSessionState::Fenced,
    },
    revoked_at_unix_ms: value.revoked_at.map(|timestamp| timestamp.unix_millis()),
  }
}

fn invalid(request_id: &RequestId) -> ApiError {
  ApiError::new(
    axum::http::StatusCode::BAD_REQUEST,
    ErrorCode::InvalidRequest,
    "cache session identity is invalid",
    request_id,
  )
}
