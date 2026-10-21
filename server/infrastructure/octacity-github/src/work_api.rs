//! Shared bounded Work API transport; Source, Reporter and review retain separate authorization.
use crate::api;
use http_body_util::BodyExt as _;
use octacity_work_source_protocol::{FailureClass, MAX_RESPONSE_BYTES};
use serde_json::Value;
/// Closed transport failure; retry advice is data, never another SDK-side retry.
pub(crate) struct WorkApiFailure {
  pub(crate) class: FailureClass,
  pub(crate) status: Option<u16>,
  pub(crate) retry_after_millis: Option<u32>,
}
impl From<FailureClass> for WorkApiFailure {
  fn from(class: FailureClass) -> Self {
    Self {
      class,
      status: None,
      retry_after_millis: None,
    }
  }
}
fn throttled(headers: &http::HeaderMap) -> WorkApiFailure {
  let delay = (|| {
    let retry = match headers.get("retry-after") {
      Some(v) => Some(v.to_str().ok()?.parse::<u64>().ok()?.checked_mul(1000)?),
      None => None,
    };
    let reset = if headers.get("x-ratelimit-remaining").is_some_and(|v| v == "0") {
      match headers.get("x-ratelimit-reset") {
        Some(value) => {
          let epoch = value.to_str().ok()?.parse::<u64>().ok()?.checked_mul(1000)?;
          let now = u64::try_from(chrono::Utc::now().timestamp_millis()).ok()?;
          Some(epoch.saturating_sub(now))
        }
        None => Some(60_000),
      }
    } else {
      None
    };
    Some(retry.into_iter().chain(reset).max().unwrap_or(60_000)).filter(|v| *v <= 3_600_000)
  })();
  match delay {
    Some(delay) => WorkApiFailure {
      class: FailureClass::Transient,
      status: None,
      retry_after_millis: Some(delay as u32),
    },
    None => FailureClass::Permanent.into(),
  }
}

pub(crate) async fn read(
  client: &octocrab::Octocrab,
  url: &url::Url,
  version: &str,
) -> Result<(Value, http::HeaderMap), WorkApiFailure> {
  request(client, url, version, reqwest::Method::GET, None).await
}
pub(crate) async fn request(
  client: &octocrab::Octocrab,
  url: &url::Url,
  version: &str,
  method: reqwest::Method,
  body: Option<&Value>,
) -> Result<(Value, http::HeaderMap), WorkApiFailure> {
  // A successful mutation may already have applied despite a lost, malformed
  // or oversized body. Bound decoding, then let the original outbox operation
  // observe on retry; never retry an HTTP write in this transport.
  let invalid_body = || {
    WorkApiFailure::from(if method == reqwest::Method::GET {
      FailureClass::ProtocolFault
    } else {
      FailureClass::Transient
    })
  };
  let request = client
    .build_request(http::Request::builder().method(method.clone()).uri(url.as_str()), body)
    .map_err(|_| FailureClass::Permanent)?;
  let request = api::pin_version(request, version).map_err(|_| FailureClass::Permanent)?;
  let response = client.execute(request).await.map_err(|_| FailureClass::Transient)?;
  match response.status() {
    reqwest::StatusCode::TOO_MANY_REQUESTS => return Err(throttled(response.headers())),
    reqwest::StatusCode::FORBIDDEN
      if response
        .headers()
        .get("x-ratelimit-remaining")
        .is_some_and(|value| value == "0")
        || response.headers().contains_key("retry-after") =>
    {
      return Err(throttled(response.headers()));
    }
    reqwest::StatusCode::UNAUTHORIZED
    | reqwest::StatusCode::FORBIDDEN
    | reqwest::StatusCode::NOT_FOUND
    | reqwest::StatusCode::UNPROCESSABLE_ENTITY => {
      return Err(WorkApiFailure {
        class: FailureClass::Permanent,
        status: Some(response.status().as_u16()),
        retry_after_millis: None,
      });
    }
    reqwest::StatusCode::CONFLICT => return Err(FailureClass::Conflict.into()),
    status if status.is_server_error() => return Err(FailureClass::Transient.into()),
    reqwest::StatusCode::OK => {}
    reqwest::StatusCode::NO_CONTENT if method == reqwest::Method::DELETE => {
      return Ok((Value::Null, response.headers().clone()));
    }
    _ => return Err(FailureClass::ProtocolFault.into()),
  }
  if response
    .headers()
    .get("content-length")
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.parse::<u64>().ok())
    .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
  {
    return Err(invalid_body());
  }
  let headers = response.headers().clone();
  let mut bytes = Vec::new();
  let mut body = response.into_body();
  while let Some(frame) = body.frame().await {
    let frame = frame.map_err(|_| FailureClass::Transient)?;
    let Ok(chunk) = frame.into_data() else { continue };
    if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
      return Err(invalid_body());
    }
    bytes.extend_from_slice(&chunk);
  }
  serde_json::from_slice(&bytes)
    .map(|value| (value, headers))
    .map_err(|_| invalid_body())
}

/// Builds only an installed repository path and verifies its current native identity.
pub(crate) async fn repository(
  client: &octocrab::Octocrab,
  settings: &crate::config::ApiSettings,
  owner: &str,
  name: &str,
  native_id: &str,
) -> Result<url::Url, WorkApiFailure> {
  let mut repository = settings.base.clone();
  repository
    .path_segments_mut()
    .map_err(|_| FailureClass::Permanent)?
    .pop_if_empty()
    .extend(["repos", owner, name]);
  let (metadata, _) = read(client, &repository, &settings.version).await?;
  let id = metadata
    .get("id")
    .and_then(Value::as_u64)
    .filter(|id| *id > 0)
    .ok_or(FailureClass::ProtocolFault)?;
  if id.to_string() != native_id {
    return Err(FailureClass::Conflict.into());
  }
  Ok(repository)
}
