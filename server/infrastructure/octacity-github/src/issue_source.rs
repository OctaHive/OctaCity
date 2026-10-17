//! Bounded GitHub DTO translation; no forge type enters Factory or the Work wire contract.
use crate::work_api::{WorkApiFailure, read};
use crate::{
  api,
  config::{ApiSettings, Credential},
  issue_selector::{IssueSelector, IssueState},
};
use octacity_work_source_protocol::{
  Candidate, FailureClass,
  source::{Page, ScanIntent},
};
use serde::Deserialize;
use serde_json::Value;

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
) -> Result<Page, WorkApiFailure> {
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
