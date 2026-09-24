//! Exact bounded HTTP adapter for Octa's published L2 cache protocol v1.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::{
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};

use axum::{
  Router,
  body::{Body, Bytes, to_bytes},
  extract::{Path, Query, Request, State},
  http::{HeaderMap, HeaderValue, StatusCode, header},
  middleware::{self, Next},
  response::{IntoResponse, Response},
  routing::{get, post},
};
use octacity_server_application::{
  BlobDescriptor, BlobEncoding, CacheDataPlaneError, CacheDataPlaneUseCases, CacheRequestAuthority, CacheWriteResult,
  Digest, DigestAlgorithm, FindMissingBlobsRequestV1, MAX_ACTION_RESULT_WIRE_BYTES, MAX_REMOTE_CACHE_METADATA_BYTES,
  REMOTE_CACHE_BLOB_CONTENT_TYPE, REMOTE_CACHE_JSON_CONTENT_TYPE, REMOTE_CACHE_PROTOCOL_HEADER,
  REMOTE_CACHE_PROTOCOL_HEADER_VALUE_V1, WriteActionRequestV1,
};
use serde::Deserialize;

/// Type-erased application capability consumed by the cache ingress.
#[derive(Clone)]
pub struct CacheDataPlaneApplication(Arc<dyn CacheDataPlaneUseCases>);

impl CacheDataPlaneApplication {
  /// Erases one concrete cache coordinator without exposing infrastructure to HTTP.
  pub fn new(application: Arc<impl CacheDataPlaneUseCases + 'static>) -> Self {
    Self(application)
  }
}

#[derive(Clone)]
struct RouterState {
  application: CacheDataPlaneApplication,
  max_blob_bytes: usize,
}

/// Builds the isolated Octa cache v1 router with a bounded blob body ceiling.
pub fn cache_router(application: CacheDataPlaneApplication, max_blob_bytes: usize) -> Router {
  let state = RouterState {
    application,
    max_blob_bytes,
  };
  Router::new()
    .route(
      "/v1/actions/{algorithm}/{hash}/{size}",
      get(read_action).put(write_action),
    )
    .route("/v1/blobs/missing", post(find_missing))
    .route(
      "/v1/blobs/{algorithm}/{hash}/{expanded}/{encoding}/{encoded}/{entries}",
      get(read_blob).put(write_blob),
    )
    .layer(middleware::from_fn(add_protocol_header))
    .with_state(state)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NamespaceQuery {
  namespace: String,
}

async fn read_action(
  State(state): State<RouterState>,
  Path((algorithm, hash, size)): Path<(String, String, u64)>,
  Query(query): Query<NamespaceQuery>,
  headers: HeaderMap,
) -> Response {
  let request = match request_authority(&headers, Some(query.namespace)) {
    Ok(request) => request,
    Err(error) => return error.response(),
  };
  let action = match digest(&algorithm, &hash, size) {
    Ok(action) => action,
    Err(error) => return error.response(),
  };
  match state.application.0.read_action(request, action).await {
    Ok(Some(result)) => json(StatusCode::OK, &result),
    Ok(None) => empty(StatusCode::NOT_FOUND),
    Err(error) => application_error(error),
  }
}

async fn write_action(
  State(state): State<RouterState>,
  Path((algorithm, hash, size)): Path<(String, String, u64)>,
  Query(query): Query<NamespaceQuery>,
  headers: HeaderMap,
  request: Request,
) -> Response {
  if !write_headers(&headers, REMOTE_CACHE_JSON_CONTENT_TYPE) {
    return invalid();
  }
  let authority = match request_authority(&headers, Some(query.namespace)) {
    Ok(request) => request,
    Err(error) => return error.response(),
  };
  let action = match digest(&algorithm, &hash, size) {
    Ok(action) => action,
    Err(error) => return error.response(),
  };
  let body = match to_bytes(request.into_body(), MAX_ACTION_RESULT_WIRE_BYTES + 64 * 1024).await {
    Ok(body) => body,
    Err(_) => return payload_too_large(),
  };
  let request: WriteActionRequestV1 = match serde_json::from_slice(&body) {
    Ok(request) => request,
    Err(_) => return invalid(),
  };
  match state.application.0.write_action(authority, action, request).await {
    Ok(CacheWriteResult::Written) => empty(StatusCode::CREATED),
    Ok(CacheWriteResult::AlreadyPresent) => empty(StatusCode::NO_CONTENT),
    Err(error) => application_error(error),
  }
}

async fn find_missing(State(state): State<RouterState>, headers: HeaderMap, request: Request) -> Response {
  if content_type(&headers) != Some(REMOTE_CACHE_JSON_CONTENT_TYPE) {
    return invalid();
  }
  let authority = match request_authority(&headers, None) {
    Ok(request) => request,
    Err(error) => return error.response(),
  };
  let body = match to_bytes(request.into_body(), MAX_REMOTE_CACHE_METADATA_BYTES).await {
    Ok(body) => body,
    Err(_) => return payload_too_large(),
  };
  let request: FindMissingBlobsRequestV1 = match serde_json::from_slice(&body) {
    Ok(request) => request,
    Err(_) => return invalid(),
  };
  match state.application.0.find_missing(authority, request).await {
    Ok(response) => json(StatusCode::OK, &response),
    Err(error) => application_error(error),
  }
}

async fn read_blob(
  State(state): State<RouterState>,
  Path(path): Path<(String, String, u64, String, u64, u64)>,
  headers: HeaderMap,
) -> Response {
  let authority = match request_authority(&headers, None) {
    Ok(request) => request,
    Err(error) => return error.response(),
  };
  let blob = match blob_descriptor(&path) {
    Ok(blob) => blob,
    Err(error) => return error.response(),
  };
  match state.application.0.read_blob(authority, blob).await {
    Ok(Some(bytes)) => typed(StatusCode::OK, REMOTE_CACHE_BLOB_CONTENT_TYPE, bytes),
    Ok(None) => empty(StatusCode::NOT_FOUND),
    Err(error) => application_error(error),
  }
}

async fn write_blob(
  State(state): State<RouterState>,
  Path(path): Path<(String, String, u64, String, u64, u64)>,
  headers: HeaderMap,
  request: Request,
) -> Response {
  if !write_headers(&headers, REMOTE_CACHE_BLOB_CONTENT_TYPE) {
    return invalid();
  }
  let authority = match request_authority(&headers, None) {
    Ok(request) => request,
    Err(error) => return error.response(),
  };
  let blob = match blob_descriptor(&path) {
    Ok(blob) => blob,
    Err(error) => return error.response(),
  };
  if blob.encoded_size_bytes > state.max_blob_bytes as u64 {
    return payload_too_large();
  }
  let bytes = match to_bytes(request.into_body(), state.max_blob_bytes).await {
    Ok(bytes) => bytes.to_vec(),
    Err(_) => return payload_too_large(),
  };
  match state.application.0.write_blob(authority, blob, bytes).await {
    Ok(CacheWriteResult::Written) => empty(StatusCode::CREATED),
    Ok(CacheWriteResult::AlreadyPresent) => empty(StatusCode::NO_CONTENT),
    Err(error) => application_error(error),
  }
}

fn request_authority(headers: &HeaderMap, namespace: Option<String>) -> Result<CacheRequestAuthority, RequestError> {
  if single_header(headers, REMOTE_CACHE_PROTOCOL_HEADER) != Some(REMOTE_CACHE_PROTOCOL_HEADER_VALUE_V1) {
    return Err(RequestError::Invalid);
  }
  let authorization = single_header(headers, header::AUTHORIZATION.as_str()).ok_or(RequestError::Unauthorized)?;
  let bearer_token = authorization
    .strip_prefix("Bearer ")
    .filter(|value| !value.is_empty())
    .ok_or(RequestError::Unauthorized)?;
  let observed_at_unix_ms = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .ok()
    .and_then(|value| i64::try_from(value.as_millis()).ok())
    .ok_or(RequestError::Unavailable)?;
  Ok(CacheRequestAuthority {
    bearer_token: bearer_token.to_owned(),
    namespace,
    observed_at_unix_ms,
  })
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
  let mut values = headers.get_all(name).iter();
  let first = values.next()?.to_str().ok()?;
  values.next().is_none().then_some(first)
}

fn content_type(headers: &HeaderMap) -> Option<&str> {
  single_header(headers, header::CONTENT_TYPE.as_str())
}

fn write_headers(headers: &HeaderMap, expected_content_type: &str) -> bool {
  single_header(headers, header::IF_NONE_MATCH.as_str()) == Some("*")
    && content_type(headers) == Some(expected_content_type)
}

fn digest(algorithm: &str, hash: &str, size: u64) -> Result<Digest, RequestError> {
  let algorithm = algorithm
    .parse::<DigestAlgorithm>()
    .map_err(|_| RequestError::Invalid)?;
  let digest = Digest::from_hex(algorithm, hash, size).map_err(|_| RequestError::Invalid)?;
  (algorithm == DigestAlgorithm::Blake3)
    .then_some(digest)
    .ok_or(RequestError::Invalid)
}

fn blob_descriptor(path: &(String, String, u64, String, u64, u64)) -> Result<BlobDescriptor, RequestError> {
  let encoding = match path.3.as_str() {
    "identity" => BlobEncoding::Identity,
    "zstd_v1" => BlobEncoding::ZstdV1,
    _ => return Err(RequestError::Invalid),
  };
  let descriptor = BlobDescriptor {
    digest: digest(&path.0, &path.1, path.2)?,
    encoding,
    encoded_size_bytes: path.4,
    expanded_size_bytes: path.2,
    entry_count: path.5,
  };
  descriptor.validate().map_err(|_| RequestError::Invalid)?;
  Ok(descriptor)
}

enum RequestError {
  Invalid,
  Unauthorized,
  Unavailable,
}

impl RequestError {
  fn response(self) -> Response {
    match self {
      Self::Invalid => invalid(),
      Self::Unauthorized => unauthorized(),
      Self::Unavailable => unavailable(),
    }
  }
}

async fn add_protocol_header(request: Request, next: Next) -> Response {
  let mut response = next.run(request).await;
  response.headers_mut().insert(
    REMOTE_CACHE_PROTOCOL_HEADER,
    HeaderValue::from_static(REMOTE_CACHE_PROTOCOL_HEADER_VALUE_V1),
  );
  response
}

fn application_error(error: CacheDataPlaneError) -> Response {
  match error {
    CacheDataPlaneError::InvalidRequest => invalid(),
    CacheDataPlaneError::AuthorizationRejected => unauthorized(),
    CacheDataPlaneError::PayloadTooLarge => payload_too_large(),
    CacheDataPlaneError::Conflict => empty(StatusCode::CONFLICT),
    CacheDataPlaneError::QuotaExceeded => empty(StatusCode::INSUFFICIENT_STORAGE),
    CacheDataPlaneError::Integrity | CacheDataPlaneError::Unavailable => unavailable(),
  }
}

fn json(status: StatusCode, value: &impl serde::Serialize) -> Response {
  match serde_json::to_vec(value) {
    Ok(body) => typed(status, REMOTE_CACHE_JSON_CONTENT_TYPE, body),
    Err(_) => unavailable(),
  }
}

fn typed(status: StatusCode, content_type: &'static str, bytes: impl Into<Bytes>) -> Response {
  (
    [(header::CONTENT_TYPE, content_type)],
    (status, Body::from(bytes.into())),
  )
    .into_response()
}

fn empty(status: StatusCode) -> Response {
  status.into_response()
}

fn invalid() -> Response {
  empty(StatusCode::BAD_REQUEST)
}

fn unauthorized() -> Response {
  empty(StatusCode::UNAUTHORIZED)
}

fn payload_too_large() -> Response {
  empty(StatusCode::PAYLOAD_TOO_LARGE)
}

fn unavailable() -> Response {
  empty(StatusCode::SERVICE_UNAVAILABLE)
}
