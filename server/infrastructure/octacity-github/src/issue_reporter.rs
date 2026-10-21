//! Bounded native Issue effects behind the provider-neutral Reporter contract.
use crate::{
  api,
  config::{ApiSettings, Credential},
  issue_selector::IssueState,
  status_mapping::StatusMapping,
  work_api::{WorkApiFailure, read, request as api_request},
};
use octacity_work_source_protocol::{FailureClass, reporter};
use serde::Deserialize;
mod ordered;

#[derive(Deserialize)]
struct Issue {
  id: u64,
  number: u64,
  state: IssueState,
}
#[derive(Clone, Deserialize)]
struct Label {
  name: String,
  #[serde(default)]
  id: Option<u64>,
}

pub(crate) async fn report(
  settings: &ApiSettings,
  owner: &str,
  name: &str,
  credential: &Credential,
  request: &reporter::Request,
  mapping: &StatusMapping,
) -> Result<reporter::Outcome, WorkApiFailure> {
  let intent = request.command.intent().ok_or(FailureClass::Unsupported)?;
  let effects = mapping.effects(&intent.status).map_err(|_| FailureClass::Unsupported)?;
  let number = intent
    .display_id
    .parse::<u64>()
    .ok()
    .filter(|v| *v > 0 && v.to_string() == intent.display_id)
    .ok_or(FailureClass::InvalidRequest)?;
  let client = api::client(
    &settings.base,
    &settings.version,
    &settings.root_certificates,
    credential,
  )
  .map_err(|_| FailureClass::Permanent)?;
  let repository = crate::work_api::repository(&client, settings, owner, name, &intent.resource.resource_id).await?;
  let url = |segments: &[&str]| -> Result<url::Url, FailureClass> {
    let mut url = repository.clone();
    url
      .path_segments_mut()
      .map_err(|_| FailureClass::Permanent)?
      .extend(segments);
    Ok(url)
  };
  let ordered = request.protocol_version == reporter::ORDERED_PROTOCOL_VERSION;
  let labels = if ordered {
    ordered::scope_labels(mapping, effects)
  } else {
    effects.add().iter().chain(effects.remove()).collect()
  };
  // One bounded catalogue page covers ordinary managed groups. Missing entries
  // use the exact name endpoint, so a large repository never hides a valid label.
  // No cache crosses operations: Apply still checks fresh native identities/state.
  let mut catalogue = std::collections::BTreeMap::new();
  if ordered && !labels.is_empty() {
    let mut catalogue_url = url(&["labels"])?;
    catalogue_url.query_pairs_mut().append_pair("per_page", "100");
    let (value, _) = read(&client, &catalogue_url, &settings.version).await?;
    let page: Vec<Label> = serde_json::from_value(value).map_err(|_| FailureClass::ProtocolFault)?;
    if page.len() > 100 {
      return Err(FailureClass::ProtocolFault.into());
    }
    for label in page {
      if catalogue.insert(label.name.to_lowercase(), label).is_some() {
        return Err(FailureClass::Conflict.into());
      }
    }
  }
  let mut ids = std::collections::BTreeMap::new();
  for label in labels {
    let observed = if let Some(observed) = catalogue.get(&label.to_lowercase()) {
      observed.clone()
    } else {
      let (value, _) = read(&client, &url(&["labels", label])?, &settings.version).await?;
      serde_json::from_value::<Label>(value).map_err(|_| FailureClass::ProtocolFault)?
    };
    if observed.name.to_lowercase() != label.to_lowercase() {
      return Err(FailureClass::Conflict.into());
    }
    if ordered {
      let id = observed.id.filter(|id| *id > 0).ok_or(FailureClass::ProtocolFault)?;
      if ids.values().any(|other| *other == id) {
        return Err(FailureClass::Conflict.into());
      }
      ids.insert(label.to_lowercase(), id);
    }
  }
  let number = number.to_string();
  let issue_url = url(&["issues", &number])?;
  let mut labels_url = url(&["issues", &number, "labels"])?;
  labels_url.query_pairs_mut().append_pair("per_page", "100");
  let (state, labels) = inspect(&client, &issue_url, &labels_url, &settings.version, intent).await?;
  if matches!(request.command, reporter::Command::Inspect(_)) {
    return Ok(reporter::Outcome::Planned(ordered::plan(
      mapping,
      &settings.base,
      intent,
      &ids,
      state,
      &labels,
    )?));
  }
  if let reporter::Command::Apply(step) = &request.command {
    let plan = ordered::plan(mapping, &settings.base, intent, &ids, state, &labels)?;
    if !plan.steps.contains(&step.step) {
      return Err(FailureClass::Conflict.into());
    }
    let (target, method, body) = match ordered::mutation(&step.step)? {
      ordered::Mutation::Remove(label) => (
        url(&["issues", &number, "labels", &label])?,
        reqwest::Method::DELETE,
        None,
      ),
      ordered::Mutation::Add(label) => (
        url(&["issues", &number, "labels"])?,
        reqwest::Method::POST,
        Some(serde_json::json!({"labels":[label]})),
      ),
      ordered::Mutation::State(state) => (
        issue_url.clone(),
        reqwest::Method::PATCH,
        Some(serde_json::json!({"state":state})),
      ),
    };
    api_request(&client, &target, &settings.version, method, body.as_ref()).await?;
    let (state, labels) = inspect(&client, &issue_url, &labels_url, &settings.version, intent)
      .await
      .map_err(|error| {
        if error.class == FailureClass::ProtocolFault {
          FailureClass::Transient.into()
        } else {
          error
        }
      })?;
    return Ok(reporter::Outcome::Progressed(ordered::plan(
      mapping,
      &settings.base,
      intent,
      &ids,
      state,
      &labels,
    )?));
  }
  let contains = |expected: &String| {
    labels
      .iter()
      .any(|label| label.name.to_lowercase() == expected.to_lowercase())
  };
  if matches!(request.command, reporter::Command::Project(_)) {
    for label in effects.remove().iter().filter(|label| contains(label)) {
      api_request(
        &client,
        &url(&["issues", &number, "labels", label])?,
        &settings.version,
        reqwest::Method::DELETE,
        None,
      )
      .await?;
    }
    let additions: Vec<_> = effects.add().iter().filter(|label| !contains(label)).collect();
    if !additions.is_empty() {
      let body = serde_json::json!({"labels": additions});
      api_request(
        &client,
        &url(&["issues", &number, "labels"])?,
        &settings.version,
        reqwest::Method::POST,
        Some(&body),
      )
      .await?;
    }
    if let Some(desired) = effects.state().filter(|desired| *desired != state) {
      let body = serde_json::json!({"state": desired});
      api_request(
        &client,
        &issue_url,
        &settings.version,
        reqwest::Method::PATCH,
        Some(&body),
      )
      .await?;
    }
  }
  let (state, labels) = if matches!(request.command, reporter::Command::Project(_)) {
    inspect(&client, &issue_url, &labels_url, &settings.version, intent)
      .await
      .map_err(|error| {
        // Effects may have landed before their final read-back was lost.
        // Recovery must observe this same intent, not terminally discard it.
        if error.class == FailureClass::ProtocolFault {
          FailureClass::Transient.into()
        } else {
          error
        }
      })?
  } else {
    (state, labels)
  };
  let contains = |expected: &String| {
    labels
      .iter()
      .any(|label| label.name.to_lowercase() == expected.to_lowercase())
  };
  let matches = effects.add().iter().all(contains)
    && !effects.remove().iter().any(contains)
    && effects.state().is_none_or(|desired| desired == state);
  let receipt = reporter::ProjectionReceipt {
    effects: intent.effects.clone(),
    projection_id: intent.projection_id.clone(),
    resource_id: intent.resource.resource_id.clone(),
    item_id: intent.item_id.clone(),
    configuration_digest: intent.configuration_digest.clone(),
    mapping: intent.mapping.clone(),
    status: intent.status.clone(),
    matches,
  };
  match &request.command {
    reporter::Command::Observe(_) => Ok(reporter::Outcome::Observed(receipt)),
    reporter::Command::Project(_) if matches => Ok(reporter::Outcome::Applied(receipt)),
    _ => Err(FailureClass::Conflict.into()),
  }
}
async fn inspect(
  client: &octocrab::Octocrab,
  issue_url: &url::Url,
  labels_url: &url::Url,
  version: &str,
  intent: &reporter::ProjectionIntent,
) -> Result<(IssueState, Vec<Label>), WorkApiFailure> {
  let (value, _) = read(client, issue_url, version).await?;
  if value.get("pull_request").is_some() {
    return Err(FailureClass::Conflict.into());
  }
  let issue: Issue = serde_json::from_value(value).map_err(|_| FailureClass::ProtocolFault)?;
  if issue.id == 0 || issue.id.to_string() != intent.item_id || issue.number.to_string() != intent.display_id {
    return Err(FailureClass::Conflict.into());
  }
  let (value, headers) = read(client, labels_url, version).await?;
  // A bounded incomplete label set cannot establish a matching observation.
  if headers.contains_key("link") {
    return Err(FailureClass::ProtocolFault.into());
  }
  let labels: Vec<Label> = serde_json::from_value(value).map_err(|_| FailureClass::ProtocolFault)?;
  if labels.len() > 100
    || labels
      .iter()
      .any(|label| label.name.is_empty() || label.name.len() > 256 || label.name.chars().any(char::is_control))
  {
    return Err(FailureClass::ProtocolFault.into());
  }
  Ok((issue.state, labels))
}
