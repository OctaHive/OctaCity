use std::sync::Arc;

use axum::{
  Extension, Json,
  extract::{Path, Query, State, rejection::QueryRejection},
};
use octacity_server_application::{
  BuildLogSearchCursorInput, BuildLogSearchInput, BuildLogSearchPageProjection,
  BuildLogStream as ApplicationBuildLogStream, LogSearchMode as ApplicationLogSearchMode, SearchBuildLogsQuery,
};
use serde::Deserialize;

use super::{ApiError, ManagementApplication, application_error, query_error};
use crate::{
  RequestId,
  v1::{
    BuildLogSearchFreshness, BuildLogSearchHit, BuildLogSearchMode, BuildLogSearchPage,
    BuildLogStream as RestBuildLogStream, Cursor, ErrorCode,
  },
};

pub(crate) const DEFAULT_LOG_SEARCH_LIMIT: u16 = 50;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BuildLogSearchParameters {
  query: String,
  mode: BuildLogSearchMode,
  build_id: Option<String>,
  attempt_id: Option<String>,
  job_id: Option<String>,
  stream: Option<RestBuildLogStream>,
  occurred_from_unix_ms: Option<i64>,
  occurred_through_unix_ms: Option<i64>,
  after: Option<Cursor>,
  #[serde(default = "default_limit")]
  limit: u16,
}

const fn default_limit() -> u16 {
  DEFAULT_LOG_SEARCH_LIMIT
}

pub(super) async fn search_build_logs(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
  parameters: Result<Query<BuildLogSearchParameters>, QueryRejection>,
) -> Result<Json<BuildLogSearchPage>, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = SearchBuildLogsQuery::try_from_input(BuildLogSearchInput {
    project_id,
    text: parameters.query,
    mode: match parameters.mode {
      BuildLogSearchMode::FullText => ApplicationLogSearchMode::FullText,
      BuildLogSearchMode::Literal => ApplicationLogSearchMode::Literal,
    },
    build_id: parameters.build_id,
    attempt_id: parameters.attempt_id,
    job_id: parameters.job_id,
    stream: parameters.stream.map(|stream| match stream {
      RestBuildLogStream::Stdout => ApplicationBuildLogStream::Stdout,
      RestBuildLogStream::Stderr => ApplicationBuildLogStream::Stderr,
    }),
    occurred_from_unix_ms: parameters.occurred_from_unix_ms,
    occurred_through_unix_ms: parameters.occurred_through_unix_ms,
    after: parameters
      .after
      .map(|cursor| decode_cursor(&cursor, &request_id))
      .transpose()?,
    limit: parameters.limit,
  })
  .map_err(|error| application_error(error.classification(), &request_id))?;
  let page = application
    .log_search
    .0
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  project_page(page, &request_id).map(Json)
}

fn decode_cursor(cursor: &Cursor, request_id: &RequestId) -> Result<BuildLogSearchCursorInput, ApiError> {
  let (occurred_at, chunk_id) = cursor
    .as_str()
    .split_once('.')
    .ok_or_else(|| invalid_cursor(request_id))?;
  let occurred_at_unix_ms = occurred_at.parse().map_err(|_| invalid_cursor(request_id))?;
  if chunk_id.is_empty() {
    return Err(invalid_cursor(request_id));
  }
  Ok(BuildLogSearchCursorInput {
    occurred_at_unix_ms,
    chunk_id: chunk_id.to_owned(),
  })
}

fn project_page(page: BuildLogSearchPageProjection, request_id: &RequestId) -> Result<BuildLogSearchPage, ApiError> {
  Ok(BuildLogSearchPage {
    items: page
      .items
      .into_iter()
      .map(|hit| BuildLogSearchHit {
        chunk_id: hit.chunk_id.to_string(),
        build_id: hit.build_id.to_string(),
        attempt_id: hit.attempt_id.to_string(),
        job_id: hit.job_id.to_string(),
        stream: match hit.stream {
          ApplicationBuildLogStream::Stdout => RestBuildLogStream::Stdout,
          ApplicationBuildLogStream::Stderr => RestBuildLogStream::Stderr,
        },
        first_sequence: hit.first_sequence,
        last_sequence: hit.last_sequence,
        occurred_at_unix_ms: hit.occurred_at_unix_ms,
        snippet: hit.snippet,
      })
      .collect(),
    next_cursor: page
      .next_cursor
      .map(|cursor| Cursor::new(format!("{}.{}", cursor.occurred_at_unix_ms, cursor.chunk_id)))
      .transpose()
      .map_err(|_| internal_cursor(request_id))?,
    freshness: BuildLogSearchFreshness {
      indexed_through: page.freshness.indexed_through,
      committed_through: page.freshness.committed_through,
      caught_up: page.freshness.caught_up,
    },
  })
}

fn invalid_cursor(request_id: &RequestId) -> ApiError {
  ApiError::new(
    axum::http::StatusCode::BAD_REQUEST,
    ErrorCode::InvalidRequest,
    "Build-log search cursor is invalid",
    request_id,
  )
}

fn internal_cursor(request_id: &RequestId) -> ApiError {
  ApiError::new(
    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    ErrorCode::Internal,
    "Build-log search cursor could not be represented",
    request_id,
  )
}
