//! Read-only review normalization; provider DTOs and bodies never leave this adapter.
use crate::{
  api,
  config::{ApiSettings, Credential},
  work_api::{WorkApiFailure, read},
};
use octacity_work_source_protocol::{FailureClass, review};
use serde_json::Value;

pub(crate) async fn observe(
  settings: &ApiSettings,
  owner: &str,
  name: &str,
  credential: &Credential,
  intent: &review::Intent,
) -> Result<review::Observation, WorkApiFailure> {
  let number = review_number(&intent.item_id)?;
  let client = api::client(
    &settings.base,
    &settings.version,
    &settings.root_certificates,
    credential,
  )
  .map_err(|_| FailureClass::Permanent)?;
  let repository = crate::work_api::repository(&client, settings, owner, name, &intent.resource.resource_id).await?;
  let mut url = repository;
  url
    .path_segments_mut()
    .map_err(|_| FailureClass::Permanent)?
    .extend(["pulls", &number.to_string()]);
  let (value, _) = read(&client, &url, &settings.version).await?;
  if value.get("id").and_then(Value::as_u64).filter(|id| *id > 0).is_none()
    || value.get("number").and_then(Value::as_u64) != Some(number)
    || value
      .pointer("/base/repo/id")
      .and_then(Value::as_u64)
      .map(|id| id.to_string())
      .as_deref()
      != Some(&intent.resource.resource_id)
  {
    return Err(FailureClass::Conflict.into());
  }
  Ok(review::Observation {
    subject: intent.subject(),
    facts: review::Facts {
      title: text(value.get("title")),
      number: Some(number.to_string()),
      url: navigation(value.get("html_url")),
      author: text(value.pointer("/user/login")),
      state: match (
        value.get("state").and_then(Value::as_str),
        value.get("merged").and_then(Value::as_bool),
      ) {
        (Some("closed"), Some(true)) => Some(review::State::Merged),
        (Some("closed"), Some(false)) => Some(review::State::Closed),
        (Some("open"), _) => Some(review::State::Open),
        _ => None,
      },
      draft: value.get("draft").and_then(Value::as_bool),
      head: revision(value.pointer("/head/sha")),
      base: revision(value.pointer("/base/sha")),
      changed_files: value.get("changed_files").and_then(Value::as_u64),
      additions: value.get("additions").and_then(Value::as_u64),
      deletions: value.get("deletions").and_then(Value::as_u64),
      review: None,
      checks: None,
    },
  })
}
fn text(value: Option<&Value>) -> Option<String> {
  value
    .and_then(Value::as_str)
    .filter(|value| {
      !value.is_empty()
        && value.len() <= octacity_work_source_protocol::MAX_FIELD_BYTES
        && !value.chars().any(char::is_control)
    })
    .map(str::to_owned)
}
fn revision(value: Option<&Value>) -> Option<String> {
  text(value).filter(|value| api::object_id(value))
}
fn navigation(value: Option<&Value>) -> Option<String> {
  let value = text(value)?;
  let url = url::Url::parse(&value).ok()?;
  (url.scheme() == "https"
    && url.host_str().is_some()
    && url.username().is_empty()
    && url.password().is_none()
    && url.query().is_none()
    && url.fragment().is_none())
  .then_some(value)
}

pub(crate) async fn checks(
  settings: &ApiSettings,
  owner: &str,
  name: &str,
  credential: &Credential,
  intent: &review::Intent,
) -> Result<review::CheckPage, WorkApiFailure> {
  review_number(&intent.item_id)?;
  let head = intent
    .head
    .as_deref()
    .filter(|head| api::object_id(head))
    .ok_or(FailureClass::InvalidRequest)?;
  let context = serde_json::to_vec(&(
    &intent.read_id,
    &intent.configuration_digest,
    &intent.resource.resource_id,
    &intent.resource.resource_locator,
    &intent.item_id,
    head,
    intent.max_items,
    intent.max_page_bytes,
  ))
  .map_err(|_| FailureClass::InvalidRequest)?;
  let (page, context) = crate::issue_pagination::current_for(intent.cursor.as_deref(), &context)?;
  let client = api::client(
    &settings.base,
    &settings.version,
    &settings.root_certificates,
    credential,
  )
  .map_err(|_| FailureClass::Permanent)?;
  let mut url = crate::work_api::repository(&client, settings, owner, name, &intent.resource.resource_id).await?;
  url
    .path_segments_mut()
    .map_err(|_| FailureClass::Permanent)?
    .extend(["commits", head, "check-runs"]);
  url
    .query_pairs_mut()
    .append_pair("per_page", &intent.max_items.to_string())
    .append_pair("page", &page.to_string());
  let (value, headers) = read(&client, &url, &settings.version).await?;
  let next_cursor = crate::issue_pagination::next(&headers, &url, page, context)?;
  let runs = value
    .get("check_runs")
    .and_then(Value::as_array)
    .ok_or(FailureClass::ProtocolFault)?;
  if runs.len() > intent.max_items as usize {
    return Err(FailureClass::ProtocolFault.into());
  }
  let mut items = Vec::new();
  for run in runs {
    if run.get("head_sha").and_then(Value::as_str) != Some(head) {
      return Err(FailureClass::Conflict.into());
    }
    let id = run
      .get("id")
      .and_then(Value::as_u64)
      .filter(|id| *id > 0)
      .ok_or(FailureClass::ProtocolFault)?
      .to_string();
    let name = text(run.get("name")).ok_or(FailureClass::ProtocolFault)?;
    let state = match run.get("status").and_then(Value::as_str) {
      Some("queued" | "waiting" | "pending" | "requested") => review::CheckState::Pending,
      Some("in_progress") => review::CheckState::Running,
      Some("completed") => review::CheckState::Completed,
      _ => review::CheckState::Unknown,
    };
    let conclusion = if state == review::CheckState::Completed {
      match run.get("conclusion").and_then(Value::as_str) {
        Some("success") => Some(review::Conclusion::Success),
        Some("failure") => Some(review::Conclusion::Failure),
        Some("cancelled") => Some(review::Conclusion::Cancelled),
        Some("skipped") => Some(review::Conclusion::Skipped),
        Some("neutral") => Some(review::Conclusion::Neutral),
        Some("timed_out") => Some(review::Conclusion::TimedOut),
        Some("action_required") => Some(review::Conclusion::ActionRequired),
        Some("stale") => Some(review::Conclusion::Stale),
        _ => None,
      }
    } else {
      None
    };
    items.push(review::Check {
      id,
      name,
      producer: text(run.pointer("/app/slug")),
      state,
      conclusion,
      url: navigation(run.get("html_url")),
    });
  }
  Ok(review::CheckPage {
    subject: intent.subject(),
    head: head.to_owned(),
    items,
    next_cursor,
  })
}

fn review_number(value: &str) -> Result<u64, FailureClass> {
  value
    .parse::<u64>()
    .ok()
    .filter(|number| *number > 0 && number.to_string() == value)
    .ok_or(FailureClass::InvalidRequest)
}
