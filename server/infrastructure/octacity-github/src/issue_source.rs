//! Bounded GitHub DTO translation; no forge type enters Factory or the Work wire contract.
use crate::{
  api,
  config::{ApiSettings, Credential},
  issue_selector::{IssueSelector, IssueState},
};
use http_body_util::BodyExt as _;
use octacity_work_source_protocol::{
  Candidate, FailureClass, MAX_RESPONSE_BYTES,
  source::{Page, ScanIntent},
};
use serde::Deserialize;
use serde_json::Value;

/// Closed transport failure; retry advice is data, never another SDK-side retry.
pub(crate) struct SourceReadFailure {
  pub(crate) class: FailureClass,
  pub(crate) retry_after_millis: Option<u32>,
}
impl From<FailureClass> for SourceReadFailure {
  fn from(class: FailureClass) -> Self {
    Self {
      class,
      retry_after_millis: None,
    }
  }
}
fn throttled(headers: &http::HeaderMap) -> SourceReadFailure {
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
    Some(delay) => SourceReadFailure {
      class: FailureClass::Transient,
      retry_after_millis: Some(delay as u32),
    },
    None => FailureClass::Permanent.into(),
  }
}

#[derive(Deserialize)]
struct Issue {
  id: u64,
  number: u64,
  updated_at: String,
  title: String,
  body: Option<String>,
  labels: Vec<Label>,
  state: IssueState,
}
#[derive(Deserialize)]
struct Label {
  name: String,
}

pub(crate) async fn scan(
  api: &ApiSettings,
  owner: &str,
  name: &str,
  credential: &Credential,
  intent: &ScanIntent,
  selectors: &[IssueSelector],
) -> Result<Page, SourceReadFailure> {
  let (page, context) = crate::issue_pagination::current(intent)?;
  let since = intent
    .window
    .map(|window| {
      // GitHub's since is exclusive and second-precision. Fetch one second of overlap,
      // then enforce the caller's inclusive millisecond window locally.
      chrono::DateTime::from_timestamp((window.from_millis / 1000) as i64 - 1, 0)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .ok_or(FailureClass::InvalidRequest)
    })
    .transpose()?;
  let client =
    api::client(&api.base, &api.version, &api.root_certificates, credential).map_err(|_| FailureClass::Permanent)?;
  let mut repository = api.base.clone();
  repository
    .path_segments_mut()
    .map_err(|_| FailureClass::Permanent)?
    .pop_if_empty()
    .push("repos")
    .push(owner)
    .push(name);
  let (metadata, _) = read(&client, &repository, &api.version).await?;
  let native_id = metadata
    .get("id")
    .and_then(Value::as_u64)
    .filter(|id| *id > 0)
    .ok_or(FailureClass::ProtocolFault)?;
  if native_id.to_string() != intent.resource.resource_id {
    return Err(FailureClass::Conflict.into());
  }
  let mut url = repository;
  url
    .path_segments_mut()
    .map_err(|_| FailureClass::Permanent)?
    .push("issues");
  url
    .query_pairs_mut()
    .append_pair("state", "all")
    .append_pair("sort", "updated")
    .append_pair("direction", "asc")
    .append_pair("per_page", &intent.max_items.to_string())
    .append_pair("page", &page.to_string());
  if let Some(since) = since {
    url.query_pairs_mut().append_pair("since", &since);
  }
  let (issues, headers) = read(&client, &url, &api.version).await?;
  let next_cursor = crate::issue_pagination::next(&headers, &url, page, context)?;
  let issues: Vec<Value> = serde_json::from_value(issues).map_err(|_| FailureClass::ProtocolFault)?;
  if issues.len() > usize::from(intent.max_items) {
    return Err(FailureClass::ProtocolFault.into());
  }
  let mut candidates = Vec::new();
  let mut matching_selections = Vec::new();
  for issue in issues {
    if issue.get("pull_request").is_some() {
      continue;
    }
    let issue: Issue = serde_json::from_value(issue).map_err(|_| FailureClass::ProtocolFault)?;
    if issue.id == 0 || issue.number == 0 {
      return Err(FailureClass::ProtocolFault.into());
    }
    let updated = chrono::DateTime::parse_from_rfc3339(&issue.updated_at).map_err(|_| FailureClass::ProtocolFault)?;
    let updated = u64::try_from(updated.timestamp_millis()).map_err(|_| FailureClass::ProtocolFault)?;
    if intent
      .window
      .is_some_and(|window| updated < window.from_millis || updated > window.until_millis)
    {
      continue;
    }
    let labels: Vec<String> = issue.labels.into_iter().map(|label| label.name).collect();
    let matches = selectors
      .iter()
      .zip(std::iter::once(&intent.selection).chain(&intent.additional_selections))
      .filter(|(selector, _)| selector.matches(issue.state, &labels))
      .map(|(_, selection)| selection.clone())
      .collect::<Vec<_>>();
    if matches.is_empty() {
      continue;
    }
    if !intent.additional_selections.is_empty() {
      matching_selections.push(matches);
    }
    candidates.push(Candidate {
      container_id: intent.resource.resource_id.clone(),
      item_id: issue.id.to_string(),
      display_id: issue.number.to_string(),
      update_identity: issue.updated_at,
      title: issue.title,
      body: issue.body,
      fields: Default::default(),
      labels,
      state: match issue.state {
        IssueState::Open => "open",
        IssueState::Closed => "closed",
      }
      .into(),
    });
  }
  Ok(Page {
    scan_id: intent.scan_id.clone(),
    resource_id: intent.resource.resource_id.clone(),
    configuration_digest: intent.configuration_digest.clone(),
    selection: intent.selection.clone(),
    candidates,
    matching_selections,
    next_cursor,
  })
}
async fn read(
  client: &octocrab::Octocrab,
  url: &url::Url,
  version: &str,
) -> Result<(Value, http::HeaderMap), SourceReadFailure> {
  let request = client
    .build_request(
      http::Request::builder().method(reqwest::Method::GET).uri(url.as_str()),
      None::<&Value>,
    )
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
    | reqwest::StatusCode::UNPROCESSABLE_ENTITY => return Err(FailureClass::Permanent.into()),
    reqwest::StatusCode::CONFLICT => return Err(FailureClass::Conflict.into()),
    status if status.is_server_error() => return Err(FailureClass::Transient.into()),
    reqwest::StatusCode::OK => {}
    _ => return Err(FailureClass::ProtocolFault.into()),
  }
  if response
    .headers()
    .get("content-length")
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.parse::<u64>().ok())
    .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
  {
    return Err(FailureClass::ProtocolFault.into());
  }
  let headers = response.headers().clone();
  let mut bytes = Vec::new();
  let mut body = response.into_body();
  while let Some(frame) = body.frame().await {
    let frame = frame.map_err(|_| FailureClass::Transient)?;
    let Ok(chunk) = frame.into_data() else { continue };
    if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
      return Err(FailureClass::ProtocolFault.into());
    }
    bytes.extend_from_slice(&chunk);
  }
  serde_json::from_slice(&bytes)
    .map(|value| (value, headers))
    .map_err(|_| SourceReadFailure::from(FailureClass::ProtocolFault))
}
